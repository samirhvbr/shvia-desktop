# ShvIA Desktop — Notas de Desenvolvimento

<!--
  Este arquivo segue o padrão agents.md (lido por ferramentas de IA além do
  Claude Code). O conteúdo abaixo do título H1 é duplicado em CLAUDE.md —
  mantenha os dois byte-idênticos abaixo do H1. Se editar um, edite o outro.
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

Formato: `versão - comentário em português`

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
- **Nenhum segredo do servidor mora neste repo.** E **não há "secrets de CI"** onde
  guardá-los: a CI foi removida na 0.4.6 e o build é 100% local. Onde as
  credenciais de assinatura vivem: no macOS, a senha de notarização no **keychain**
  (serviço `shvia-notarize`); no Windows, o certificado no **repositório de
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

## PS — Commits: a skill COMMITTER cuida disso

**Existe `.committer.yml` na raiz deste repositório** — é o opt-in da skill
**COMMITTER**, que roda em ciclo (cron, via `~/x/GIT/run.sh`). Enquanto esse arquivo
existir com `enabled: true`, **commitar e pushar não é trabalho seu**.

**O que muda para você:**

- **Não commite nem pushe por padrão.** Conclua a entrega bumpando o `version.md`
  **com a entrada de changelog** e deixe a árvore pronta. É dali que a mensagem do
  commit sai — o changelog virou o artefato de handoff entre você e a skill.
- A skill monta `X.Y.Z - descrição`, commita e pusha a branch atual sozinha. Ela
  **nunca bumpa versão** (isso continua sendo julgamento seu) e nunca inventa
  mensagem: sem entrada de changelog ela cai num fallback Sonnet, e sem conseguir
  descrever com honestidade ela aborta e espera.

**Você ainda commita quando:**

- o Samir pedir explicitamente;
- a tarefa exigir o SHA na hora (deploy, abrir PR, referência cruzada);
- o `.committer.yml` sumir ou estiver `enabled: false` — aí vale o fluxo antigo,
  você bumpa, commita e pusha.

**Por que isso existe:** tirar de um modelo caro (Opus/Fable) o trabalho mecânico de
empacotar commit, que um Sonnet — ou, na maioria das vezes, nenhum modelo — resolve.
Economiza token e devolve tempo de desenvolvimento.

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

<!-- /RELEASES-RULE -->
