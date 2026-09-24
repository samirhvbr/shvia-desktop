# ShvIA Desktop — Instruções para Claude Code
<!--
  `AGENTS.md` (padrão agents.md, lido por ferramentas de IA em geral) e `CLAUDE.md`
  (lido pelo Claude Code) são o MESMO texto abaixo do H1 — só o título difere.
  Editou um, edite o outro: o teste `agents_e_claude_sao_espelho` reprova a
  divergência. Este bloco é escrito na 3ª pessoa de propósito, sem "este arquivo",
  para poder ser byte-idêntico nos dois (achado F-21).
-->
> **Leia também:** [README.md](README.md) (visão geral + decisão de arquitetura) ·
> [.continue/estado-atual.md](.continue/estado-atual.md) (onde paramos) ·
> [.continue/escopo-projeto.md](.continue/escopo-projeto.md) (escopo e fases) ·
> [docs/decisoes.md](docs/decisoes.md) (ADRs).

---

## 🔄 Antes de começar: `git pull`

**SEMPRE** verifique atualizações remotas antes de escrever ou alterar qualquer
coisa neste repositório:

```bash
git pull          # já está pré-autorizado (allow)
```

Trabalhar sobre uma base desatualizada gera conflitos. Puxe primeiro, sempre.
Para só inspecionar antes: `git fetch && git status`.

---

## O que é este repo

Cliente **desktop multiplataforma** (Tauri 2) do **ShvIA** — a plataforma de IA
da Blue3 hospedada em `https://ai.shvia.org`. **Shell fino:** a janela carrega
o ShvIA web (Blade) remoto; o servidor é a fonte da verdade (dados, senhas,
permissões). Detalhes e o porquê em [README.md](README.md) e
[docs/decisoes.md](docs/decisoes.md). **Não confundir** com o repo Laravel do
ShvIA (`/Users/samir/x/IA`) — aquele é o servidor, este é o cliente.

---

## Commit standard (mandatory)

Format: `X.Y.Z - description in English (US)`, the same sentence as the `CHANGELOG.md` heading.

```
1.6.51 - a cd out of the project does not reach the next command, proved with the real SDK
1.6.38 - the Claude Code login asks in a native dialog before it starts
1.6.33 - a half-bumped commit is refused before it exists
```

**Non-negotiable rules:**
1. The version **always** comes from `version.md`: bump the file **in the same commit** as the
   change, never in a separate one.
2. Bump criterion:
   - **Z**: a visible UI/menu/window change, a packaging or build adjustment.
   - **Y**: a new runtime capability, an IPC redesign, an auth-handoff change.
   - **X**: a stable version, bumped by hand.
3. The message is in **English (US)**, descriptive enough for `git log --grep` (the Language
   block below says why). Portuguese messages already in the history stay as they are. Until
   1.6.54 this line said Portuguese and contradicted that block.
4. No `feat:`, `fix:`, `chore:`, and no vague messages ("adjust", "fix", "update").

---

## Servidor remoto é a fonte da verdade

- **Nenhum banco roda aqui.** Sem MySQL, sem SQLite, sem Postgres no cliente — a
  regra do ShvIA ("MariaDB/MySQL, nunca SQLite") é satisfeita por construção.
- **Nenhum segredo do servidor mora neste repo.** A pipeline de build/empacotamento/assinatura
  (matriz macOS/Windows/Linux) foi removida na 0.4.6 por custo e o build de release segue
  **100% local por decisão** — não por falta de cota: a conta migrou para GitHub Enterprise
  (50.000 min/mês), mas runners macOS consomem minutos a **10x** (ver `docs/build.md`), então
  trazer a matriz de volta é uma escolha de arquitetura à parte, não consequência automática do
  upgrade. Testes (Rust/lint) já rodam via `.github/workflows/ci.yml` desde 01–02/09/2026.
  Enquanto o build de release for local, **não há "secrets de CI"** onde guardar credencial de
  assinatura. Onde elas vivem hoje: no macOS, the notarization credential is an App Store
  Connect API key in `~/.shvia/` since 1.6.39 (the app-specific password in the **keychain**,
  service `shvia-notarize`, is the fallback — see `docs/build.md`); no Windows, o certificado no **repositório de
  certificados do SO** (ou um `.pfx` fora da árvore do repo), apontado por
  `SHVIA_WIN_CERT_THUMBPRINT` / `SHVIA_WIN_PFX` em variável de ambiente da sessão.
  Nunca versionadas (ver `.gitignore`).
- **Auth é cookie de sessão same-origin** (a janela navega o FQDN real). Não
  inventar fluxo de token na F1 — o login é a tela do próprio ShvIA.

---

## Stack & runtimes

- **Tauri 2** (Rust) + WebView nativo do SO (WKWebView / WebView2 / WebKitGTK).
- Casca web mínima (Vite/TS). Sidecar Python (PyInstaller) entra na **F2**.
- Três ecossistemas: `npm` (casca) · `cargo` (Rust) · `pip` (sidecar).
- Comandos úteis (após o scaffolding da F1):
  ```bash
  npm run tauri dev          # roda o app em dev
  cargo clippy               # lint do Rust
  npm run build              # build da casca web
  npx tauri build            # empacota (pesado — pede confirmação)
  ```

---

## Documentação (sem doc, sem deploy)

- **Função/decisão nova vira doc** em `docs/` (estável) antes de entrar.
- **WIP** mora em `.continue/` (versionada, fora de build). Quando amadurece,
  migra para `docs/`.
- **Decisões** vão em [docs/decisoes.md](docs/decisoes.md) (formato ADR). Não
  relitigar direção já decidida dentro de um how-to — linkar o ADR.
- `CLAUDE.md` e `AGENTS.md` são **espelhados** abaixo do H1 — editar os dois.

---

## Referências rápidas

- Versão atual: `version.md`
- Escopo e plano de fases: [.continue/escopo-projeto.md](.continue/escopo-projeto.md)
- Arquitetura técnica: [docs/arquitetura.md](docs/arquitetura.md)
- Roteiro de fundação (F0/F1): [docs/roteiro-fundacao.md](docs/roteiro-fundacao.md)
- Base técnica (colher ativos): SHVTERM em `/Users/samir/Projetos/SHVTERM`
- Servidor/fonte da verdade: ShvIA em `/Users/samir/x/SHVIA/SHVIA-WEB` (`https://ai.shvia.org`)

---

<!-- COMMIT-RULE:repodocs -->

## Commits — you commit, and nothing is delivered until you have

> Marked echo. The single source is **[samirhvbr/repodocs](https://github.com/samirhvbr/repodocs/blob/master/docs/versioning.md#who-commits-and-when)**
> — change it there, not here. This block is regenerated.

**Committing is your job.** Not "leave the tree ready and something downstream
packages it" — you run `git commit`, and `git push`, as the last step of the work
you were asked to do. The COMMITTER skill that used to commit on an agent's
behalf is `enabled: false` in every repository of this fleet since 03/09/2026;
what is left of it is a kill-switch, not a scheduler. **If you do not commit,
nobody does.**

**Do not report a task as finished before the commit exists.** "Done",
"delivered", "concluded" mean the work is in `git log` — never that it is sitting
uncommitted where only this session can see it. The commit is the last step *of
the task*, not a follow-up for someone else. If you are about to write
"finished", commit first, then write it.

**Push is part of the delivery, and a refused push is the one place a human enters.**
Commit *and* push, every delivery — a clean push needs nobody's permission and is never
held back for review. When the push is **refused** (conflict, non-fast-forward, protected
branch), stop there and say so: never force, never rewrite history to get past it, never
invent a merge resolution you have not verified. The gate is the refused push, not the
commit.

**Every commit obeys the versioning rules**, with no exception:

- Subject `X.Y.Z - short description in English (US)`, the version taken from
  `version.md` and **bumped in the same commit**.
- The `CHANGELOG.md` entry is written first — its `## X.Y.Z - description`
  heading *is* the subject.
- No Conventional Commits prefix (`feat:`, `fix:`, `chore:`) and no vague
  subject ("update", "ajuste", "wip", "changes", "several improvements").

**The bump is the one clause a repository may override — in writing.** If this
repository's own documentation says the version is stamped some other way, and says
why, follow that. Otherwise the line above applies to you. An override nobody wrote
down is not an exception. Nothing else in this block bends: the changelog entry, the
subject, the language, one subject per commit, and committing before you report done
all hold regardless.

**An override moves *when* the version is decided, never *whether* every commit
carries it.** A delivery split into blocks — the default — must come out with the
version on **every** subject, not on the last one. A placeholder left in a subject
that reaches the default branch is a defect and is permanent, because the default
branch is not rewritten. Measured: 26 of them in the one repository that stamps at
merge, before its mechanism was fixed.

**All of this governs the repositories we own.** In a repository that is not
ours, the host's commit convention governs instead — their subject line, in
their language. `X.Y.Z` is meaningless where there is no `version.md` of ours,
and there is no version there for us to bump. Our versioning rules govern our
remotes, not every remote we can push to.

**One subject per commit.** The subject has to describe the whole commit
honestly. The moment your description needs an "and" to be true, it is two
commits.

**Split a large delivery into blocks.** A complex task is committed as a series
of commits grouped by subject, each small enough to be described in one line and
read on its own. They may share a version — bump `version.md` in the first and
repeat the number in the rest; two commits carrying one version is expected, not
a mistake. **Splitting is the default** for anything non-trivial, because the
history is the documentation of *how* the work was done, and one commit touching
six unrelated subjects documents none of them.

**The standard you are keeping:** someone reading `git log` alone — a year from
now, without the conversation that produced the work — can say what happened,
when, why, and at which version. If your commit would fail that test, it is too
big or its subject is too vague, and both are fixed the same way.

<!-- /COMMIT-RULE -->

---

<!-- LANGUAGE-RULE:repodocs -->

## Language — English (US) at home, the upstream's when we are guests

> Marked echo. The single source is **[samirhvbr/repodocs](https://github.com/samirhvbr/repodocs/blob/master/docs/conventions.md#8-language)**
> — change it there, not here. This block is regenerated.

**Everything that lives in this repository, or in GitHub's interface around it,
is written in English (US)**: documents, **commit messages**, pull request titles
and bodies, issues, code comments, changelog entries, release notes.

Commit format: `X.Y.Z - short description in English`. The version comes from
`version.md` and is bumped in the same commit. Conventional Commits prefixes
(`feat:`, `fix:`, `chore:`) and vague one-word messages are forbidden.

**Three carve-outs, and only three.** The first is end-user-facing strings — UI
text, transactional email, product copy: product i18n for a Brazilian audience,
not repository content. The second is the **Blue3 internal repositories**
(`BLUE3-ISP/*`, `samirhvbr/blue3-intranet`, `samirhvbr/blue3-ai-login`), which
are Portuguese throughout — if you are reading this block inside one of them,
this is the wrong block: they carry `LANGUAGE-RULE-PT`. A repository joins that
set by a written decision, never by argument.

**The third is `.continue/`.** The queue is written in the language its author
thinks in, and becomes English (US) when the work is **produced** and the
document moves to `docs/`. A Portuguese draft in the queue is not a violation to
be fixed: it is unfinished work in the language it is being thought in, and
translating it or moving it out before the thing exists destroys the only place
that thing exists.

History is not rewritten: Portuguese messages already in the log stay as they
are.

**In a repository that is not ours, the upstream's conventions win — the
language and the commit shape both.** Opening a pull request or an issue on a
repository we do not own makes us guests, and a guest writes in the host's
language. Our `X.Y.Z - description` is meaningless there anyway: they have no
`version.md` of ours, and no version for us to bump.

**Check before you write, and the first signal that answers wins:** a written
instruction (`CONTRIBUTING.md`, a pull request or issue template, a contribution
section in the README), then the last ~20 merged pull requests, then the issues,
then the commit log. A written instruction beats observed practice — if they ask
for English and their log is Portuguese, write English. Below that line the
**clear majority** decides, and clear means clear.

**When you cannot tell, write English (US).** A private repository, an empty
history, no network, a refused `gh` call and a genuinely mixed log all land in
the same place — the house rule. Unverifiable is not a licence to guess.

**This is a scope boundary, not a second carve-out.** Nothing in *our*
repositories changes because a foreign one is Portuguese, and code identifiers
are English wherever you are.

<!-- /LANGUAGE-RULE -->

<!-- QUEUE-RULE:repodocs -->

## The queue empties by production, and by nothing else

> Marked echo. The single source is **[samirhvbr/repodocs](https://github.com/samirhvbr/repodocs/blob/master/docs/conventions.md#1-continue-is-the-queue--docs-is-what-has-been-produced)**
> — change it there, not here. This block is regenerated.

**`.continue/` holds work that does not exist yet.** A document leaves it when —
and **only** when — the thing it describes **exists**. Length is not an exit
condition. Neither is age, language, untidiness, the end of a session, or an
agent who would have written it differently.

> `tela.md` says *"a black screen with a yellow ball in the middle"*. It leaves
> the queue when there is a black screen with a yellow ball. Until then it stays,
> at any length, in whatever shape it is in — because until then it is the only
> place that thing exists.

**"Produce", applied to a queue item, means making the thing exist.** Not editing
the document, not translating it, not promoting it to `docs/`. The document is
the specification; the deliverable is the thing. Removing the document is the
**last step of the commit that carries the work** — never a step of its own.

**Never empty this folder as tidying.** A queue item deleted without the work
being done destroys the only artefact a project has before it has code — and
what usually replaces it is worse than the loss: a `docs/` page describing a
screen nobody built, indistinguishable from a page describing one that exists.
If a plan has to be visible in `docs/` before it is built, it is `PROPOSED`,
never `ACTIVE`.

**The half-a-page rule is about a record that ended up in the queue**, and about
nothing else. It has no opinion on the length of a specification of unbuilt
work: a 1,300-line brief about something that does not exist is in the only
place it can be. A long queue item is a project with a lot still to build.

**The queue is written in the language its author thinks in**, and becomes
English (US) on the way out, when the work is produced and the document moves to
`docs/`. A Portuguese draft in `.continue/` is not a violation to be fixed.

<!-- /QUEUE-RULE -->

<!-- RELEASES-RULE:repodocs -->

## Releases — the `version.md` on GitHub is what the Releases show

> Marked echo. The single source is **[samirhvbr/repodocs](https://github.com/samirhvbr/repodocs/blob/master/docs/versioning.md)**
> — change it there, not here. This block is regenerated.

**The `version.md` of the default branch, on GitHub, is what the GitHub Releases
must show.** The local checkout does not enter the calculation: it can be behind,
ahead or mid-work, and none of that is published — GitHub cannot tag a commit it
does not have.

**The bump and the Release are one act.** A commit that bumps `version.md` is not
finished until that version has a tag, a published Release, and the **`Latest`
badge on it** — the same push, not "later". A badge sitting on an older release
tells whoever looks that the project is at a version it is not.

- `.github/workflows/release.yml` does it on any push that touches `version.md`.
- `./tools/release.sh` does it by hand. It is **idempotent and self-healing**:
  it publishes whatever is missing and moves a drifted badge back. Running it is
  always safe, so it is both the check and the fix.

A PR publishes nothing while it is a PR. The moment it merges, the push moves
`version.md` on the default branch and the Release becomes that version.

Tag and Release title are the **bare version — no `v` prefix**.

<!-- /RELEASES-RULE -->
