# `codex-runner` — the Codex engine of SHVIA's Code mode

Drives `codex app-server --stdio` and speaks the same NDJSON as `anna` and
`claude-runner` (`SHVIA-CODE/docs/embedding.md`), so it is drop-in behind the
bridge: only the producer of the stream changes.

Auth is whatever `codex login` stored — the user's ChatGPT subscription. This runner
never embeds a login and never sets `OPENAI_API_KEY`, because an API key would move
the user off their subscription and onto pay-per-token without saying so.

```bash
./install.sh          # copies to ~/.local/share/shvia-codex-runner + ~/.local/bin
codex-runner --version
```

No npm dependency: it drives the `codex` CLI the user already has.

## 🔴 The guarantee, and the hole in it — read this before shipping the engine

> **Inside the project, writes do NOT ask. At the sandbox boundary, you get a card.**

That is the entire promise, and it is narrower than the Claude engine's. It is stated
this way because it is what was measured, not what was hoped: **Codex has no "ask
about everything" mode.** Its own execpolicy decides what is trivially safe, and an
integrator cannot turn that off — `echo hello` ran with no card under three different
policies, including `askForApproval.granular` with every flag set.

So this engine has **one level, "Sandbox"**. There is no Manual/Edit/Auto, because
three labels over one behaviour would be the approval pill lying three different ways.
`--aprovacao` is accepted and ignored (the bridge passes it to every engine; refusing
to spawn over a flag that cannot change anything would break the spawn for no gain).

### Against `claude-runner`'s Auto, honestly

Read from `politica.mjs:130-150`, not from memory:

| | claude-runner Auto | this engine |
|---|---|---|
| write inside the project | no card | no card — **the same** |
| network egress | always a card | not measured; likely a boundary request, **unverified** |
| destructive `Bash` | always a card | none |
| read of a secret / outside the project | always a card | none |

The first row matters: "Codex writes without asking" is true, and it is **also true of
the Claude engine on Auto**. The rows below it are where they genuinely differ.

## The startup ruler

The runner **refuses to start** if the sandbox does not hold. Once per spawn it asks
`command/exec` — which runs in the server sandbox without creating a thread or turn,
so it costs no inference — to write into `$HOME`, and requires the attempt to fail.

Calibrated toward the alarm: an app-server that cannot run the probe (older Codex,
transport error) also refuses. *"I could not measure"* must never read as *"it is
clean"*.

⚠️ `$HOME` is the target for a measured reason. The first version probed the
workspace's parent under `/tmp`, which `workspace-write` **allows by design** — the
ruler passed with the sandbox turned off. A guard that is always green is worse than
no guard, because it looks like one.

## Diagnosing

`SHVIA_CODEX_DEBUG=1` dumps the raw JSON-RPC both ways to stderr. It exists because
an early failure surfaced as `unknown error` with the server's actual message thrown
away — a translation layer that cannot show its input is one nobody can debug.

## Layout

| file | role |
|---|---|
| `protocolo.mjs` | the pure translation (policy, notifications, gates, decisions). Everything testable lives here |
| `protocolo.test.mjs` | `node --test protocolo.test.mjs` |
| `codex-runner.mjs` | the process: spawn, framing, gate queue, startup ruler |
| `install.sh` | install + a load proof, so a missing file fails here and not on the user's first turn |

⚠️ **Green unit tests are not evidence about this protocol.** Five defects — including
a turn that ended on the ack, a usage counter stuck at zero and a first card that hung
forever — lived through a green suite, because the tests asserted the shape the author
invented using data invented in that same shape. The record is
`docs/code/MOTOR-CODEX-20260909.md`; the rule that came out of it is that payloads get
validated against the generated schemas before being sent.
