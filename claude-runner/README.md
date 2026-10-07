# claude-runner — motor "Claude Code (assinatura)" do Modo Code

Motor **paralelo** do Modo Code que orquestra o **cliente oficial** (Claude Agent
SDK) usando a **assinatura Pro/Max do usuário** e fala o **mesmo NDJSON do `anna`**
(`SHVIA-CODE/docs/embedding.md`). Por falar o mesmo protocolo, é **drop-in** atrás
do `code_bridge.rs` — o bridge e a UI de cards não mudam. O `code_bridge.rs`
escolhe o motor pelo campo `engine` do `spawn` (`engine:'claude'` → este runner;
ausente/`'gateway'` → `anna`).

## ⚠️ Fora do gateway

Este motor manda a inferência **direto do `claude` para a Anthropic** com a
assinatura do usuário — **sem passar pelo gateway do SHVIA**. Logo, **não há**
auditoria (`inference_requests`), quota, mascaramento LGPD nem medidor "Uso de
API" neste modo. É um **toggle paralelo** ("Motor: SHVIA gateway | Claude Code
assinatura"), não um substituto do `anna`.

## Instalação

```bash
./install.sh            # → ~/.local/bin/claude-runner (padrão anna)
```

**You normally do not run this.** Since SHVIA-DESKTOP 1.13.0 the app installs this runner itself, at its start,
when the machine has none and has Node 18+ (and keeps it current, 1.7.1): ADR-040 in `docs/decisoes.md`.
`SHVIA_RUNNERS_AUTO=0` turns the first install off. The script is still what runs — the app, the "Instalar
runner" button and this command are the same installer.

Pré-requisitos: **Node 18+** e o **Claude Code oficial autenticado**
(`claude login` ou `claude setup-token`). A assinatura é usada; **sem API key**.
O runner remove `ANTHROPIC_API_KEY` do próprio processo para garantir o fallback
à assinatura.

## Protocolo

Igual ao `anna` (`embedding.md`):

- **stdin**: `{"type":"user","text":…}` · `{"type":"exit"}` · `{"id":…,"decision":"approve"|"always"|"reject"}` · `{"id":…,"decision":"continue"|"stop","message":…}` (answer to a `stop_request`)
- **stdout**: `model` · `text{delta}` · `tool_call` · `tool_result` · `gate_request` · `stop_request` (only with `--parada host`) · `usage` · `turn_done` · `error`/`warn`

## Steering the turn in flight (1.11.0)

`{"type":"steer","text":"..."}`, sent while a turn runs, goes INTO it: `streamInput` on the turn's own query, as a
user message with `priority: "next"`, which the model reads before its next request. The runner answers
`{"type":"steer_applied","text"}` only when the main conversation starts that next request (a `message_start`
without `parent_tool_use_id`): `streamInput` resolving does not prove the model read it, and a correction that lands
while the model writes its last message never is. Still unconfirmed when the turn ends, it goes back to the host,
once, with `{"type":"steer_deferred","text"}`, and the runner does NOT run it: the host decides (the Code-mode page
puts it at the front of its own queue, where its turn bookkeeping lives). With no turn running it is a normal message. Same contract as the
Codex runner (`turn/steer`). Rules and the real-SDK measurement: `orientacao.mjs`; test: `orientacao.test.mjs`.

## The Run (1.5.0) — `--parada host`, `--teto-iteracoes`, `--teto-custo`

Block B2 of `docs/code/RUN-20260910.md`. With `--parada host` the runner installs the SDK
`Stop` hook: when the model wants to end a turn, the runner emits a `stop_request`
(`id`, `iteration`, `last`, `reason`), **blocks**, and does what the host answers —
`continue` keeps the model going in the same context with `message` as its reason;
`stop` ends the turn as always. No answer in 60 s or stdin closed → `stop`. The runner
never decides; the page does, on behalf of the server's orchestrator (ADR-034).

`--teto-custo X` goes to the SDK as `maxBudgetUsd`, **added to what the session had already cost**
when the turn opened (the SDK counts the session's cost against the budget, and Claude Code restores
it on `resume`: against the bare cap a session past it could not run another turn). A hit cap comes
back as `warn` + `turn_done`, never as a silent end. Without the flags nothing changes from 1.4.x.

`--teto-iteracoes N` is **not** an SDK cap since 1.13.2. It is the number of the orchestrator's
rounds, which the page counts and enforces at every stop; as `maxTurns` it counted the agent's own tool
rounds, and a Run — one held-open turn — ended on its second iteration with *"Reached maximum number
of turns (100)"*. The desktop still passes the flag; the runner ignores it.

**The `usage` event carries the turn's cost, not the session's.** The SDK's `total_cost_usd` is the cost
of the session so far; the runner reports `cost` as the difference from the last result (a total
below the last one is a new session), `cost_session` as the running total, and marks the event
`cost_scope: "turn"`. A page that does not know the marker treats `cost` as it always did — as the
session's — and subtracts it itself (SHVIA-WEB 2.111.108 and later).

Module: `parada.mjs`, proved by `parada.test.mjs` (`node --test claude-runner/`) and, for the event
translation, by `npm run prova:runner`.

## Permissão (a regra "nada roda/escreve sem o dev ver")

A política é um **PreToolUse hook + `settingSources: []`** (o hook é a autoridade
única e bypassa as allow-rules; o settings-vazio impede o `~/.claude/settings.json`
pessoal do usuário de auto-aprovar). Leitura (`Read`/`Glob`/…) = auto; o resto vira
`gate_request` (bloqueia) → decisão do card → `allow`/`deny`. O `gate_request.id`
é o `tool_use_id` real (casa com o `tool_result`).
