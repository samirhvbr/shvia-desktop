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

🔬 A write is a different story. Under `read-only` the agent did not ask — it
**refused and explained in prose** (*"este ambiente está com sandbox de arquivos em
modo somente leitura"*), and `prova.txt` was never created. Blocked, not gated.

### 🔬 Measurement M — the boundary DOES gate

Under `on-request` + `workspace-write`, asked to write outside the workspace root:

```
gate_request  id 0  scope Bash
  command: /bin/bash -lc "printf 'ESCAPE' > '…/FORA-DO-SANDBOX.txt'"
  why: "Posso escrever fora do diretório atual, mas isso fica fora do sandbox…"
→ rejected → tool_result empty → turn_done → file never created
```

That is the whole promise this engine can make, and it is the reason it ships at all.

## The guarantee, stated with its hole

> **Inside the project: writes do not ask. At the boundary: card.**

📋→🔬 **Compared with `claude-runner`'s Auto level** (read from `politica.mjs:130-150`,
not from memory): Auto **also allows writes inside the project with no card**
(`nivel !== "manual" && edicao.has(toolName) → allow`). So on that specific point the
two engines match. Auto still gates two things Codex does not, **at every level**:

| | claude-runner Auto | Codex engine |
|---|---|---|
| write inside the project | no card | no card |
| network egress (`WebFetch`/`WebSearch`) | 🔬 always a card | 📋 not measured — `workspace-write` disables network by default, so it likely surfaces as a boundary request; **unverified** |
| destructive `Bash` | 🔬 always a card | ❌ none — no notion of it |
| read of a secret / outside the project | 🔬 always a card | ❌ none |

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
- Network egress at the boundary — 📋 only. It should be measured before the doc
  claims it.
- `thread/resume` — the runner starts a fresh thread per process. The app-server
  supports resuming; nothing here uses it yet.
