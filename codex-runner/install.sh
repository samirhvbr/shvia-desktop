#!/usr/bin/env bash
# Installs the Codex engine runner as `~/.local/bin/codex-runner`.
#
# Unlike `claude-runner`, this one has NO npm dependency: it drives the `codex`
# CLI that the user already has, over its app-server stdio. So there is no
# `npm ci` step and no `node_modules` — which also means the install cannot fail
# halfway through a dependency tree.
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/shvia-codex-runner"
BIN="$HOME/.local/bin"

command -v node >/dev/null 2>&1 || { echo "erro: Node 18+ não encontrado no PATH."; exit 1; }

mkdir -p "$DEST" "$BIN"
# Every file the runner imports. The Claude runner shipped broken once because
# this list missed a module, so the load proof below exists to catch exactly that.
cp "$DIR/codex-runner.mjs" "$DIR/protocolo.mjs" "$DIR/package.json" "$DEST/"

NODE_ABS="$(command -v node)"
cat > "$BIN/codex-runner" <<WRAP
#!/usr/bin/env bash
NODE="$NODE_ABS"
[ -x "\$NODE" ] || NODE=node
exec "\$NODE" "$DEST/codex-runner.mjs" "\$@"
WRAP
chmod +x "$BIN/codex-runner"

# Proof that the install LOADS, not just that files were copied. A missing module
# would otherwise only surface on the user's first turn, as a dead engine.
if ! ERRO="$("$BIN/codex-runner" --version 2>&1)"; then
  echo "🔴 a instalação ficou incompleta — o runner não responde --version:" >&2
  printf '%s\n' "$ERRO" | head -5 >&2
  echo "   (falta copiar algum arquivo do runner? veja o \`cp\` acima)" >&2
  exit 1
fi
echo "✓ codex-runner $ERRO instalado em $BIN/codex-runner"

case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "⚠️  adicione $BIN ao PATH para o app encontrar o runner." ;;
esac

if ! command -v codex >/dev/null 2>&1; then
  echo "⚠️  o CLI 'codex' não está no PATH — instale o Codex CLI e rode 'codex login'."
elif ! codex login status >/dev/null 2>&1; then
  echo "⚠️  o 'codex' não está autenticado — rode 'codex login'."
fi
