# Decisões de Arquitetura (ADRs) — ShvIA Desktop

Registro das decisões estruturais, formato ADR (contexto → decisão →
consequências → alternativas). Não relitigar uma decisão aqui dentro de um
how-to; linkar o ADR.

---

## ADR-001 — Base = SHVTERM (Tauri 2); fork Claude descartado

- **Data:** 30/06/2026 · **Status:** Aceito
- **Contexto:** Precisávamos de um app desktop multiplataforma para o ShvIA. Dois
  candidatos a base internos: (a) o fork `claude-desktop-debian` (Electron,
  Linux-only, empacota o Claude Desktop); (b) o SHVTERM (Tauri 2 + React, cliente
  SSH multiplataforma, com CI dos 3 SOs, updater e packaging prontos).
- **Decisão:** Usar o **SHVTERM como base técnica**. **Descartar o fork** Claude —
  ele é Electron Linux-only e atado à marca Anthropic; o SHVTERM já resolve o que
  precisamos e é cross-platform.
- **Consequências:** Stack = Tauri 2. Ganhamos CI/updater/packaging/UX nativa
  prontos. O fork foi **arquivado** em `archive/claude-fork` + tag
  `archive/claude-fork-v0.2.2` (push no `origin`) — nada perdido.
- **Alternativas:** manter o fork Electron (rejeitado: Linux-only, marca
  Anthropic, Chromium pesado). Começar do zero (rejeitado: SHVTERM já pronto).

---

## ADR-002 — Shell fino: Tauri carrega o ShvIA web (Blade) remoto

- **Data:** 30/06/2026 · **Status:** Aceito
- **Contexto:** O ShvIA é um app **server-rendered** (Laravel + Blade + Alpine),
  com uma API `/api/v1` rica. Objetivo: "mesmas funções" e "a cara do ShvIA",
  rápido. Duas formas: (A) janela Tauri **carrega o Blade remoto**; (B) UI React
  própria sobre a API.
- **Decisão:** **Forma A (shell fino).** A janela navega `https://ia.blue3.com.br`;
  a UI é o próprio Blade do ShvIA. Zero rewrite, "mesmas funções" literal.
- **Consequências:** F1 entrega tudo sem código de UI. Camada nativa fica fina
  (janela/tray/deep-link/notificações/updater). Risco: streaming SSE no WebKitGTK
  (ADR-006). Forma B fica como evolução opcional (F2+) para telas desktop-only.
- **Alternativas:** Forma B / React-on-API (rejeitada na F1: exige paridade de API
  + reescrever telas; mantém duas UIs). Híbrido (a forma A já é o primeiro passo
  natural de um híbrido).

---

## ADR-003 — Tauri 2, não Electron, não NativePHP

- **Data:** 30/06/2026 · **Status:** Aceito
- **Contexto:** Escolha de runtime para empacotar.
- **Decisão:** **Tauri 2** (WebView nativo do SO + núcleo Rust).
- **Consequências:** Binário enxuto (sem Chromium embarcado, ~120 MB/build a
  menos). Custo: WebView varia por SO (WKWebView/WebView2/WebKitGTK) → risco de
  inconsistência, concentrado no WebKitGTK (ADR-006).
- **Alternativas:**
  - **Electron** — rejeitado (Chromium pesado, e o Tauri do SHVTERM já existe).
    Mantido como **fallback** se o WebKitGTK quebrar o SSE.
  - **NativePHP** — rejeitado **por restrição de dados**: o ganho dele é SQLite
    local, mas o ShvIA **exige MariaDB/MySQL e proíbe SQLite** (CI fixa
    `mariadb:11.4`). Embutir MySQL por cliente forkaria a camada de dados.

---

## ADR-004 — Servidor remoto é a fonte da verdade (online-first)

- **Data:** 30/06/2026 · **Status:** Aceito (pendente confirmação de produto)
- **Contexto:** O ShvIA hospedado detém dados, senhas, permissões. O chat de IA já
  depende de provedores server-side.
- **Decisão:** O desktop é **online-first / efetivamente online-only**. **Não abre
  banco no cliente** — a regra "nunca SQLite" é satisfeita por construção. Estado
  local = só URL do servidor + estado da janela.
- **Consequências:** Sem servidor, o app não funciona — endereçado com **tela
  offline** + retry. Operação genuinamente offline é um *killer* (exigiria
  NativePHP+MySQL embutido ou reescrita — outro projeto).
- **Pendência:** confirmar "online-only é aceitável" como propriedade assinada
  (ver escopo §7).

---

## ADR-005 — Auth por cookie de sessão Sanctum, same-origin

- **Data:** 30/06/2026 · **Status:** Aceito (verificar middleware em código na F1)
- **Contexto:** `/chat` e `/compare` usam auth de **sessão web**; `config/sanctum`
  com `guard=['web']`. Bearer token só autentica `/api/v1`, **não** as rotas Blade.
- **Decisão:** Na F1, **não há plumbing de token**. A janela navega o FQDN real →
  login é a tela Breeze normal → cookie de sessão autentica tudo, como num browser.
- **Consequências:** F1 simples. Persistência da sessão entre reinícios depende do
  WebView guardar o cookie (validar). O **deep-link SSO `shvia://`** é a única
  origem não-FQDN → tratar na F2 (`SANCTUM_STATEFUL_DOMAINS` / handoff). Ações
  nativas que usem `/api/v1` podem mintar Bearer via `POST /api/v1/auth/login` e
  guardar no keychain (pós-1.0, via sidecar).
- **A verificar (F1):** middleware exato de `/chat` em `routes/web.php`.

---

## ADR-006 — Streaming SSE no WebKitGTK é o risco #1 (smoke-test primeiro)

- **Data:** 30/06/2026 · **Status:** Aceito
- **Contexto:** O `/chat` transmite via `fetch` + `ReadableStream.getReader()`.
  Sólido em WebView2/WKWebView; o **WebKitGTK** (Linux) é o ponto fraco histórico
  de fetch-streaming.
- **Decisão:** O **primeiro passo de implementação** é um **smoke-test do streaming
  `/chat` no Linux/WebKitGTK**, antes de qualquer polish.
- **Consequências:** Se passar, caminho livre. Se falhar, **fallback Electron**
  (Chromium pinado), ainda thin-shell hospedado — nunca NativePHP.
- **A verificar (F1):** trecho de streaming em `public/js/app.js` (~linha 4003).
- **Atualização (0.2.2):** primeiro teste manual no **WebKitGTK (Linux)** foi
  **positivo** — a janela carregou o `/chat` e a ANNA respondeu com stats de
  geração (`tokens`/`tok-s`). Forte indício de que o fetch-streaming funciona;
  falta **cravar o token-a-token** (assistir uma resposta nova pintar aos poucos).
  Quirk **de ambiente** (não do SSE): WebKitGTK sem GPU (VM/NVIDIA) exige
  `WEBKIT_DISABLE_DMABUF_RENDERER=1` (render por software), senão a janela fica em
  branco.

---

## ADR-007 — Repo reaproveitado, separado do Laravel

- **Data:** 30/06/2026 · **Status:** Aceito
- **Contexto:** Onde o projeto mora.
- **Decisão:** **Reaproveitar este repo** (`samirhvbr/SHVIA-DESKTOP`, `master`) —
  o nome encaixa (SHVIA-DESKTOP = desktop do ShvIA). Histórico do fork preservado
  em `archive/claude-fork`. **Não** é monorepo com o Laravel: o desktop é cliente
  de um servidor já hospedado; juntá-los acoplaria releases.
- **Consequências:** ShvIA (Laravel) fica intocado. SHVTERM é repo **irmão** (colher
  ativos, sem merge).
- **Nota:** a análise sugeriu um repo novo `shvia-desktop-app`; preferimos
  reaproveitar este pelo nome e pelo histórico já no `origin`.
