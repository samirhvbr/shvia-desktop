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

usage() { sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; }

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

OS=$(uname -s); [ "$OS" = Darwin ] && OS=macOS
echo "==> ShvIA Desktop — build local ($OS)"

if [ "$SKIP_NPM_CI" -eq 0 ]; then
  echo "==> npm ci"
  npm ci
fi

echo "==> sincroniza versão (version.md -> manifests)"
npm run version:sync

echo "==> tauri build"
if [ -n "$BUNDLES" ]; then
  npx tauri build --bundles "$BUNDLES"
else
  npx tauri build
fi

echo "==> pronto. Instaladores em src-tauri/target/release/bundle/:"
find src-tauri/target/release/bundle -maxdepth 2 -type f \
  \( -name '*.deb' -o -name '*.AppImage' -o -name '*.rpm' -o -name '*.dmg' \
     -o -name '*.app.tar.gz' \) -exec ls -lh {} \; 2>/dev/null || true
