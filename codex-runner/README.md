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
codex-runner --codex-version   # the `codex` this runner would start, e.g. 0.164.0 (1.7.0)
```

No npm dependency: it drives the `codex` CLI the user already has.

## The agent's plan (1.10.2)

`turn/plan/updated` — the agent's task list, whole, each time it changes — leaves the runner as
`{"type":"plan_update","plan":[{"step","status"}],"explanation"}`, the app-server's own shape
(`status`: `pending` | `inProgress` | `completed`). ShvIA's Code mode shows it in the **Tasks**
tab (SHVIA-WEB `docs/FRONTEND/ABA-TAREFAS.md`). Until 1.10.2 the notification had no case in
`traduzirNotificacao` and never left the runner. The desktop shell forwards any event line, so
nothing else changes on the way.

## Steering the turn in flight (1.11.0)

`{"type":"steer","text":"..."}` on stdin, sent while a turn runs, goes INTO that turn through the
app-server's `turn/steer`: the model reads it before its next step instead of the message waiting
for the whole turn to end. It is aimed at the turn by id (`expectedTurnId`, from `turn/start`'s
answer), so a correction that arrives late is refused rather than landing in the next turn.

| Situation | What the runner does | Line to the host |
|---|---|---|
| a turn is running | `turn/steer` | `{"type":"steer_applied","text"}` |
| the server refuses it (the turn just ended; review/compact turns cannot be steered) | hands it back; does NOT run it — the host decides | `{"type":"steer_deferred","text"}` |
| no turn running | runs it as a normal message | (the normal turn events) |

A refused steer is **not** an `error` line: the desktop bridge reads `error` as "the turn is over",
and the turn may be fine. A steer sent right after the message waits for the turn's id instead of
being deferred. The Code-mode page puts a deferred steer at the front of its own queue: the page owns the
queue (its chips, the Run's accounting), and a turn the runner started on its own would reach it with none
of that. The page sends `steer` only when the shell declares `recursos.orientar`. Test:
`orientar.test.mjs` (the real runner against a fake app-server).

## 🔴 The guarantee, and the hole in it — read this before shipping the engine

> **Inside the sandbox's writable set, writes do NOT ask. Outside it, you get a card.**

⚠️ That sentence is deliberate and was corrected once. It is **not** "outside the
project": `workspace-write` includes `/tmp`, so a turn writes there with no card. The
fence is the writable set, and promising the project would promise a fence that does
not exist.

That is the entire promise, and it is narrower than the Claude engine's. It is stated
this way because it is what was measured, not what was hoped: **Codex has no "ask
about everything" mode.** Its own execpolicy decides what is trivially safe, and an
integrator cannot turn that off — `echo hello` ran with no card under three different
policies, including `askForApproval.granular` with every flag set.

So this engine has **one level, "Sandbox"**. There is no Manual/Edit/Auto, because
three labels over one behaviour would be the approval pill lying three different ways.
`--aprovacao` is accepted and ignored (the bridge passes it to every engine; refusing
to spawn over a flag that cannot change anything would break the spawn for no gain).

**One exception, per turn (1.9.0): a PLAN turn.** When the page's Approval is "Plan", the
user message carries `plano: true`, and that turn starts with `sandboxPolicy: readOnly`.
Codex can read and run commands that write nothing, and a write fails in the sandbox. The
next turn gets back the sandbox `thread/start` reported (`sandboxDoTurno` in
`protocolo.mjs`). Unlike Manual/Edit/Auto this is a real difference in behaviour, which is
why it exists here. Proof: `plano.test.mjs`, against a fake app-server that records every
`turn/start`.

### Against `claude-runner`'s Auto, honestly

Read from `decidir()` in `claude-runner/politica.mjs`, not from memory (the line numbers this
sentence once gave have moved):

| | claude-runner Auto | this engine |
|---|---|---|
| write inside the project, an ordinary file | no card | 🔬 no card — **the same** |
| write of `.env`, an instruction file (`CLAUDE.md`, `AGENTS.md`) or Claude Code's settings inside the project | `always` card through the file tools since 1.36.6; none through `Bash` | no card, as for any write inside the project (not measured by file name) |
| write outside the project, or the writable set | `always` card through the file tools since 1.36.6; none through `Bash` | 🔬 **card**, and the rejection holds |
| destructive `Bash` (`rm -rf`) | always a card | 🔬 **card** — Codex gates it itself |
| network egress | always a card | 🔬 **no card — blocked**: it runs sandboxed and fails on DNS |
| read of `.env` inside the project | always a card | 🔬 **no card** |

Three rows deserve reading twice. "Codex writes without asking" is true of an ordinary file
in the project and is **also true of the Claude engine on Auto**; it is not true of `.env`, an
instruction file or Claude Code's settings, which the Claude engine asks about (through its
file tools, since 1.36.6) and this engine does not, because inside the project it takes a write
for a write whatever the file is called. A write through a shell command (`echo x > CLAUDE.md`)
is fenced by neither: the Claude engine's Auto runs it with no card too (see "What `auto` does
not cover" in `claude-runner/README.md`). And network is not gated here, it is *blocked*: nothing
leaves, but nobody is asked either.

🔴 **The real gap is the last row.** `claude-runner` gates a `.env` read at every
level via `caminhoProibido()`; Codex reads it and hands back the secret. This runner
**cannot** close that: it learns of a command from `item/started`, which arrives once the
command has begun, and there is no execpolicy surface to force a prompt (measured — see
the doc). The second row is a narrower gap of the same kind, and has not been measured by file
name. Anyone shipping this engine is accepting both knowingly.

## External sandbox mode (1.28.0)

Where a container is the boundary and Codex's own sandbox cannot run inside it, the host sets
`SHVIA_CODEX_SANDBOX_EXTERNO=<an absolute directory it guarantees read-only>`. Every turn then
starts with `externalSandbox` (network `restricted`), the startup ruler proves a write into that
directory is refused, and a plan turn is refused because its read-only sandbox does not run
there. The default mode is unchanged. Contract and measurements:
[MOTOR-CODEX-20260909.md](../docs/code/MOTOR-CODEX-20260909.md#external-sandbox-1280); test:
`externo.test.mjs`.

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

If the app-server rejects `turn/start`, the runner reports its error once, emits
`turn_done` for that host message and processes the next queued message. It does not
wait for a completion notification from a turn that never started or retry the
rejected message. `ciclo.test.mjs` checks this with the real runner and a local
app-server stand-in; it does not require account access or an inference request.

## Layout

| file | role |
|---|---|
| `protocolo.mjs` | the pure translation (policy, notifications, gates, decisions). Everything testable lives here |
| `protocolo.test.mjs` | `node --test protocolo.test.mjs` |
| `codex-runner.mjs` | the process: spawn, framing, gate queue, startup ruler, `--version`, `--codex-version`, `--modelos`, `--uso` |
| `plataforma.mjs` | which `codex` to start on each OS (`resolverCodex`: `SHVIA_CODEX_BIN`, PATH, the usual install dirs, the npm shim on Windows) |
| `uso.mjs` | the account's plan limits for the Usage tab (`--uso`), in the same shape as `claude-runner/uso.mjs` |
| `esquema.mjs` + `schemas/` | every outgoing payload is validated against Codex's own generated schema before it goes on the wire |
| `install.sh`, `install.ps1` | install (Linux/macOS, Windows) + a load proof, so a missing file fails here and not on the user's first turn |
| `*.test.mjs` | `npm run prova:politica` runs them all: protocol, catalogue, turn cycle, external mode, plan, steering, platform, usage |

⚠️ **Green unit tests are not evidence about this protocol.** Five defects — including
a turn that ended on the ack, a usage counter stuck at zero and a first card that hung
forever — lived through a green suite, because the tests asserted the shape the author
invented using data invented in that same shape. The record is
`docs/code/MOTOR-CODEX-20260909.md`; the rule that came out of it is that payloads get
validated against the generated schemas before being sent — and on its first run that
validator caught a sixth: `sandboxMode`, a field that does not exist in
`ThreadStartParams`, sent since the runner's first line and discarded in silence. **A
wrong field does not fail; it vanishes** — and every measurement taken before that fix
described Codex's default rather than this engine.
