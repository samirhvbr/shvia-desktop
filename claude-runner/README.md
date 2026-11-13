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

## Approval levels, plan turns and the other flags

`--aprovacao manual|edit|auto` (default `manual`) mirrors `anna`'s three levels; the SDK's
`permissionMode` is not used, because the `PreToolUse` hook is what emits the `gate_request`
the page renders. `bypassPermissions` and `dontAsk` are never used (ADR-032). What each level
does is decided by `decidir()` in [`politica.mjs`](politica.mjs):

- **Every level:** a read inside the project folder runs without a card; a read outside it asks
  (`confirm`). Reading a protected path (the secret denylist shared with `anna`: `.env`, keys,
  `.ssh`, `.aws`, `.git`, `credentials`, …), network egress (`WebFetch`, `WebSearch`, and a `Bash`
  command `comandoDeRede()` recognizes as network), a command `comandoDestrutivo()` recognizes as
  destructive and a command in which `caminhoProtegidoNoComando()` finds a protected path always
  ask, with an `always` card the page never approves by itself. These are pattern checks over the
  command text; the functions and their tests are the exact contract.
- **Every level, the file-writing tools (1.36.6):** a write by `Write`, `Edit`, `MultiEdit` or
  `NotebookEdit` goes through the same fence as a read before any level releases it. A protected
  path, an agent instruction file (`ANNA.md`, `AGENTS.md`, `CLAUDE.md`, `CLAUDE.local.md`, as in
  `anna` 0.15.4) or Claude Code's `.claude/settings.json` / `settings.local.json`, and any path
  outside the project folder ask with an `always` card, by the requested name or by where a
  symlink really leads. A read outside the project whose card shows only a path inside it (through
  a symlink) asks as `always` too, because the page's check would read that card as inside.
  **This is the fence of those four tools and nothing more:** a `Bash` command is not a write to
  this policy, so at `auto` a shell can write what they cannot (see "What `auto` does not cover").
- **`edit`:** file edits inside the fence also run without a card.
- **`auto`:** what is not in the "always ask" set above runs without a card. The command is read
  the way the shell runs it (1.36.6, corrected in 1.42.6): quotes and backslashes resolved,
  `$(…)` and backticks judged as scripts of their own even inside double quotes and kept whole in
  the word they are part of (`X=$(echo a b) rm -rf V` is an assignment and then `rm`), the shell's
  keywords (`then`, `do`, `!`) skipped, and every word after a command that starts another command
  (`sudo`, `env`, `xargs`, `timeout`, `nice`, `find -exec`, …) judged as the command it may start.
  A comment is not read (`ls # list; rm -rf V` asks nothing), and a `<<` inside a comment, a
  `${…}` or a `((…))` opens no here-document. A here-document body is text unless the command it
  goes to runs it (a shell, `.`, `source`, `sudo -s`). A pipe into any shell asks, named or
  wrapped as it may be (`| zsh`, `| env bash`, `| "bash"`). A flag that the line completes when it
  runs (`-r$(echo f)`) counts as any flag. A line whose command name is decided when it runs (a
  variable, a substitution, a glob) or that uses `$'…'` asks as `always` (`sintaxeIlegivel()`).
  The checks are held against what they replaced: `politica-diferencial.test.mjs` runs about 800
  command lines through this policy and through the frozen 1.36.5 one, and a line that 1.36.5
  asked about must still ask unless the test names a reason.

### What `auto` does not cover

The level trusts the agent with the shell. These checks read the words of a command line: they
catch spellings, not intent, and they are a tripwire, not a sandbox. Each line below runs with no
card at `auto`. Every one was run by a real shell in a sandbox with `rm` and the like stubbed, and
the stub was called; `politica-diferencial.test.mjs` asserts each is still open and fails the day
one closes, so that this list is edited in the same commit.

- **`Bash` writes anything.** The fence above belongs to the four file-writing tools.
  `echo PWNED > CLAUDE.md` and `echo x >> ~/.bashrc` run with no card at `auto`, where `Write` of
  the same files asks. 1.36.6 said that no level writes an instruction file or a path outside the
  project unasked; that is true of the tools and was never true of the shell.
- **A command the policy reads as something else:**
  - a redirection in front of it: `2>/dev/null rm -rf V`, `>/dev/null rm -rf V`, `<<EOF rm -rf V`:
    the first word is the redirection, and `rm` is read as its argument;
  - `env -S 'rm -rf V'`, where `env` splits the string into a command;
  - `/usr/bin/time rm -rf V`: `time` by path is not the keyword (`command time rm -rf V` asks);
  - `function f { rm -rf V; }; f` (`f() { rm -rf V; }; f` asks);
  - `bash <<< 'rm -rf V'`, a here-string into a shell (a here-document into one asks);
  - brace expansion: `{rm,-rf,V}` runs `rm -rf V` in bash.
- **A word that is decided when the line runs:** a flag held in a variable (`F=-rf; rm $F V`), or
  made by a substitution (`rm $(echo -rf) V`); a glob that names a protected file (`cat .e*`,
  `cat .en?`, `cat .*` read `.env`). `rm -$X V` and `rm -r$(echo f) V` ask.
- **Something the agent wrote or a program runs later:** a script it wrote and ran
  (`echo 'rm -rf V' > x.sh; bash x.sh`, and `. x.sh`, `source x.sh`); a process substitution into
  a shell (`bash <(echo 'rm -rf V')`); a string the shell runs on exit (`trap 'rm -rf V' EXIT`); a
  program that runs a command (`python3 -c "import os; os.system('rm -rf V')"`, `perl -e`); a
  here-document body that a program runs, which 1.36.6 reads as text (`make -f - <<EOF` with a
  recipe, `python3 - <<EOF`; 1.36.5 asked about the first).
- **A shell fed by a pipe from inside a group:** `echo 'rm -rf V' | (cd /tmp; bash)` (`| (bash)`
  and `| { bash; }` ask; only the first command of the group is looked at).

None of these is new in 1.42.6 except `make -f - <<EOF`; the rest ran with no card on 1.36.5 too.

A **plan turn** (`plano: true` on the user message, the page's "Plan") denies every tool that
is not a read before the level is consulted (`decidirNoPlano`), so no approval can release a
write in that turn. Tests: `politica.test.mjs`, `plano.test.mjs`.

| Flag | What it does |
|---|---|
| `--version` | Prints `claude-runner <version>` from `package.json` (a version carrier of this repository) and exits |
| `--modelos` | Prints the Claude Code catalogue from the SDK's `supportedModels()` and exits; a control request, no turn |
| `--uso` | Prints the plan limits of the account it was started under (`uso.mjs`) and exits; no turn, no tokens |
| `--model`, `--effort`, `--cwd` | Model, effort level and project folder of the session |
| `--aprovacao` | The approval level above |
| `--parada host`, `--teto-custo`, `--teto-iteracoes` | The Run (section above) |

The account is chosen by the desktop, not by a flag: it sets the profile's variable
(`CLAUDE_CONFIG_DIR` or `CLAUDE_SECURESTORAGE_CONFIG_DIR`) in the runner's environment
([docs/code/CONTAS-CLAUDE.md](../docs/code/CONTAS-CLAUDE.md)).
