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

# ── The new install is assembled BESIDE the old one, and replaces it only once it loads (1.38.13) ──
#
# Until 1.38.13 the files were copied over the installed runner and `npm ci` ran there. `npm ci`
# starts by deleting `node_modules`, so an update that lost the network halfway left the NEW runner
# files with no Agent SDK. The app decides whether to reinstall by comparing exactly those files and
# the version in `package.json` (`src-tauri/src/code_bridge/atualizacao.rs`): from the next start on
# they matched, the runner counted as up to date, `--version` still answered (it runs before the SDK
# import), and every Code turn died on that import until the next app release or a press of the
# install button.
#
# Now the copy, `npm ci` and the load proof all happen in "$STAGE". Only an install that passed the
# three is moved into place, and a failure before that leaves the previous runner as it was: still
# working, still at its old version, so the next start tries the update again. A first install that
# fails leaves no runner at all, and the next start tries it again too.
STAGE="$DEST.incoming"
OLD="$DEST.previous"
rm -rf "$STAGE"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE" "$BIN"
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
cp "$DIR/claude-runner.mjs" "$DIR/politica.mjs" "$DIR/parada.mjs" "$DIR/orientacao.mjs" "$DIR/uso.mjs" "$DIR/package.json" "$DIR/package-lock.json" "$STAGE/"

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
( cd "$STAGE" && npm ci --omit=dev --no-audit --no-fund )

# O caminho do node é GRAVADO na instalação, com fallback para o PATH.
# Motivo: quem chama este wrapper é o app de GUI, e app de GUI não herda o PATH
# do shell (ADR-030) — no macOS o launchd entrega /usr/bin:/bin:/usr/sbin:/sbin e
# o `exec node` morria com "node: not found", que na tela virava o enganoso
# "claude-runner não encontrado" (caso real de 20/08). Se o node mudar de lugar
# depois (upgrade do Homebrew, troca de gerenciador de versão), cai no PATH.
NODE_ABS="$(command -v node)"

# A versão do SDK vai na tela porque ELA é o catálogo de modelos: quando o
# seletor não oferece um modelo que já existe, é este número que responde por quê.
# ── A instalação é CONFERIDA antes de se declarar pronta ─────────────────────
#
# O ✓ abaixo dizia "instalado" tendo copiado um conjunto de arquivos incompleto,
# e quem descobria era o usuário, com uma mensagem que culpava o binário. Importar
# o módulo instalado exercita a cadeia inteira de imports locais: se faltar
# arquivo, falha aqui, com o nome do que falta.
#
# `</dev/null` (1.6.25): importing the runner RUNS it, and its top level opens `readline` on
# stdin, exiting only on EOF. From the app that is fine (`output()` gives a null stdin), but the
# error message tells people to run this script in a TERMINAL, where stdin never ends: the
# install finished and then hung on its own proof until Ctrl-D.
if ! ERRO="$("$NODE_ABS" --input-type=module -e "import('file://$STAGE/claude-runner.mjs')" </dev/null 2>&1 >/dev/null)"; then
  echo "🔴 a instalação ficou incompleta — o runner não carrega:" >&2
  printf '%s\n' "$ERRO" | head -5 >&2
  echo "   (falta copiar algum arquivo do runner? veja o \`cp\` acima)" >&2
  exit 1
fi

# The swap. Two renames in one folder, so the runner is missing only between them, and if the second
# one fails the old install goes back. What a killed run leaves behind is cleared by the next one:
# "$STAGE" at its start, "$OLD" here.
rm -rf "$OLD"
if [ -e "$DEST" ] || [ -L "$DEST" ]; then
  mv "$DEST" "$OLD"
fi
if ! mv "$STAGE" "$DEST"; then
  if [ -e "$OLD" ]; then mv "$OLD" "$DEST"; fi
  echo "🔴 não consegui pôr a instalação nova no lugar de $DEST; a anterior ficou como estava." >&2
  exit 1
fi
rm -rf "$OLD"

cat > "$BIN/claude-runner" <<EOF
#!/bin/sh
NODE="$NODE_ABS"
[ -x "\$NODE" ] || NODE=node
exec "\$NODE" "$DEST/claude-runner.mjs" "\$@"
EOF
chmod +x "$BIN/claude-runner"

SDK="$(node -p "require('$DEST/node_modules/@anthropic-ai/claude-agent-sdk/package.json').version" 2>/dev/null || echo '?')"
echo "✓ claude-runner instalado em $BIN/claude-runner (Agent SDK $SDK)"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "⚠️  adicione $BIN ao PATH para o app encontrar o runner." ;;
esac
if ! command -v claude >/dev/null 2>&1; then
  echo "⚠️  o CLI 'claude' não está no PATH — instale o Claude Code e rode 'claude login'."
fi
