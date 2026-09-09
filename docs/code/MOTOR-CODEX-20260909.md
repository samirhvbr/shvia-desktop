# Codex as a third Code-mode engine — what was measured on 09/09/2026

> **Status: slice 1 (the runner) built and proven live; slices 2 and 3 not started.**
> The bridge (`code_bridge.rs`) does not know the `codex` engine yet, and the web UI
> still shows a two-state toggle. Nothing in this document is reachable from the
> product — it runs only from `codex-runner/` by hand.

Provenance is marked on every row, and the distinction is the point of the file:

| mark | meaning |
|---|---|
| 🔬 | **measured live** — command run, exchange captured, outcome reproduced |
| 📋 | **read from the schema** — the protocol says so; nobody exercised it |

🔴 **Why the marks matter here more than usual.** This engine was designed twice. The
first design said *"app-server preserves the per-action gate"* — inferred from 📋
evidence alone (the schema carries `execCommandApproval`, `applyPatchApproval` and
their decision enums). It was wrong: **a message existing in a protocol is not the
same as it being sent.** Codex decides when to ask, and one live turn refuted three
coherent schema readings.

## The interfaces, and why `exec` was rejected

| interface | per-action approval | verdict |
|---|---|---|
| `codex exec --json` | 🔬 none — one sandbox policy up front; the only mention of confirmation in its help is the flag that SKIPS it | rejected: an engine on it emits no card at all |
| `codex app-server --stdio` | 🔬 asks at the sandbox boundary, not per action | adopted |

🔬 The wire is **newline-delimited JSON-RPC**, no `Content-Length` framing —
measured by sending one `initialize` line and reading the answer, not assumed from
the LSP family (which frames with headers and would have hung forever).

## The gate: three configurations, one answer

| configuration | `echo hello` | verdict |
|---|---|---|
| `untrusted` + `read-only` | 🔬 **ran, no card** | Codex's execpolicy treats it as trusted |
| `granular` (every flag true) + `experimentalApi` | 🔬 **ran, no card** | bought nothing; costs a second experimental dependency |
| `on-request` + `workspace-write` | 🔬 **ran, no card** | same |

> **Codex has no "ask about everything" mode, and an integrator cannot add one.**

## 🔬 What the engine actually does — measured under its own policy

Every row below was run under **`sandbox: workspace-write` + `approvalPolicy: on-request`**,
which is what `POLITICA` sends. That qualifier is the whole point of the table: an
earlier version of it was measured while the runner sent `sandboxMode`, a field that
**does not exist** in `ThreadStartParams` (it is `sandbox`). The server discarded it in
silence, so those numbers described Codex's default, not this engine. Two rows flipped
when the field was fixed.

| attempt | result | note |
|---|---|---|
| write inside the project | 🔬 no card | same as `claude-runner` on Auto |
| write to `/tmp` | 🔬 no card | **`workspace-write` includes `/tmp`** — it is inside the writable set, so no boundary is crossed |
| write to `$HOME` | 🔬 **card**, rejection held, file never created | this is measurement M |
| `rm -rf` inside the project | 🔬 **card** (*"Você autorizou a exclusão de diretório…"*) | Codex gates destructive commands on its own |
| `cat .env` inside the project | 🔬 no card — the secret came back as text | **the one real gap** against `claude-runner`, whose `caminhoProibido()` gates this at every level |
| `curl https://example.com` | 🔬 no card — ran sandboxed and failed with `Could not resolve host` | **blocked, not gated.** Nothing leaves; nobody is asked |

🔴 **The boundary is not "outside the project" — it is "outside the sandbox's writable
set".** Those are different sentences and `/tmp` is the difference. Saying the first one
would promise a fence that does not exist.

⚠️ **`git push` was not measured separately.** It is network egress, and the row above
covers that shape; the doc does not claim more than was run.

✅ **Investigated and does NOT exist — the command does appear.** An earlier revision of
this file reported that a run had produced no `tool_call`/`tool_result`, and raised the
possibility of a command executing without showing in the Code timeline. **It was the
measuring script, not the runner.** Chased on 09/09/2026 against the raw wire
(`SHVIA_CODEX_DEBUG=1`): `item/started` arrives with `item.type=commandExecution`, the
runner's filter accepts exactly that type, and both `tool_call` and `tool_result` are
emitted with the id, the name and the arguments. The two runs that "showed" the absence
had been read through a summariser that only printed `gate_request` and `tool_call`, and
through a `grep` that had died of pattern complexity and returned empty — **empty output
was read as absence**.

The line stays instead of being deleted: removing it would invite the next person to
"discover" the same non-defect, and the correction is the useful half. Reading the Codex
engine's guarantee, this matters — a secret read is **not gated**, and it **is** recorded.
Those are different sentences and only the first one is a gap.

## The guarantee, stated with its hole

> **Inside the project: writes do not ask. At the boundary: card.**

📋→🔬 **Compared with `claude-runner`'s Auto level** (read from `politica.mjs:130-150`,
not from memory): Auto **also allows writes inside the project with no card**
(`nivel !== "manual" && edicao.has(toolName) → allow`). So on that specific point the
two engines match. Auto still gates two things Codex does not, **at every level**:

| | claude-runner Auto | Codex engine |
|---|---|---|
| write inside the project | no card | no card |
| network egress | 🔬 always a card | 🔬 **no card — blocked**: the command runs sandboxed and fails on DNS |
| destructive `Bash` (`rm -rf`) | 🔬 always a card | 🔬 **card** — Codex gates it on its own |
| read of a secret (`.env`) inside the project | 🔬 always a card | 🔬 **no card** — the gap |

Because of that, the Codex engine has **one level, "Sandbox"** — not Manual/Edit/Auto.
Three labels over one behaviour would be the approval pill lying three different ways.

## The defects this work produced, and what they share

Five, all in the runner, all found by running rather than reading:

| # | defect | how it presented |
|---|---|---|
| 1 | turn ended on the `turn/start` ack | one line of output, `exit 0` — success-shaped |
| 2 | `usage` read `params.usage` on `turn/completed`, which has no such field | `tokens: 0` every turn |
| 3 | error text read from `params.message`; it is nested in `params.error` | `unknown error` — the only copy of the diagnosis, discarded |
| 4 | any `error` ended the turn, including `willRetry: true` | a healthy turn **killed** mid-reconnect, blamed on the model |
| 5 | `msg.id && msg.decision` — the server numbers its requests **from zero** | first card of every session hangs forever; looks like a freeze |

> **The signature they share: the arithmetic was right and the SHAPE was invented.**
> Unit tests written by the same author, from the same assumption, stayed green
> through all five — they tested the shape I made up, with data I made up in that
> same shape. 🔴 **Slice 2 validates every payload against the 39 generated schema
> files before sending it.** That is not a preference; it is what these five cost.

## The startup ruler, and the two corrections it needed

`provarQueOSandboxSegura()` runs once per spawn: it asks `command/exec` (which runs
"in the server sandbox without creating a thread or turn" — no model call, no
inference cost) to write into `$HOME`, and **refuses to start** unless the write
fails. Reversion-proven 🔬: sandbox on → starts; `dangerFullAccess` → exits 3 with the
reason; restored → starts.

Both corrections came from the reversion proof failing, and both are worth keeping:

1. 🔴 **The first probe target was wrong.** It wrote to the workspace's parent, under
   `/tmp` — which `workspace-write` **allows by design**. The ruler passed with the
   sandbox turned off: always green, which is worse than absent because it looks like
   a guard. `$HOME` discriminates, because the user owns it and an OS permission error
   is impossible there.
2. 🔴 **The runner read the host before the probe finished.** `{"type":"exit"}` shut it
   down mid-probe, and a `{"type":"user"}` would have run a turn before the sandbox was
   ever proven. Host input is now wired only after the ruler passes. **A ruler the
   engine can outrun is not a ruler.**

## Environment note, not a defect of ours

🔬 `gpt-5.5` — the account's configured and `isDefault` model, listed as available by
`model/list` — returned **404 "does not exist or you do not have access to it"** from
`chatgpt.com/backend-api/codex/responses`, after having worked minutes earlier.
Catalog and backend disagreeing, intermittently. It affects the whole Codex CLI on
this machine, not just SHVIA; the workaround is `model = "gpt-5.3-codex-spark"`.

## What is deliberately NOT done

- Bridge and UI (slices 2 and 3) — they wait on Code-mode identity and persistence.
- Closing the `.env` gap **by gating**. The runner cannot: it learns of a command from
  `item/started`, which arrives when the command has already begun, and there is no
  execpolicy surface to force a prompt. What it does instead, since `1.4.35`, is **say
  so before the first turn**: it imports `caminhoProibido` from `claude-runner/politica.mjs`
  — the same list, never a copy — scans the project root and emits a `warn` naming the
  files. `warn`, not `exit`: a sandbox that does not hold breaks the engine's guarantee,
  while a `.env` in the folder is a normal project condition, and refusing to start over
  it would teach people to ignore the warning.
- `thread/resume` — the runner starts a fresh thread per process. The app-server
  supports resuming; nothing here uses it yet.

## 🔬 Execpolicy: there is no surface for the integrator

Asked the running server for its config (`config/read`): the only policy-shaped keys are
`approval_policy`, `approvals_reviewer` and `shell_environment_policy`. In the binary,
`execpolicy` appears **only** inside the amendment flow
(`proposed_execpolicy_amendment` / `approved_execpolicy_amendment`) — Codex proposes an
amendment when it asks, and a client may accept it. There is no rules file to supply and
no way to force a prompt on a chosen command prefix.

So the `.env` gap cannot be closed by configuration. That "never" is now measured rather
than assumed.
