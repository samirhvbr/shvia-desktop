# Claude Code profile — ShvIA Desktop

This project's `.claude/` follows the Blue3/samirhvbr house pattern: effort level
and permissions posture. Stack: **Tauri 2 (Rust) + web shell (npm/Vite) + Node
runners (`claude-runner/`, `codex-runner/`)**; the Pentest engine is a binary built in
another repository, and there is no Python source here (the Python sidecar planned for
F2 was never built). The server is remote, so **no database runs here**; signing
credentials live on release machines, outside the tree.

## Files

| File | Role |
|---------|-------|
| `settings.json` | The **active** profile (versioned): `effortLevel: xhigh`, `defaultMode: plan`, the security **deny** and **ask** lists, and one `PostToolUse` hook. **It chooses no model.** |
| `launch.json` | Preview configuration: the Vite dev server of the local shell (`npm run dev`, port 1420), not the Tauri app |
| `prompts/` | The `loop-work` skill's Stop-hook templates (`continuacao.md`, `reabastecimento.md`) |
| `skills/` | Project skills (`impeccable`, `loop-work`), mirrored under `.agents/skills/` |

> **The model is not in this repository.** It is the user's choice, made with
> `/model`, per session; a subagent inherits the session's model. `settings.json`
> carries no `model`, no `fallbackModel`, no `availableModels`, and nothing in
> `env` that steers one (`ANTHROPIC_MODEL`, `ANTHROPIC_DEFAULT_*_MODEL`,
> `CLAUDE_CODE_SUBAGENT_MODEL`). There are no stand-by profiles to copy over
> `settings.json` to change the model — `/model` does that. See repodocs
> ADR-027.

> The **allow-list** (shortcuts that avoid repeated prompts) is **not** in
> `settings.json` on purpose: granting the agent a permission is **your** act.
> Apply the block below by hand when you want fewer prompts.

## Recommended allow-list (paste into `permissions.allow`)

```jsonc
"allow": [
  "Read", "Edit", "Write",
  "Bash(git status:*)", "Bash(git diff:*)", "Bash(git log:*)",
  "Bash(git show:*)", "Bash(git branch:*)",
  "Bash(git add:*)", "Bash(git commit:*)", "Bash(git push:*)",
  "Bash(node -c:*)", "Bash(node --check:*)",
  "Bash(npm run dev:*)", "Bash(npm run build:*)", "Bash(npm run tauri:*)",
  "Bash(npm install:*)", "Bash(npx tauri info:*)",
  "Bash(cargo check:*)", "Bash(cargo build:*)", "Bash(cargo test:*)",
  "Bash(cargo fmt:*)", "Bash(cargo clippy:*)",
  "Bash(python -m py_compile:*)", "Bash(python3 -m py_compile:*)",
  "Bash(pytest:*)", "Bash(bats:*)", "Bash(shellcheck:*)"
]
```

## Rules worth remembering

- **Effort `max` goes through the env** (`CLAUDE_CODE_EFFORT_LEVEL=max`). The
  JSON `effortLevel` field only accepts `low/medium/high/xhigh` — `max` there is
  ignored.
- **The context window comes with the model the user picked**, not with this
  repository: no model variable and no window flag is set here.
- **`defaultMode: plan`** — the agent plans before acting. It keeps the habit of
  reviewing structural changes before touching code.

## Deny and ask lists (as `settings.json` has them since 1.6.55)

- **deny:** reading key files (`*.pem`, `*.key`, `*.p8`, `*.p12`, `*.pfx`) anywhere in the tree.
- **ask:** `rm -rf` and piping `curl`/`wget` into `sh`/`bash`.

1.6.55 aligned the lists with repodocs: five commands moved from deny to ask and seven rules
left deny. The CHANGELOG entry of that version records which and why. **Do not loosen them**
without a documented reason.

## Hook

`PostToolUse` on `Edit|Write|MultiEdit` runs the Impeccable skill's `scripts/hook.mjs` when
that file exists (guarded by `-f`, so a clone without the skill is unaffected), with a 5 s
timeout.
