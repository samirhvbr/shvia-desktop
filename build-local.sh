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
#   ./build-local.sh --no-sign       # (macOS) NÃO assina/notariza — build de teste
#
# ASSINATURA (macOS): sem assinar, o macOS trata o app como "danificado" e oferece
# MOVER PARA A LIXEIRA quando ele é aberto depois de baixado/enviado (atributo de
# quarentena). Este script, no macOS, assina o app com o cert **Developer ID
# Application** do keychain e o `tauri build` **notariza + staple** sozinho quando
# há credencial de notarização. A senha de app (Apple ID) fica no **keychain**
# (serviço "shvia-notarize"), NUNCA no repo. Guardar uma vez:
#   security add-generic-password -U -s shvia-notarize -a SEU_APPLE_ID -w
# (pede a senha de app escondida — gere em appleid.apple.com › Senhas de app).
# Sem cert -> build sai sem assinar; sem credencial -> assina mas não notariza.
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

usage() { awk 'NR>1{ if($0=="set -euo pipefail") exit; sub(/^# ?/,""); print }' "$0"; }

# ── Preflight: checa o toolchain ANTES dos passos lentos ────────────────────
# Falha em <1s com mensagem ACIONÁVEL (ex.: "instale o Rust: curl … rustup.rs")
# em vez do erro críptico "cargo metadata: No such file or directory" 5s adentro.
# Coleta TODOS os faltantes de uma vez. Recupera o cargo de ~/.cargo/bin se ele
# existe mas não está no PATH (caso comum: rustup instalado, mas o terminal novo
# não recarregou o PATH).
preflight() {
  local missing=()

  command -v node >/dev/null 2>&1 || missing+=(
    "Node.js não encontrado. Instale o Node 20+ (https://nodejs.org, 'brew install node' ou nvm)."
  )
  command -v npm >/dev/null 2>&1 || missing+=(
    "npm não encontrado (vem com o Node)."
  )

  # Rust: se o cargo não está no PATH mas o rustup instalou em ~/.cargo/bin,
  # puxa pro PATH desta execução e avisa — não obriga a reabrir o shell.
  if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
    echo "    (cargo achado em ~/.cargo/bin — adicionado ao PATH desta execução; pra"
    echo "     fixar, rode 'source \$HOME/.cargo/env' ou reabra o terminal)"
  fi
  if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
    missing+=(
"Rust (cargo) não encontrado — é o que o Tauri usa pra compilar.
       Instale:  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
       Depois:   source \"\$HOME/.cargo/env\"   (ou reabra o terminal)"
    )
  fi

  # macOS: o toolchain da Apple (clang/linker) vem do Command Line Tools do Xcode.
  # (o próprio rustup precisa dele pra linkar — checar junto evita erro na 2ª tentativa.)
  if [ "$_BUILD_OS" = macOS ] && ! xcode-select -p >/dev/null 2>&1; then
    missing+=(
"Command Line Tools do Xcode ausentes (clang/linker do macOS).
       Instale:  xcode-select --install"
    )
  fi

  # Linux (Debian/Ubuntu): o WebKitGTK dev é obrigatório pro WebView do Tauri.
  if [ "$_BUILD_OS" = Linux ] && command -v pkg-config >/dev/null 2>&1 \
     && ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    missing+=(
"libwebkit2gtk-4.1-dev ausente (e outras deps do Tauri no Linux). Instale:
       sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \\
         libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf"
    )
  fi

  if [ "${#missing[@]}" -gt 0 ]; then
    echo "" >&2
    echo "❌ pré-requisitos faltando ($_BUILD_OS) — corrija e rode de novo:" >&2
    echo "" >&2
    local m
    for m in "${missing[@]}"; do
      echo "   • $m" >&2
      echo "" >&2
    done
    exit 1
  fi
}

# ── macOS: assinatura (Developer ID) + notarização (senha de app) ────────────
# Sem isto, um app baixado/enviado (com quarentena) é rejeitado pelo Gatekeeper e
# o macOS oferece MOVER PARA A LIXEIRA. Fluxo:
#   1) acha o cert "Developer ID Application" no keychain e exporta
#      APPLE_SIGNING_IDENTITY — o `tauri build` assina o .app (hardened runtime).
#   2) se houver credencial de notarização (senha de app no keychain, serviço
#      "shvia-notarize"), exporta APPLE_ID/APPLE_PASSWORD/APPLE_TEAM_ID — aí o
#      `tauri build` NOTARIZA e faz STAPLE do .app automaticamente.
# Segredo NUNCA entra no repo: a senha de app mora só no keychain deste Mac.
NOTARY_SERVICE="shvia-notarize"
SIGN_ENABLED=0
NOTARIZE_ENABLED=0

setup_macos_signing() {
  [ "$_BUILD_OS" = macOS ] || return 0
  if [ "${NO_SIGN:-0}" -eq 1 ]; then
    echo "    (--no-sign: build de teste, SEM assinar/notarizar)"
    return 0
  fi

  # 1) Identidade de assinatura: 1ª "Developer ID Application" do keychain,
  #    salvo se APPLE_SIGNING_IDENTITY já vier do ambiente.
  if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
    APPLE_SIGNING_IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
      | awk -F'"' '/Developer ID Application/{print $2; exit}' || true)"
  fi
  if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
    echo "    ⚠️  sem cert 'Developer ID Application' no keychain — BUILD SAI SEM ASSINAR."
    echo "        (o macOS vai oferecer 'mover p/ lixeira' ao abrir o app baixado)"
    return 0
  fi
  export APPLE_SIGNING_IDENTITY
  SIGN_ENABLED=1
  echo "    ✔ assinatura: $APPLE_SIGNING_IDENTITY"

  # Team ID: extrai o (XXXXXXXXXX) do fim da identidade, salvo se já vier do ambiente.
  if [ -z "${APPLE_TEAM_ID:-}" ]; then
    APPLE_TEAM_ID="$(printf '%s' "$APPLE_SIGNING_IDENTITY" \
      | sed -n 's/.*(\([A-Z0-9]\{10\}\))$/\1/p')"
  fi

  # 2) Credencial de notarização (senha de app) do keychain, salvo se já vier do ambiente.
  if [ -z "${APPLE_PASSWORD:-}" ]; then
    APPLE_PASSWORD="$(security find-generic-password -s "$NOTARY_SERVICE" -w 2>/dev/null || true)"
  fi
  if [ -z "${APPLE_ID:-}" ]; then
    APPLE_ID="$(security find-generic-password -s "$NOTARY_SERVICE" 2>/dev/null \
      | awk -F'"' '/"acct"/{print $4}' || true)"
  fi

  if [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
    export APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID
    NOTARIZE_ENABLED=1
    echo "    ✔ notarização: $APPLE_ID (team $APPLE_TEAM_ID) — tauri build vai notarizar+staple"
    echo "      (a notarização sobe o app p/ a Apple e ESPERA — pode levar alguns minutos)"
  else
    echo "    ⚠️  vou ASSINAR mas NÃO notarizar (falta credencial). Guarde a senha de app:"
    echo "        security add-generic-password -U -s \"$NOTARY_SERVICE\" -a \"SEU_APPLE_ID\" -w"
    echo "        (sem notarizar, o app abre mas ainda pede liberação em Ajustes → Privacidade)"
  fi
}

# Verificação pós-build: prova que o .app/.dmg ficaram aceitáveis ao Gatekeeper.
verify_macos_signature() {
  [ "$_BUILD_OS" = macOS ] || return 0
  [ "$SIGN_ENABLED" -eq 1 ] || { echo "    (build sem assinatura — nada a verificar)"; return 0; }

  local app dmg
  app="$(find src-tauri/target/release/bundle/macos -maxdepth 1 -name '*.app' 2>/dev/null | head -1 || true)"
  dmg="$(find src-tauri/target/release/bundle/dmg  -maxdepth 1 -name '*.dmg' 2>/dev/null | head -1 || true)"
  if [ -z "$app" ]; then echo "    (sem .app p/ verificar)"; return 0; fi

  echo "  • codesign --verify (deep, strict):"
  if codesign --verify --deep --strict --verbose=2 "$app" >/tmp/_cs.txt 2>&1; then
    echo "      ✔ assinatura íntegra"
  else
    echo "      ❌ assinatura inválida:"; sed 's/^/        /' /tmp/_cs.txt
  fi

  echo "  • autoridade + hardened runtime:"
  codesign -dvvv "$app" 2>&1 \
    | grep -E 'Authority=|TeamIdentifier=|Identifier=|flags=' | sed 's/^/      /' || true

  echo "  • Gatekeeper (spctl assess):"
  spctl -a -t exec -vvv "$app" 2>&1 | sed 's/^/      /' || true

  echo "  • staple (ticket de notarização anexado):"
  if xcrun stapler validate "$app" >/dev/null 2>&1; then
    echo "      ✔ .app com staple (abre offline, sem prompt)"
  else
    echo "      ⚠️  .app SEM staple — notarização não rodou/falhou (ainda pede liberação)"
  fi
  if [ -n "$dmg" ]; then
    if xcrun stapler validate "$dmg" >/dev/null 2>&1; then
      echo "      ✔ .dmg com staple"
    elif [ "$NOTARIZE_ENABLED" -eq 1 ]; then
      echo "      • .dmg ainda sem staple — notarizando o próprio .dmg (submit + staple)…"
      if xcrun notarytool submit "$dmg" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" \
             --team-id "$APPLE_TEAM_ID" --wait 2>&1 | sed 's/^/        /' \
         && xcrun stapler staple "$dmg" 2>&1 | sed 's/^/        /'; then
        echo "      ✔ .dmg notarizado + stapled"
      else
        echo "      ⚠️  não notarizei o .dmg — mas o .app dentro dele já está"
        echo "          notarizado+stapled, então distribuir o .dmg funciona mesmo assim."
      fi
    fi
  fi
}

SKIP_NPM_CI=0
NO_SIGN=0
BUNDLES=""
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-npm-ci) SKIP_NPM_CI=1 ;;
    --no-sign)     NO_SIGN=1 ;;
    --bundles)     shift; BUNDLES="${1:-}" ;;
    -h|--help)     usage; exit 0 ;;
    *) echo "opção desconhecida: $1 (use --help)" >&2; exit 2 ;;
  esac
  shift
done

echo "==> ShvIA Desktop — build local ($_BUILD_OS)"

step "[pré-requisitos] verifica o toolchain (Rust, Node, Xcode/WebKitGTK)"
preflight

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

if [ "$_BUILD_OS" = macOS ]; then
  step "[macOS] assinatura + notarização (Developer ID + notarytool)"
  setup_macos_signing
fi

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

if [ "$_BUILD_OS" = macOS ]; then
  step "[macOS] verificação (codesign / spctl / stapler)"
  verify_macos_signature
fi
_summary
