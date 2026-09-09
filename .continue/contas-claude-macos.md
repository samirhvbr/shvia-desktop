# Proposal — account profiles that are true on more than one machine

> **Status: proposed, not decided.** WIP under `.continue/` per `CLAUDE.md`. If it is
> accepted it becomes ADR-034 and this file migrates to `docs/`; if it is rejected, the
> rejection is worth writing down too, because the measurement that motivated it stands
> either way.
>
> **The measurement is not part of this proposal** — it is already established and lives in
> [`docs/code/CONTAS-CLAUDE.md`](../docs/code/CONTAS-CLAUDE.md) ("macOS: a variável que
> separa as contas do dono é outra", 08/09/2026). What follows only decides what to build.

---

## The problem in one paragraph

`contas_claude::semente()` offers a named profile only when its directory exists, and it
knows two names: `~/.claude-blue3` and `~/.claude-pessoal`. On the owner's Mac neither
exists, so the picker offers `Padrão do sistema` and nothing else. The accounts *are* there
— behind `CLAUDE_SECURESTORAGE_CONFIG_DIR`, a variable this module cannot express, pointing
at directories that are empty because on macOS that variable is a key into the Keychain and
not a store. Three separate logins exist on the machine and the app can reach exactly one
of them: the default, which is the one nobody chose.

**Widening the hardcoded list does not fix it and is not proposed.** Registering a
`-cred-` directory as a `dir` makes `aplicar()` export it as `CLAUDE_CONFIG_DIR`, which on
Linux gives `Not logged in` and on macOS gives the right credential with a blank
configuration home that forks settings, history and project state in silence. Two operating
systems, two different wrong answers, one registry entry. The failure modes are measured in
the doc linked above.

**Everything below keeps the module's stated principles unchanged:** the page sends an ID
from a closed list and never a path; shell aliases are never parsed; credentials are never
read, moved or created. Where a proposal comes close to one of those lines, the section
says where the line is.

---

## Question 1 — which variable should a profile be allowed to name?

### The three options

**A. Replace `CLAUDE_CONFIG_DIR` with `CLAUDE_SECURESTORAGE_CONFIG_DIR`.**
Matches the owner's Mac exactly: shared configuration, separate logins. Cheapest change to
`aplicar()` — one string.

- 🔴 **It breaks the machine ADR-033 was validated on.** The Linux profiles are
  configuration homes; re-exporting their paths under the other variable would key
  credentials by a path whose Keychain/credential entry was never written, and every
  existing `contas-claude.json` in the fleet silently changes meaning on upgrade.
- 🔴 **It quietly undoes 1.4.19.** A securestorage profile has **no configuration home of
  its own** — that is the whole point of it. So `cli_config.rs` cannot write
  `settings.json` "into the profile": there is no such place. Writing into the
  securestorage directory would put the file somewhere the client never reads — a silent
  no-op, which is the exact defect 1.4.19 fixed for the other variable.

**B. Keep `CLAUDE_CONFIG_DIR` only, and let the user migrate.**
Zero code. The owner would re-`claude login` into `~/.claude-blue3` and `~/.claude-pessoal`
and get separate configuration homes as a bonus.

- Honest and cheap, and for a single-user product not absurd.
- 🔴 But it makes the app dictate the shell layout, and the module's own posture is the
  opposite: it reads the machine and never tells the machine what to be. It also throws
  away the property the owner deliberately chose — one shared `~/.claude` (settings, MCP
  servers, history, `projects/`) with two logins. Migrating means either duplicating all of
  that or losing it in one of the two accounts.
- 🔴 And it leaves the Desktop reaching the *default* login, which is a third account
  nobody selected.

**C. Record the variable per profile. ← recommended**

`contas-claude.json` gains one field per entry, from a **closed enum of exactly two**:

```json
{
  "contas": [
    { "id": "empresa-blue3", "rotulo": "Empresa · Blue3",
      "var": "CLAUDE_SECURESTORAGE_CONFIG_DIR", "dir": "/Users/samir/.claude-cred-blue3" },
    { "id": "pessoal", "rotulo": "Pessoal",
      "var": "CLAUDE_SECURESTORAGE_CONFIG_DIR", "dir": "/Users/samir/.claude-cred-pessoal" }
  ],
  "selecionada": "padrao"
}
```

- `var` is absent in every file written before this change, and **absent means
  `CLAUDE_CONFIG_DIR`** — so every existing registry keeps its exact current meaning and
  the Linux machine needs no edit.
- The enum is closed to two names. Anything else is **discarded** on read, like a bad `id`
  or an out-of-home `dir` — `normalizar` already has that posture and this is one more
  clause in it. An open field would let a hand-edited file inject an arbitrary environment
  variable into a child process, which is a different and much worse feature.
- `var` **never crosses the bridge**, in neither direction. The page still sends an ID and
  still receives `{id, rotulo, disponivel}`. The invariant of ADR-026/ADR-031 is untouched
  because nothing about this field is page-supplied.
- `aplicar()` takes the profile rather than a bare path, and sets the one variable that
  profile names.

**Cost, stated plainly:** the mental model gets harder. `dir` now means two different things
depending on `var` — a configuration home, or a namespace string that is hashed and
otherwise unused. That is genuinely worse to explain than one field with one meaning, and
it is the price of being true on two machines instead of one.

### The consequence that decides it: `cli_config.rs`

"Connect my CLI" writes `settings.json` into the selected profile's directory (1.4.19). For
a securestorage profile that destination **does not exist**, and the correct destination is
the shared `~/.claude/settings.json`.

Only option C can tell the difference. Under A, `cli_config` would write into a
credential-namespace directory the client never reads — no error, no effect, and the user
discovers it by asking why the configuration "didn't take", which is precisely the bug
report 1.4.19 was written to prevent. So the proposal is:

> `Cliente::ClaudeCode.caminho()` receives `None` for a securestorage profile, landing on
> `~/.claude/settings.json`. The native confirmation dialog already shows the exact path
> before writing, so the user sees the shared destination and decides — no new UI needed to
> make it honest.

### What does **not** change under C

`resolver` keeps refusing a profile whose directory is missing (`conta_indisponivel`), for
both variable kinds. On macOS a securestorage directory does not technically need to exist
— the client only hashes the string — so this check is stricter than the platform requires.
**Keep it anyway:** on Linux the directory *is* where the credential file lives, so the
check is load-bearing there; and requiring a deliberate `mkdir` costs the user one command
and buys one rule that reads the same on both operating systems. A per-OS relaxation would
be a second thing to explain for no gain.

---

## Question 2 — how does a profile get registered at all?

Today: seeded from two hardcoded names, or hand-edited into
`~/Library/Application Support/cloud.blue3.shvia/contas-claude.json`. There is no UI. On a
machine the seed does not recognise, the feature is invisible and the user is not told it
exists.

### Rejected: scan `$HOME` for `.claude-*` directories

Tempting, and wrong for the same reason parsing aliases is wrong: it infers meaning from a
name. `~/.claude-blue3` and `~/.claude-cred-blue3` are indistinguishable to a glob and mean
**different variables**. A scan would have to guess which, and guessing wrong produces the
silent-fork failure described above. Discovery by naming convention is a closed list whose
contents someone else writes.

### Proposed: a native "Add account…" gesture, and then delete the seed

Same shape as ADR-031's folder fence — **the path comes from the user's gesture**, not from
the page and not from a guess. `tauri-plugin-dialog` already provides the directory picker
the folder fence uses; the profile registration is:

1. native directory picker → `dir` (the page never sees it, never chose it);
2. native confirmation naming the two separations in the user's words, which sets `var`:
   *"Separate only the login (shared settings and history)"* →
   `CLAUDE_SECURESTORAGE_CONFIG_DIR`; *"Separate the whole Claude Code configuration"* →
   `CLAUDE_CONFIG_DIR`;
3. `rotulo` defaults to the directory's basename, `id` is derived from it and passed through
   `id_valido` with a numeric suffix on collision.

**Why not a proper form:** Tauri 2 has no native multi-field prompt, so a real form means a
second native window owned by the shell — a meaningful amount of new surface for a feature
one person uses on two machines. The picker-plus-confirm version above needs no new window
and no new IPC verb that carries a path. If the label matters more than it looks, a native
window is the upgrade path, not a redesign.

**Once the gesture exists, `semente()` should be deleted, not extended.** Its whole job was
to guess what the user had; a gesture asks. Hand-editing stays documented as the escape
hatch and the second-machine path — it is already validated on every read and that does not
change.

**Trade-off:** deleting the seed means the Linux machine, on a fresh install, no longer
gets its two profiles for free — it gets an empty list and one gesture per account. That is
a real regression in convenience for exactly one machine, and the argument for accepting it
is that the same convenience is what made this feature invisible on the other one.

---

## Question 3 — how should the screen say "registered, but no login here"?

`disponivel` means the directory exists. On macOS, for a securestorage profile, that now
reports **only that the user ran `mkdir`** — the client never writes there. It is the
weakest signal in the feature and it is the one the screen renders.

### Rejected: ask the client

The obvious idea — run the control channel and read whether it says logged-in — was tested.
`--modelos` returns an identical catalogue with and without the variable on macOS/2.1.265,
so it discriminates nothing here. Recorded in the doc so it is not proposed again.

### Proposed: a presence check on the credential store, advisory only

The client keys its credential store by `sha256(dir)[0:8]` (documented and measured). So
"does this profile have a login" is answerable by checking whether that **item exists** —
on macOS a Keychain query by service name with no `-w`, so no secret is read, nothing is
decrypted, and no authorization prompt appears.

**This does not cross the module's line.** Its principle is that credentials are never
read, moved or created. Existence is to a credential what `is_dir()` is to a file: the
module already decides `disponivel` by asking whether something is there without opening
it, and this is the same question asked of the right object.

The bridge grows one field alongside the existing ones, which keep their exact meaning:

| field | meaning |
|---|---|
| `disponivel` | unchanged — the directory exists. Still what gates the option. |
| `login` | `"presente"` · `"ausente"` · `"desconhecido"` |

`desconhecido` is the honest answer wherever the check is not implemented (Windows today)
or the derivation did not match, and it is what an older shell reports by simply not
sending the field — capability by presence, the same design as `recursos.conta`.

### 🔴 The constraint that makes this safe: it may inform, never block

This check depends on an **undocumented internal derivation** of the official client. If
the scheme changes, it starts reporting `ausente` for perfectly good profiles.

> `resolver` must **not** gain an arm for `login`. A profile with `login: "ausente"` stays
> selectable and still spawns. The screen may warn — *"no Claude Code login found for this
> profile on this computer"* — and must not disable the option.

This is deliberately the inverse of the module's posture everywhere else, and the reason is
the difference in what we know. `conta_desconhecida` and `conta_indisponivel` fail loudly
because they are facts this code establishes itself and can stand behind. `login` is a
reading of somebody else's private implementation. A check we cannot fully trust must never
be able to refuse a turn — the worst outcome of a stale derivation should be a wrong
warning, never a working account the app declines to use.

---

## What this proposal does not answer

- **Windows.** Untouched, unmeasured, and the credential store is a third mechanism again.
  `login` reports `desconhecido` there until someone measures it.
- **Whether a turn actually differs per account on macOS.** The keying is measured; a real
  turn under each account is not, and it spends quota on two subscriptions. Worth doing
  before shipping option C, and it is the owner's call to spend it.
- **The web half.** A `login` field and an "Add account…" entry both need the compositor
  ruler in SHVIA-WEB. Gated by presence like `recursos.conta`, so an old shell with a new
  page keeps the ruler it has.
