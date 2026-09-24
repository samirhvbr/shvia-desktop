# Claude Code profile — ShvIA Desktop

This project's `.claude/` follows the Blue3/samirhvbr house pattern: effort level
and permissions posture. Target stack: **Tauri 2 (Rust) + web shell (npm/Vite) +
Python sidecar (arrives in F2)** — the server is remote, so **no database and no
secret runs here**.

## Files

| File | Role |
|---------|-------|
| `settings.json` | The **active** profile (versioned): `effortLevel`, `defaultMode: plan` and the security **deny-list** only. **It chooses no model.** |

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

## Deny-list (already in `settings.json`)

It blocks reading `.env`/keys (`*.pem`/`*.key`/`*.p8`/`*.p12`/`*.pfx`),
`rm -rf`, `git push --force/-f`, `git reset --hard`, `git clean -fd` and
`curl|sh`/`wget|sh`. **Do not loosen it** without a documented reason.
