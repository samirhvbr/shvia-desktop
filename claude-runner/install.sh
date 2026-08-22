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

# O lock do DEST é apagado DE PROPÓSITO — é a correção de 21/08. O catálogo de
# modelos do Modo Code vem do Agent SDK (`--modelos` → `supportedModels()`), então
# a VERSÃO DO SDK É O CATÁLOGO. Com o lock de uma instalação velha no destino, o
# `npm install` respeitava o pin em vez do `^`, e o seletor seguia oferecendo
# "Opus" = Opus 4.8 semanas depois do Opus 5 existir. Perguntar ao SDK para não
# manter cópia que envelhece calada (ver code_bridge.rs) não adianta se a cópia
# que envelhece é o próprio SDK. Não há build reproduzível a proteger: o runner
# não viaja no instalador (só o `anna` viaja), é instalação local do dono da máquina.
rm -f "$DEST/package-lock.json"
( cd "$DEST" && npm install --omit=dev --no-audit --no-fund )

# O caminho do node é GRAVADO na instalação, com fallback para o PATH.
# Motivo: quem chama este wrapper é o app de GUI, e app de GUI não herda o PATH
# do shell (ADR-030) — no macOS o launchd entrega /usr/bin:/bin:/usr/sbin:/sbin e
# o `exec node` morria com "node: not found", que na tela virava o enganoso
# "claude-runner não encontrado" (caso real de 20/08). Se o node mudar de lugar
# depois (upgrade do Homebrew, troca de gerenciador de versão), cai no PATH.
NODE_ABS="$(command -v node)"

cat > "$BIN/claude-runner" <<EOF
#!/bin/sh
NODE="$NODE_ABS"
[ -x "\$NODE" ] || NODE=node
exec "\$NODE" "$DEST/claude-runner.mjs" "\$@"
EOF
chmod +x "$BIN/claude-runner"

# A versão do SDK vai na tela porque ELA é o catálogo de modelos: quando o
# seletor não oferece um modelo que já existe, é este número que responde por quê.
SDK="$(node -p "require('$DEST/node_modules/@anthropic-ai/claude-agent-sdk/package.json').version" 2>/dev/null || echo '?')"
echo "✓ claude-runner instalado em $BIN/claude-runner (Agent SDK $SDK)"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "⚠️  adicione $BIN ao PATH para o app encontrar o runner." ;;
esac
if ! command -v claude >/dev/null 2>&1; then
  echo "⚠️  o CLI 'claude' não está no PATH — instale o Claude Code e rode 'claude login'."
fi
