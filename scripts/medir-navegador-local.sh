#!/usr/bin/env bash
# Measures the agent's browser on this machine (ADR-042) against the REAL app, not a stub: the
# built binary runs on a virtual display (Xvnc) inside a private D-Bus session and a throwaway
# HOME, so it touches neither the tray nor the settings of any ShvIA you have open. A fake ShvIA
# page (port 8766) calls the bridge exactly as the real page does; a fake carrier portal (8765)
# is what the agent browses; a robot presses Return or Escape on the native consent dialogs.
#
# It measures what the unit tests cannot: that a second window with its own profile opens on
# WebKitGTK, that a script's answer comes back through eval_with_callback, that a pop-up and a
# file: link do nothing, that nothing a person typed or a page hid reaches the answer, and that
# a login cookie survives an app restart. NOT measured here: macOS, Windows, a Wayland session,
# and a real site. Not part of CI (it needs Xvnc, xdotool and ImageMagick's `import`).
#
#   scripts/medir-navegador-local.sh            # uses src-tauri/target/debug/shvia-desktop
#   BIN=/path/to/shvia-desktop TELA=:97 scripts/medir-navegador-local.sh
set -uo pipefail
AQUI=$(cd "$(dirname "$0")" && pwd)
RAIZ=$(cd "$AQUI/.." && pwd)
FIX=$AQUI/navegador-local
BIN=${BIN:-$RAIZ/src-tauri/target/debug/shvia-desktop}
TELA=${TELA:-:97}
PORTA_VNC=${PORTA_VNC:-5997}
T=$(mktemp -d "${TMPDIR:-/tmp}/medir-navegador.XXXXXX")
PIDS=()

for ferramenta in Xvnc xdotool import dbus-run-session python3; do
  command -v "$ferramenta" >/dev/null || { echo "faltando: $ferramenta" >&2; exit 2; }
done
[ -x "$BIN" ] || { echo "binário não encontrado: $BIN (rode cargo build em src-tauri)" >&2; exit 2; }
[ -f "$RAIZ/dist/index.html" ] || { echo "falta dist/ (npm run build): a casca local do app" >&2; exit 2; }

limpar() { touch "$T/parar"; for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done; }
trap limpar EXIT

Xvnc "$TELA" -geometry 1280x800 -depth 24 -SecurityTypes None -localhost -rfbport "$PORTA_VNC" >"$T/xvnc.log" 2>&1 &
PIDS+=($!)
python3 -I "$FIX/servidor.py" "$T" >"$T/servidor.log" 2>&1 &
PIDS+=($!)
# The app's own shell (index.html) is served at the dev URL a debug build uses.
python3 -I -m http.server 1420 --bind 127.0.0.1 --directory "$RAIZ/dist" >"$T/casca.log" 2>&1 &
PIDS+=($!)
sleep 2
DISPLAY=$TELA "$FIX/robo-dialogos.sh" "$T" &
PIDS+=($!)

# rodar <cenario> <navegador true|false> <pasta HOME>
rodar() {
  local cenario=$1 nav=$2 H=$3
  mkdir -p "$H/.config/cloud.blue3.shvia" "$H/.local/share" "$H/.cache"
  printf '%s' "$cenario" >"$T/cenario.txt"
  printf '{"close_to_tray":false,"avisou":true,"aparelho":true,"navegador":%s}' "$nav" >"$H/.config/cloud.blue3.shvia/tray.json"
  printf '{"url":"http://127.0.0.1:8766"}' >"$H/.config/cloud.blue3.shvia/server.json"
  echo aceitar >"$T/modo.txt"
  echo "── cenário: $cenario (navegador=$nav)"
  HOME=$H XDG_CONFIG_HOME=$H/.config XDG_DATA_HOME=$H/.local/share XDG_CACHE_HOME=$H/.cache \
    DISPLAY=$TELA WEBKIT_DISABLE_DMABUF_RENDERER=1 LIBGL_ALWAYS_SOFTWARE=1 GDK_BACKEND=x11 \
    setsid dbus-run-session -- "$BIN" >"$T/app-$cenario.log" 2>&1 &
  local app=$!
  local i
  for i in $(seq 1 180); do
    grep -q "\"k\": \"FIM\", \"v\": \"$cenario\"" "$T/eventos.jsonl" 2>/dev/null && break
    kill -0 "$app" 2>/dev/null || { echo "o app saiu antes do fim (veja $T/app-$cenario.log)"; break; }
    sleep 1
  done
  kill -- -"$app" 2>/dev/null; sleep 1; kill -9 -- -"$app" 2>/dev/null
  wait "$app" 2>/dev/null
}

: >"$T/eventos.jsonl"
HOME_A=$T/home-a
# SO="completo cookie" runs only those (debugging); the final tally then needs all three.
SO=${SO:-completo cookie desligado}
for c in $SO; do
  case $c in
    completo) rodar completo true "$HOME_A" ;;
    cookie) rodar cookie true "$HOME_A" ;;
    desligado) rodar desligado false "$T/home-b" ;;
  esac
done

# Facts the page cannot report about itself: what the PORTAL saw happen.
python3 -I - "$T/eventos.jsonl" <<'PY'
import json, sys
ev = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
passou = [e['v'] for e in ev if e['k'] == 'PASSOU']
falhou = [e['v'] for e in ev if e['k'] == 'FALHOU']
def tem(k, v=None): return any(e['k'] == k and (v is None or e['v'] == v) for e in ev)
extra = [
  ('o portal recebeu o texto que a pessoa autorizou ("maria")',        tem('portal_usuario', 'maria')),
  ('o portal NUNCA recebeu texto na senha',                            not tem('portal_senha')),
  ('o portal NUNCA recebeu texto no token da API',                     not tem('portal_token')),
  ('o clique em Entrar chegou ao portal',                              tem('clique', 'entrar')),
  ('o clique recusado em Excluir conta NÃO chegou ao portal',          not tem('clique', 'excluir')),
  ('a pop-up (window.open) não abriu: a página vazia não foi carregada na primeira rodada', True),
  ('o cookie de login sobreviveu ao reinício do app',                  any(e['k'] == 'cookie_na_vazia' and 'sessao=abc123' in e['v'] for e in ev)),
]
# The pop-up check needs the order of events: no `cookie_na_vazia` before the second scenario.
pos_cookie = [i for i, e in enumerate(ev) if e['k'] == 'cookie_na_vazia']
pos_fim_completo = [i for i, e in enumerate(ev) if e['k'] == 'FIM' and e['v'] == 'completo']
extra[5] = (extra[5][0], bool(pos_fim_completo) and all(p > pos_fim_completo[0] for p in pos_cookie))
print()
for nome in passou: print('  ok     ', nome[:140])
for nome in falhou: print('  FALHOU ', nome[:400])
for nome, ok in extra:
    print('  ' + ('ok     ' if ok else 'FALHOU '), nome)
    if not ok: falhou.append(nome)
fins = [e['v'] for e in ev if e['k'] == 'FIM']
print('\ncenários concluídos:', fins)
total = len(passou) + len(extra)
print(f'{total - len(falhou)}/{total} verificações; falhas: {len(falhou)}')
sys.exit(1 if falhou or len(fins) != len(__import__('os').environ.get('SO', 'completo cookie desligado').split()) else 0)
PY
codigo=$?
echo "trabalho: $T (capturas dos diálogos: $T/dialogo-*.png)"
exit $codigo
