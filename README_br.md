# ShvIA Desktop (`shvia-desktop`)

> **Projeto interno da Blue3.** Cliente **desktop multiplataforma** (macOS,
> Windows, Linux) do **ShvIA** — a plataforma interna de IA da Blue3
> (`https://ai.shvia.org`). Documentação interna — não publicar.

**Ver também:** [CLAUDE.md](CLAUDE.md) / [AGENTS.md](AGENTS.md) (convenções de
código e do agente) · [docs/README.md](docs/README.md) (índice da documentação
técnica) · [.continue/escopo-projeto.md](.continue/escopo-projeto.md) (escopo
detalhado e plano de fases) · [docs/decisoes.md](docs/decisoes.md) (ADRs).

---

## Sumário

1. [O que é](#o-que-é)
2. [Decisão de arquitetura](#decisão-de-arquitetura)
3. [Stack](#stack)
4. [Modelo organizacional](#modelo-organizacional)
5. [Estrutura do repositório (alvo)](#estrutura-do-repositório-alvo)
6. [Versão (`version.md`)](#versão-versionmd)
7. [Relação com ShvIA e SHVTERM](#relação-com-shvia-e-shvterm)
8. [Roadmap em fases](#roadmap-em-fases)
9. [Status atual](#status-atual)

---

## O que é

**ShvIA Desktop** é um app desktop que entrega o ShvIA com **a cara do projeto**
e as **mesmas funções** do web app — empacotado como aplicativo nativo para
macOS, Windows e Linux, com janela própria, ícone, bandeja (tray), notificações
de SO e auto-update.

O ShvIA em si **continua sendo o servidor Laravel hospedado** em
`https://ai.shvia.org`: chat com IA (streaming SSE), comparação de modelos,
workspaces/pastas com arquivos (RAG), base de conhecimento, skills, painel admin
e rastreamento de uso/tokens. O desktop é o **cliente** dessa instância — não
reescreve o backend nem o frontend.

---

## Decisão de arquitetura

> Decisão tomada em **30/06/2026**, após análise multi-agente (ver
> [docs/decisoes.md](docs/decisoes.md) para os ADRs completos).

**Shell fino em Tauri 2 carregando o ShvIA web (Blade) remoto.**

- **Base = SHVTERM** (`/Users/samir/Projetos/SHVTERM`), nosso cliente desktop
  **Tauri 2 + React** já multiplataforma, com CI dos 3 SOs, updater e padrões de
  empacotamento prontos. **O fork Claude Desktop foi descartado** (arquivado em
  `archive/claude-fork` + tag `archive/claude-fork-v0.2.2`).
- **A janela Tauri abre o ShvIA hospedado** (`https://ai.shvia.org`). Assim
  **"mesmas funções" é literal** — é a própria UI Blade do ShvIA. Zero código
  Laravel forkado, zero UI reescrita na Fase 1.
- **Servidor remoto = fonte da verdade** (dados, senhas, permissões). O desktop
  **não abre nenhum banco local** — a regra "MariaDB/MySQL, nunca SQLite" é
  satisfeita por construção (não há DB no cliente).
- **Auth = sessão Sanctum (cookie), same-origin.** Como navegamos o FQDN real, o
  login é a tela normal do ShvIA e o cookie de sessão autentica tudo, como num
  browser. (Bearer token só serve `/api/v1`; deep-link SSO `shvia://` é o único
  caso que exige tratamento extra — Fase 2.)
- **Camada nativa fina** (Rust/Tauri): janela com branding ShvIA, tray, deep-link
  `shvia://`, notificações de SO, auto-update e persistência de config/janela.

**Por que não Electron, não NativePHP:**

| Alternativa | Por que descartada |
|-------------|--------------------|
| **Electron** (o fork) | Chromium embarcado (~120 MB/build) sem ganho aqui; o Tauri do SHVTERM já está pronto. Mantido só como **fallback** se o streaming SSE quebrar no WebKitGTK (Linux). |
| **NativePHP** (Laravel local) | Ganho dele é SQLite local; o ShvIA **exige MariaDB/MySQL e proíbe SQLite**. Forçar MySQL em cada laptop forkaria a camada de dados — o oposto de reuso. |

**Trade-off assinado:** a arquitetura é **online-first / efetivamente
online-only**. Aceitável para um app de chat de IA (a inferência é server-side de
qualquer forma), endereçado com uma **tela offline** com a marca ShvIA + retry.
Operação genuinamente offline é um *killer* desta arquitetura. **Confirmado pelo
dono em 23/09/2026** ("aceitável — documenta"): não há modo offline planejado. O que
funciona sem o servidor é o que roda na máquina (os motores, a casca local).

---

## Stack

- **Tauri 2** (núcleo Rust) + **WebView nativo do SO** (WKWebView no macOS,
  WebView2 no Windows, WebKitGTK no Linux).
- **Frontend da casca**: mínimo (Vite/TS) — tela de bootstrap/offline e config de
  URL. A UI principal é o **Blade remoto** do ShvIA.
- **Sidecar Python** (PyInstaller) — padrão herdado do SHVTERM; **opcional na
  F1**, estrutural na F2 (vault de token no keychain, SSO, ações nativas na API).
- **Plugins Tauri**: `updater`, `process`, `store`, `notification`, `deep-link`,
  `single-instance`.
- **Três runtimes** (como o SHVTERM): `npm` (casca) · `cargo` (Rust) · `pip`
  (sidecar).
- **CI**: GitHub Actions roda testes/lint (`ci.yml`, desde 09/2026). Empacotamento/
  assinatura de release seguem locais por decisão (`build-local.*`) — a matriz
  `macos`/`windows`/`ubuntu` via `tauri-action` foi removida na 0.4.6 por custo e não
  voltou, porque runners macOS custam 10x mesmo no plano Enterprise atual.

---

## Modelo organizacional

- **Repositório reaproveitado** — este mesmo repo (`samirhvbr/SHVIA-DESKTOP`,
  branch `master`). O nome encaixa: **SHVIA-DESKTOP = o desktop do ShvIA**. O
  histórico do fork foi preservado em `archive/claude-fork` (+ tag) e
  empurrado ao `origin`.
- **Repo separado do Laravel.** O ShvIA (Laravel) permanece **intocado** no seu
  próprio repo. Não é monorepo: o desktop é cliente de um servidor já hospedado e
  compartilhado com o web app; juntá-los só acoplaria cadências de release.
- **SHVTERM** continua repo **irmão** (cliente SSH) — dele a gente **colhe
  ativos** (CI, updater, packaging, convenções), sem merge.
- **Branding:** identidade ShvIA/Blue3 (ícones/splash de `brand/`, vindos de
  `/Users/samir/x/IA/brand/`). App ID sugerido: `cloud.blue3.shvia` (alinhado ao
  `cloud.blue3.shvterm`). **Nenhuma** marca Claude/Anthropic em artefato algum.
- **Commits:** `versão - comentário em português` (bump de `version.md` no mesmo
  commit). Sem `feat:/fix:/chore:`.

---

## Estrutura do repositório (alvo)

> Layout-alvo da Fase 1. O esqueleto (`src/`, `src-tauri/`, `scripts/` e as
> configs) **já existe** desde a `0.2.0`; `sidecar/` e `.github/workflows/` entram
> nas fases seguintes. Detalhe e passo-a-passo em
> [docs/roteiro-fundacao.md](docs/roteiro-fundacao.md).

```
shvia-desktop/
├── version.md                  # fonte única X.Y.Z (0.1.0)
├── README.md                   # este arquivo
├── CLAUDE.md / AGENTS.md       # convenções do agente (espelhados)
├── brand/                      # ícones/splash ShvIA (copiar de IA/brand/)
├── src/                        # casca web mínima (bootstrap, tela offline, config URL)
├── src-tauri/
│   ├── tauri.conf.json         # version gerada de version.md na CI
│   ├── src/                    # Rust: janela, tray, deep-link, notifications, updater, store
│   ├── capabilities/           # allowlist por janela (postura de segurança)
│   └── icons/
├── sidecar/                    # (F2) serviços nativos/seguros em Python
├── scripts/packaging/          # appimage/deb/rpm adaptados do SHVTERM
├── .github/workflows/          # ci.yml (testes/lint); matriz de release ainda local
├── .claude/                    # perfil do agente: permissões + effort (sem modelo)
├── .continue/                  # WIP: estado-atual + escopo do projeto
└── docs/                       # documentação técnica estável
```

---

## Versão (`version.md`)

`version.md` guarda a versão do **app desktop** (linha própria, independente da
versão do servidor ShvIA), no formato `X.Y.Z`:

- **X** — versão estável (manual).
- **Y** — nova capacidade de runtime, redesenho de IPC, mudança de auth-handoff.
- **Z** — incremento: mudança visível de UI/menu/janela, nova capacidade de
  empacotamento, ajuste de build.

**Acoplamento com o servidor:** cada release registra a **versão mínima do
servidor ShvIA** compatível, verificada em runtime lendo o campo `version` de
`GET /api/v1/health`. Na CI, `tauri.conf.json` recebe a versão de `version.md`
(fonte única). Bump **no mesmo commit** da mudança.

---

## Relação com ShvIA e SHVTERM

| Repo | Papel aqui |
|------|------------|
| **ShvIA** (`/Users/samir/x/IA`, Laravel) | **Servidor/fonte da verdade.** O desktop carrega o Blade e consome `/api/v1`. Intocado. |
| **SHVTERM** (`/Users/samir/Projetos/SHVTERM`, Tauri) | **Base técnica.** Colhemos CI, updater, packaging, sidecar e convenções. Repo irmão, sem merge. |
| **archive/claude-fork** (neste repo) | Snapshot do fork Claude Desktop descartado. Histórico preservado. |

---

## Roadmap em fases

Resumo (detalhe em [.continue/escopo-projeto.md](.continue/escopo-projeto.md)):

| Fase | Entrega |
|------|---------|
| **F0** | Decisões + colheita de ativos do SHVTERM + branding do ShvIA. |
| **F1** | Esqueleto andante: Tauri abre `ai.shvia.org`; **smoke-test de streaming SSE no Linux** (risco #1); login por cookie; ícone/título ShvIA. **Já entrega "mesmas funções".** |
| **F2** | Polish nativo: tray, menu, About, estado de janela, config de URL, tela offline, deep-link `shvia://` + reconciliação de auth, notificações. (Sidecar entra aqui se necessário.) |
| **F3** | Check de compatibilidade de versão de servidor (`/api/v1/health`). |
| **F4** | CI + assinatura/notarização (macOS, Windows EV, Linux) + auto-update. |
| **F5** | Beta nos 3 SOs + correções de quirks de WebView + docs. |

**Esforço estimado:** ~5–6 semanas-engenheiro para 1.0 assinado/notarizado nos 3
SOs. Long pole de **prazo** (não de eng): **procurement do cert EV Windows** —
iniciar no dia 1.

---

## Status atual

**30/06/2026 — primeira versão lançada (`0.4.5`).** **Fase 1 completa e validada**
(app abre, loga por cookie, **chat com streaming SSE** funciona) e **Fase 2** bem
encorpada: **multi-janela** (Ctrl+N), **branding** (seta Blue3 P&B + "AI" navy),
estado de janela persistido, links externos no navegador, **tela offline** e
empacotamento **local** (`.deb`/`.AppImage`/`.rpm` via `build-local`). Detalhe do que funciona em
[docs/funcionalidades.md](docs/funcionalidades.md); como buildar em
[docs/build.md](docs/build.md).

**1 pendência conhecida** (limitação do WebKitGTK no Linux — ADR-008): **mic
(voz)** não funciona no empacotamento Linux local; macOS/Windows tendem a resolver,
com fallback Electron se virar must-have. Contexto vivo e pendências em
[.continue/estado-atual.md](.continue/estado-atual.md). **Ctrl+V de imagem** está
implementado (`CLIPBOARD_IMAGE_PASTE_JS` injetado no host do servidor) — atualização
de docs pendente em [.continue/estado-atual.md](.continue/estado-atual.md).

> **Sem doc, sem deploy.** Toda função nova vira doc em `docs/` antes de entrar.
