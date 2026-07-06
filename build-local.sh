#!/usr/bin/env bash
# build-local.sh — Build LOCAL do ShvIA Desktop no macOS e Linux (sem CI).
# Gera os instaladores do app, replicando o que .github/workflows/build.yml faz
# nos runners macos-latest / ubuntu-latest. O ShvIA é shell fino: SEM sidecar.
#   macOS  -> .dmg + .app.tar.gz
#   Linux  -> .deb + .AppImage (+ .rpm)   (targets="all" do tauri.conf.json)
#
# PRÉ-REQUISITOS (instalar uma vez):
#   Comum:   Node 20, Rust (rustup default stable)
#   macOS:   xcode-select --install
#   Linux (Debian/Ubuntu):
#     sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
#       libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
#
# USO (na raiz do repo):
#   ./build-local.sh                 # build normal (todos os targets do SO)
#   ./build-local.sh --skip-npm-ci   # pula 'npm ci' (deps já instaladas)
#   ./build-local.sh --bundles deb   # só um target (deb|appimage|rpm|dmg|app)
#
# VM/SEM GPU: o app empacotado pode abrir com **janela em branco** se o WebKitGTK
# não tiver acesso à GPU (DRM). Para ABRIR aqui, rode com
# WEBKIT_DISABLE_DMABUF_RENDERER=1 (render por software). Não afeta o build, só a
# execução. Detalhes em docs/decisoes.md / .continue.
#
# Saída: src-tauri/target/release/bundle/
# Obs.: o 1º build compila o Rust inteiro (~minutos); os próximos são incrementais.
set -euo pipefail
cd "$(dirname "$0")"

# ── Cronômetro do build: tempo total (parede) + por etapa ───────────────────
# Mesmo padrão do SHVTERM/build-local.sh: o "built in Xs" do Vite/cargo é só
# UMA etapa interna. Aqui medimos o script INTEIRO (npm → versão → bundle),
# por etapa e no total, e imprimimos mesmo quando aborta por erro (trap EXIT).
# Serve p/ comparar Linux × macOS × Windows e achar em qual fase otimizar.
SECONDS=0
_BUILD_OS=$(uname -s); [ "$_BUILD_OS" = Darwin ] && _BUILD_OS=macOS
_PH_NAMES=(); _PH_TIMES=(); _PH_CUR=""; _PH_START=0
_fmt() {  # $1 = segundos -> "1h 02m 03s" / "4m 05s" / "37s"
  local t=$1
  if   [ "$t" -ge 3600 ]; then printf '%dh %02dm %02ds' $((t/3600)) $(((t%3600)/60)) $((t%60))
  elif [ "$t" -ge 60 ];   then printf '%dm %02ds' $((t/60)) $((t%60))
  else                         printf '%ds' "$t"; fi
}
step() {  # fecha a etapa anterior, abre a nova, e mostra o relógio corrente
  local now=$SECONDS
  if [ -n "$_PH_CUR" ]; then
    _PH_NAMES+=("$_PH_CUR"); _PH_TIMES+=($((now - _PH_START)))
  elif [ "$now" -gt 0 ]; then
    _PH_NAMES+=("preparação"); _PH_TIMES+=("$now")
  fi
  _PH_CUR="$1"; _PH_START=$now
  echo "==> [$(_fmt "$now")] $1"
}
_summary() {  # tabela final: cada etapa + TOTAL
  if [ -n "$_PH_CUR" ]; then
    _PH_NAMES+=("$_PH_CUR"); _PH_TIMES+=($((SECONDS - _PH_START))); _PH_CUR=""
  fi
  echo ""
  echo "⏱  tempo por etapa ($_BUILD_OS):"
  if [ "${#_PH_NAMES[@]}" -gt 0 ]; then
    local i
    for i in "${!_PH_NAMES[@]}"; do
      printf '     %8s  %s\n' "$(_fmt "${_PH_TIMES[$i]}")" "${_PH_NAMES[$i]}"
    done
  fi
  echo "     ────────"
  printf '     %8s  TOTAL\n' "$(_fmt "$SECONDS")"
}
_on_exit() {  # se abortar (exit != 0), ainda mostra quanto tempo rodou
  local code=$?
  if [ "$code" -ne 0 ]; then
    echo "" >&2
    echo "❌ build abortou após $(_fmt "$SECONDS")  ($_BUILD_OS, exit $code)" >&2
  fi
}
trap _on_exit EXIT

usage() { sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'; }

SKIP_NPM_CI=0
BUNDLES=""
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-npm-ci) SKIP_NPM_CI=1 ;;
    --bundles)     shift; BUNDLES="${1:-}" ;;
    -h|--help)     usage; exit 0 ;;
    *) echo "opção desconhecida: $1 (use --help)" >&2; exit 2 ;;
  esac
  shift
done

echo "==> ShvIA Desktop — build local ($_BUILD_OS)"

step "[1/3] dependências do frontend (npm ci)"
if [ "$SKIP_NPM_CI" -eq 0 ]; then
  npm ci
else
  echo "    (pulado: --skip-npm-ci)"
fi

step "[2/3] sincroniza versão (version.md -> manifests)"
npm run version:sync

# Limpa instaladores de builds anteriores (padrão SHVTERM): o bundle dir acumula
# .deb/.AppImage/.rpm de versões antigas (ex.: ShvIA_0.4.6 ao lado do 0.5.0).
# Instalar o errado faz o app rodar versão velha — só o artefato do build ATUAL
# deve sobrar na listagem final.
rm -rf src-tauri/target/release/bundle

step "[3/3] Tauri build"
if [ -n "$BUNDLES" ]; then
  npx tauri build --bundles "$BUNDLES"
else
  npx tauri build
fi

echo ""
echo "[OK] Instaladores em src-tauri/target/release/bundle/:"
find src-tauri/target/release/bundle -maxdepth 2 -type f \
  \( -name '*.deb' -o -name '*.AppImage' -o -name '*.rpm' -o -name '*.dmg' \
     -o -name '*.app.tar.gz' \) -exec ls -lh {} \; 2>/dev/null || true
_summary
