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
cp "$DIR/codex-runner.mjs" "$DIR/protocolo.mjs" "$DIR/esquema.mjs" "$DIR/package.json" "$DEST/"
mkdir -p "$DEST/schemas"

# 🔴 The schema is REGENERATED from the Codex that is actually installed, not copied
# from the repo. A vendored copy goes stale on the next Codex release, and a stale
# schema in a validator is the worst of both: it rejects fields that became valid and
# accepts ones that stopped being. The house rule is to keep the command, not the
# derived state — and here the command is one line.
#
# The repo copy is the FALLBACK, for an older `codex` without the generator. Which one
# was used is printed, because a validator running against an unknown vintage of the
# protocol is something the next person needs to know.
if codex app-server generate-json-schema --out "$DEST/schemas" >/dev/null 2>&1 \
   && [ -f "$DEST/schemas/codex_app_server_protocol.v2.schemas.json" ]; then
  echo "  schema: gerado do codex instalado ($(codex --version 2>/dev/null | head -1))"
else
  cp "$DIR/schemas/codex_app_server_protocol.v2.schemas.json" "$DEST/schemas/"
  echo "  ⚠️  schema: cópia do repositório — este codex não gera o schema, então a"
  echo "      validação de payload pode estar medindo uma versão diferente do protocolo."
fi

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
