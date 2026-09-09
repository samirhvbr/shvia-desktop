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
# ⚠️ Toda dependência LOCAL do runner entra aqui. O `politica.mjs` entrou na 1.4.7
# (03/09) como `import ... from "./politica.mjs"` no `claude-runner.mjs`, e esta
# linha não foi acompanhada: desde então toda instalação nova terminava com o ✓ de
# sucesso e morria na primeira chamada com `ERR_MODULE_NOT_FOUND`, que a tela
# mostra como "claude-runner não encontrado" — a mesma mensagem de binário
# ausente, num caso em que o binário está lá.
#
# A régua que impede a repetição é o `--verificar` abaixo: ele importa o módulo
# instalado, então um import local novo que não for copiado reprova AQUI, no
# instalador, em vez de reprovar na tela de alguém.
cp "$DIR/claude-runner.mjs" "$DIR/politica.mjs" "$DIR/package.json" "$DIR/package-lock.json" "$DEST/"

# ── The lock is VERSIONED and installed with `npm ci` (finding F-18) ─────────
#
# ## What the previous decision was, and why it was right at the time
#
# From 21/08 this script did `rm -f package-lock.json` before `npm install`. The reasoning
# was sound: the Modo Code catalogue comes from the Agent SDK (`--modelos` →
# `supportedModels()`), so THE SDK VERSION IS THE CATALOGUE. A stale lock in the destination
# made `npm install` honour the pin instead of the `^`, and the selector kept offering
# "Opus" = Opus 4.8 weeks after Opus 5 existed.
#
# ## Why it is being reversed, with the measurement
#
# Deleting the lock did not deliver freshness — it moved the staleness. Measured on 02/09:
# this machine had the SDK at **0.3.239** while a fresh resolve of the same `^0.3.239` gives
# **0.3.258**. Nineteen patch releases apart. The installed copy froze at whatever was latest
# the last time somebody happened to reinstall, which is the very failure the deletion was
# meant to prevent.
#
# So the tradeoff was real but the sides were mislabelled. What ages is not "having a lock",
# it is a lock **nobody updates** — and a floating range is just a lock nobody can see.
#
# With `npm ci` the install is reproducible and reviewable, in a component that executes
# tools on the developer's machine. Refreshing the catalogue becomes a deliberate act:
#
#     npm run runner:sdk-bump     # updates the SDK + lock and prints the new catalogue
#
# That is one command, in the repository, visible in a diff — instead of an invisible
# resolution that differs per machine and per day.
( cd "$DEST" && npm ci --omit=dev --no-audit --no-fund )

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
# ── A instalação é CONFERIDA antes de se declarar pronta ─────────────────────
#
# O ✓ abaixo dizia "instalado" tendo copiado um conjunto de arquivos incompleto,
# e quem descobria era o usuário, com uma mensagem que culpava o binário. Importar
# o módulo instalado exercita a cadeia inteira de imports locais: se faltar
# arquivo, falha aqui, com o nome do que falta.
if ! ERRO="$("$NODE_ABS" --input-type=module -e "import('file://$DEST/claude-runner.mjs')" 2>&1 >/dev/null)"; then
  echo "🔴 a instalação ficou incompleta — o runner não carrega:" >&2
  printf '%s\n' "$ERRO" | head -5 >&2
  echo "   (falta copiar algum arquivo do runner? veja o \`cp\` acima)" >&2
  exit 1
fi

SDK="$(node -p "require('$DEST/node_modules/@anthropic-ai/claude-agent-sdk/package.json').version" 2>/dev/null || echo '?')"
echo "✓ claude-runner instalado em $BIN/claude-runner (Agent SDK $SDK)"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "⚠️  adicione $BIN ao PATH para o app encontrar o runner." ;;
esac
if ! command -v claude >/dev/null 2>&1; then
  echo "⚠️  o CLI 'claude' não está no PATH — instale o Claude Code e rode 'claude login'."
fi
