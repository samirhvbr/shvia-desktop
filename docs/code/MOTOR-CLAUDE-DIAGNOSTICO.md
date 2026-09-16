# The Claude Code engine does not answer — what to check, in order

One command tells you whether the problem is the app or the machine's login, and the rest of
this page is what to do with each answer.

> **Why this page exists.** On 08/09/2026 the engine was diagnosed from scratch on a new
> machine: three hypotheses were tried and discarded before the real cause, and every step
> was reproducible from the start. Written so the next machine costs one command instead of
> an evening. The account mechanism itself is [CONTAS-CLAUDE.md](CONTAS-CLAUDE.md) — this
> page does not repeat it, it points at it.

---

## 0. The command that splits the problem in two

```bash
claude -p "responda apenas: ok"
```

| It answers | Then |
|---|---|
| `ok` | the machine's login is fine — the problem is in the app or the runner (§2, §3) |
| `Not logged in · Please run /login` | the app is **not** the problem: the official client refuses too (§1) |

🔴 **Run this before touching the app.** On 08/09 the failure looked like a packaging
regression in a delivery that had nothing to do with it, and this one command would have
said so in ten seconds.

---

## 1. `Not logged in · Please run /login`

**`/login` does not exist in this context**, and typing it in the Code panel answers
`/login isn't available in this environment`. It is an interactive command of the CLI; the
runner is not interactive. The message is the SDK reporting the absence, not an instruction
the panel can carry out.

### Which login the app can see

Credentials are stored per **slot**, and the slot is decided by the environment of whoever
ran the login. On macOS the slot is a Keychain service name:

| How you logged in | Slot | The app reads it? |
|---|---|---|
| `claude login` | `Claude Code-credentials` | **yes** |
| `CLAUDE_SECURESTORAGE_CONFIG_DIR=<dir> claude login` | `Claude Code-credentials-<sha256(dir)[0:8]>` | no |
| `CLAUDE_CONFIG_DIR=<dir> claude login` | same shape, same suffix rule | only if that profile is selected in the picker |

Reproduce a suffix, and check a slot exists without reading it:

```bash
printf '%s' "$HOME/.claude-cred-blue3" | shasum -a 256 | cut -c1-8   # the suffix
security find-generic-password -s "Claude Code-credentials" >/dev/null && echo present
```

🔴 **A named account answering in the terminal does not mean the app works.** That was
exactly the state on 08/09: `claude-me -p` and `claude-b3 -p` both answered `ok` while
`claude -p` refused, and the app only ever reaches the slot the third one writes. The two
aliases are shell functions that export `CLAUDE_SECURESTORAGE_CONFIG_DIR`; the app exports
nothing when the selected profile is `padrao`.

### The alias is not a shortcut the app can take

`claude-me` and `claude-b3` are shell **functions** (`whence -w` says so), and their body
ends in `exec claude "$@"`. Two consequences worth knowing before anyone proposes wiring
them into the app: there is nothing to exec, and even if there were, it launches the
interactive CLI rather than `claude-runner`, which is what the app speaks to. What the alias
contributes is one line — a variable and a directory — and that line is what a profile would
have to store. See `.continue/contas-claude-macos.md`.

**The fix:** run `claude login` with **no** variable and authorise the account you want the
app to use. It lands in the default slot, which is the one the app reads, and the picker's
`Padrão do sistema` stays truthful.

---

## 2. `claude-runner não encontrado`

The runner is **not bundled**. `tauri.conf.json` declares `externalBin: ["binaries/anna"]`
and nothing else, so an installed bundle carries `anna` and `shvia-desktop` only. Absence of
the runner is an optional install that was never done — never a packaging regression.

```bash
bash <repo>/claude-runner/install.sh     # wrapper into ~/.local/bin, SDK into ~/.local/share
~/.local/bin/claude-runner --modelos     # answers the catalogue if the install is complete
```

The installer verifies itself since 1.4.22: it imports the installed module before printing
`✓`, so a missing file fails there, naming it, instead of failing on somebody's screen.

---

## 2b. The runner is installed but OLD — and the Run silently does nothing

`claude-runner --version` against `version.md` is the whole check, and it is worth doing
before blaming the Run for not working.

**Measured on the owner's Mac, 16/09/2026: installed `1.4.20`, repo `1.5.14`.** Twenty-four
versions, and the one that matters is **1.5.0**, which added `parada.mjs` — the Stop hook
that IS the Run on the Claude engine. An older runner answers turns perfectly well and
simply never asks the host whether to continue. Nothing errors. The Run just does not
happen, and the screen has no way to tell that apart from a run that decided to stop.

🔴 **And until 1.5.15 reinstalling did not fix it, because the installer was the defect.**
`install.sh` copied its files by a hand-written list that never gained `parada.mjs`, so a
fresh install refused at its own import guard — the guard added in 1.4.22, doing its job.
**Third time in this class:** `politica.mjs` was left out from 1.4.7 to 1.4.22, and
`parada.mjs` from 1.5.0 to 1.5.15.

The guard is good and was not enough: it only runs **during an installation**, and CI does
not install. So `npm run prova:instalador` now reads the imports against the `cp` line as
text — no install, no `node_modules`, runs anywhere, which is exactly where the 1.4.22 guard
cannot reach.

**If `--version` is behind:** `bash claude-runner/install.sh` from the repository. It is
idempotent, and from 1.5.15 the install is complete.

## 3. The account picker shows only `Padrão do sistema`

**Not a regression.** `contas_claude::semente()` offers a named profile only when its
directory already exists, and that rule is in the feature's first commit (1.4.16) — the
module has two commits in its life and the second changed a comment. If a profile had ever
been seeded on this machine it would be written in `contas-claude.json`; an empty `contas`
array is the seed having found nothing.

The two names it knows are `~/.claude-blue3` and `~/.claude-pessoal`, and they are a
**Linux** layout. A machine that separates its accounts another way gets one entry. The
measurement of one such machine, with the variable it uses instead and what registering the
wrong directory would do on each operating system, is in
[CONTAS-CLAUDE.md](CONTAS-CLAUDE.md#-macos-the-variable-that-separates-the-owners-accounts-is-another-one).

---

## 4. The end-to-end probe

The panel adds its own failure modes; this runs the same path with none of them.

```bash
printf '{"type":"user","text":"responda apenas: ok"}\n' \
  | ~/.local/bin/claude-runner --model opus --cwd /tmp
```

A healthy run emits `model`, then `text` with the answer, then `usage`. Anything else names
its own cause in the `error` line.

---

## 5. The temporary workaround, and what it costs

The shell never clears the environment when it spawns the runner — it only **adds** `PATH`,
`SHVIA_API_KEY` and, for a named profile, `CLAUDE_CONFIG_DIR`. So a variable in the app's own
environment reaches the runner, and launching the app with one makes the whole session use
that account:

```bash
# quit the app first: it is single-instance, and a second launch only focuses the first
CLAUDE_SECURESTORAGE_CONFIG_DIR="$HOME/.claude-cred-blue3" \
  /Applications/ShvIA.app/Contents/MacOS/shvia-desktop
```

🔴 **The screen will say `Padrão do sistema` while the turns run on another account.** That
is the one failure the accounts module says it must never produce — a name on screen and a
different subscription paying. Acceptable as a deliberate, temporary shortcut; not as a
state anyone leaves behind.

---

## What this page does not cover

The account mechanism, the bridge contract, the error codes and what `disponivel` does and
does not claim: [CONTAS-CLAUDE.md](CONTAS-CLAUDE.md). Why the picker cannot express the
second variable, and what it would take: `.continue/contas-claude-macos.md`, still a
proposal.
