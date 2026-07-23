#!/usr/bin/env bash
# Instala o claude-runner (motor "Claude Code assinatura" do Modo Code) no padrão
# do anna: copia p/ ~/.local/share/shvia-claude-runner, roda npm install e cria
# um wrapper executável em ~/.local/bin/claude-runner — que o code_bridge.rs
# resolve por PATH/local, igual ao `anna`.
#
# Pré-requisitos:
#   - Node 18+ no PATH.
#   - Claude Code oficial autenticado: `claude login` (ou `claude setup-token`).
#     A ASSINATURA Pro/Max do usuário é usada; o runner NÃO usa API key.
#
# ⚠️ Este motor sai do gateway do SHVIA (sem auditoria/quota/LGPD/medidor) —
#    é o toggle "Claude Code (assinatura)" do Modo Code, paralelo ao anna.
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/shvia-claude-runner"
BIN="$HOME/.local/bin"

command -v node >/dev/null 2>&1 || { echo "erro: Node 18+ não encontrado no PATH."; exit 1; }

mkdir -p "$DEST" "$BIN"
cp "$DIR/claude-runner.mjs" "$DIR/package.json" "$DEST/"
( cd "$DEST" && npm install --omit=dev --no-audit --no-fund )

cat > "$BIN/claude-runner" <<EOF
#!/bin/sh
exec node "$DEST/claude-runner.mjs" "\$@"
EOF
chmod +x "$BIN/claude-runner"

echo "✓ claude-runner instalado em $BIN/claude-runner"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "⚠️  adicione $BIN ao PATH para o app encontrar o runner." ;;
esac
if ! command -v claude >/dev/null 2>&1; then
  echo "⚠️  o CLI 'claude' não está no PATH — instale o Claude Code e rode 'claude login'."
fi
