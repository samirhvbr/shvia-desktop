# ShvIA Desktop — Notas de Desenvolvimento
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

## Padrão de Commits (obrigatório)

Formato: `X.Y.Z - description in English (US)`

```
0.1.0 - Cria shell Tauri apontando para o ShvIA hospedado
0.2.0 - Adiciona tray, deep-link shvia:// e tela offline
0.3.0 - Pipeline de assinatura/notarização nos 3 SOs
```

**Regras inegociáveis:**
1. A versão **sempre** vem de `version.md` — bumpe o arquivo **no mesmo commit** da
   mudança, nunca separado.
2. Critério de bump:
   - **Z**: mudança visível de UI/menu/janela, ajuste de empacotamento/build.
   - **Y**: nova capacidade de runtime, redesenho de IPC, mudança de auth-handoff.
   - **X**: versão estável — bump manual.
3. Mensagem em **português**, descritiva o suficiente para `git log --grep`.
4. Proibido `feat:`, `fix:`, `chore:` ou mensagens vagas ("ajuste", "fix", "update").

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

## Language — English (US), everywhere in the repository

**Everything that lives in this repository, or in GitHub's interface around it,
is written in English (US)**: documents, **commit messages**, pull request titles
and bodies, issues, code comments, changelog entries, release notes.

Commit format: `X.Y.Z - short description in English`. The version comes from
`version.md` and is bumped in the same commit. Conventional Commits prefixes
(`feat:`, `fix:`, `chore:`) and vague one-word messages are forbidden.

**Exactly one carve-out:** end-user-facing strings — UI text, transactional
email, product copy. That is product i18n for a Brazilian audience, not
repository content.

History is not rewritten: Portuguese messages already in the log stay as they
are.

<!-- /RELEASES-RULE -->
