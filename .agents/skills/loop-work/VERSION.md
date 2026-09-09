# loop-work — vendored copy

- **version:** 0.3.4
- **source:** `/home/samir/x/SKILLS/skill-LOOP` (commit `17c9aee`)
- **installed:** 2026-09-02

This directory is a **snapshot** of `skill/loop/` from the skill-LOOP repository.
Do not edit it here — change it upstream and re-vendor, otherwise the next update
overwrites your change.

## Layout

`hooks/loop-stop.py` resolves its prompt templates three levels up from
`skill/loop/`, which lands in `<repo>/.claude/prompts/`. That is why
`continuacao.md` and `reabastecimento.md` are vendored there alongside this
skill; the copy is broken without them.

## Not registered as a hook here

The `Stop` hook is registered **globally** (in the Claude Code settings of the
config dir in use) and points at the skill-LOOP repository. It is inert in any
repository without an active `.loop/STATE.json`, so there is nothing to register
per project — doing so would fire the hook twice.

## Updating

Re-run the vendoring from the skill-LOOP repository and bump the version above.
