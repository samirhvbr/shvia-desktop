#!/usr/bin/env bash
# .loop/loop.sh — start a timed round in this repository, then watch it.
#
#   ./.loop/loop.sh          arm for 6h and open the panel
#   ./.loop/loop.sh 10h      same, with a 10h clock  (6h | 90m | 2h30)
#
# Seeded by `loop-ctl armar` when absent, and NEVER overwritten: this copy is
# yours. Add --objetivo, --janela, --dias, --itens below and they survive every
# future `armar`. Delete the file and the next `armar` writes a fresh one.
#
# Under a clock an empty queue does not end the round — it becomes a refill
# turn (ADR-015). Declare the boundary in .loop/SCOPE.md; it goes verbatim into
# the refill prompt.
set -euo pipefail

# The root is derived, never written down: move the repo, clone it, rename it —
# this still points at the right tree. Do not replace it with a literal path.
RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DURACAO="${1:-6h}"

# Prefer whatever is on PATH (install.sh puts loop-ctl / loop-watch in
# ~/.local/bin); fall back to the skill copy that seeded this file, so the
# script works before the PATH is set up — and says which copy is serving.
CTL=(loop-ctl)
WATCH=(loop-watch)
command -v loop-ctl   >/dev/null 2>&1 || CTL=(python3 "@LOOP_CTL_PY@")
command -v loop-watch >/dev/null 2>&1 || WATCH=(python3 "@LOOP_WATCH_PY@")

# ⚠️ A shell does not know its own session_id, so this round binds to the FIRST
# session that ends a turn here — which may be a chat you left open for
# something else. That is exactly how a round in EOP adopted the wrong session
# on 2026-09-01: 18 journal entries filed under unrelated items, 4 spurious
# queue items, and two sessions driving the same tree (P-09). The adoption is
# asked for on purpose below; closing the other chats is the part only you can do.
cat >&2 <<AVISO
⚠  this round will adopt the FIRST session that stops in $RAIZ.
   Close or stop any other Claude session open on this repo before continuing.
AVISO

# Your flags. Uncomment what you want; they survive every future `armar`,
# because this file is never overwritten.
EXTRA=(
    # --objetivo "one line, reported at every stop"
    # --janela 08:00-18:00 --dias seg-sex
    # --itens 10
)

"${CTL[@]}" armar \
    --raiz "$RAIZ" \
    --duracao "$DURACAO" \
    --adotar-primeira-parada \
    ${EXTRA[@]+"${EXTRA[@]}"}

# No --ate-encerrar on purpose: a turn that dies without emitting `Stop` leaves
# the round pinned at ativo:true, and a script blocked on it would hang forever
# (P-08). Ctrl+C leaves the panel; the round keeps running.
exec "${WATCH[@]}" --raiz "$RAIZ"
