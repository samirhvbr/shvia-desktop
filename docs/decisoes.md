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

- **Data:** 30/06/2026 · **Status:** Aceito · **risco RESOLVIDO no Linux** (ver Atualização 0.2.3)
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
- **Atualização (0.2.3) — RESOLVIDO (Linux):** smoke-test **confirmado**. Num prompt
  longo (`2.224 tokens`), o raciocínio e a resposta **pintaram token-a-token**
  (~30 tok/s) — capturas em sequência mostram o texto crescer frame a frame, com o
  botão **Parar** dinâmico. O fetch-streaming funciona no WebKitGTK; **fallback
  Electron descartado** por ora. Repetir o smoke em macOS/Windows na F5 (baixo
  risco — WebView2/WKWebView são fortes em streaming).

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

---

## ADR-008 — WebKitGTK: microfone e paste de imagem têm limite no Linux

- **Data:** 30/06/2026 · **Status:** Aceito (limitação conhecida)
- **Contexto:** Com o app rodando no **Linux (WebKitGTK)**, o ShvIA pediu
  **microfone** (entrada de voz) e **Ctrl+V de imagem** no chat. Por padrão o
  WebKitGTK **não expõe** `getUserMedia` nem deixa o JS ler imagem do clipboard.
- **O que fizemos (shell, Linux):** via `with_webview` ligamos
  `enable-media-stream`, `enable-mediasource`, `enable-webrtc` e
  `javascript-can-access-clipboard`, e tratamos o signal `permission-request`
  concedendo os pedidos de **mídia**. Com isso o `getUserMedia` **fica disponível**
  e o WebView **enumera o device** de áudio/câmera (ex.: "BRIO Ultra HD Webcam").
- **Limite encontrado:** mesmo assim, a **captura efetiva do microfone** e o
  **paste de imagem** **não funcionam** no WebKitGTK neste ambiente. É uma fraqueza
  **do WebKitGTK** (mídia/clipboard), **não do nosso código** — que faz a parte dele.
- **Consequências:** no **Linux**, **voz** e **colar imagem** ficam **indisponíveis**
  por ora (chat por **texto** + **streaming** intactos). **macOS (WKWebView)** e
  **Windows (WebView2/Chromium)** tendem a suportar — validar ao empacotar lá.
- **Saída se virar must-have:** **fallback Electron** (Chromium tem mídia/clipboard
  fortes) — ver ADR-003/006. É decisão de **produto**, não tomada agora.

---

## ADR-009 — Leitura em voz (TTS) no desktop Linux: ponte nativa espeak-ng

- **Data:** 04/07/2026 · **Status:** Aceito
- **Contexto:** O ShvIA ganhou o botão "ouvir a resposta" (TTS). No **desktop
  Linux (WebKitGTK 2.52)** o `window.speechSynthesis` **existe** (o botão aparece),
  mas o backend é o **Flite** — só **4 vozes en-US**, **nenhuma pt**. O `espeak-ng`
  pt-BR do SO (via speech-dispatcher) **não** é exposto ao WebView. Resultado: a
  Anna saía **muda** ou lendo português com **sotaque de inglês**. O **Plano B
  (TTS no servidor)** do ShvIA resolve, mas depende de infra (`TTS_HOST`) que pode
  não estar de pé.
- **Decisão:** um **fallback nativo, só no Linux**, que fala pelo **espeak-ng do
  SO** via `spd-say` (speech-dispatcher). A página posta o texto num **script
  message handler do WebKitGTK** — `window.webkit.messageHandlers.shviaTts`,
  registrado em `configure_linux_webview` — e o Rust roda
  `spd-say -w -o espeak-ng -l pt-BR`; ao terminar (ou ser descartado por `-C`),
  devolve `window.__shviaTtsEnded(gen)` por `eval`. `stop` = `spd-say -C`.
- **Por que NÃO é comando Tauri (mantém a postura do ADR-001):** o handler de
  mensagens de script é o **canal nativo do próprio WebKit**, não a IPC do Tauri —
  **não** habilitamos `invoke`/capabilities para a origem remota, então a
  superfície de comandos nativos à página **continua fechada**. É a mesma pegada
  das pontes já injetadas (tarja offline, colar imagem): roda na página remota,
  fora da CSP.
- **Preferência (cliente):** servidor (`SHVIA_TTS_ENABLED`) → ponte nativa (Linux)
  → `speechSynthesis` (navegador/macOS/Windows). macOS/Windows não precisam da
  ponte (WKWebView/WebView2 têm voz pt) — por isso é `#[cfg(target_os = "linux")]`.
- **Consequências / limites:** voz **robótica** (espeak-ng), mas **pt-BR correto e
  zero infra**. Depende de `spd-say` + `speech-dispatcher-espeak-ng` no SO (padrão
  no Debian/GNOME; se faltar, o shell avisa por toast e reseta o botão). `spd-say
  -C` cancela **global** no daemon (aceitável neste app pessoal). Se o Plano B
  subir, ele tem preferência (voz melhor). Supera o trecho de **saída de voz** do
  ADR-008 (a **entrada** por microfone segue limitada).

## ADR-010 — Ponte do Modo Code no Windows (WebView2)

- **Data:** 11/07/2026 · **Status:** Aceito
- **Contexto:** O toggle **Chat | Code** do ShvIA é *fail-safe*: só aparece quando
  a casca injeta `window.__shviaCode`. O shim (`BRIDGE_JS` em `code_bridge.rs`)
  só sabia falar por `window.webkit.messageHandlers.shviaCode` — o
  script-message-handler do **WebKit** (Linux/macOS). No **Windows o WebView é o
  WebView2 (Chromium)**, que **não tem `window.webkit`**; o shim dava `return` e
  `__shviaCode` nunca nascia → **o Modo Code não aparecia no Windows** (nunca foi
  construído — zero `cfg(target_os = "windows")` no projeto até aqui).
- **Decisão:** ensinar o transporte a falar **WebView2** também, espelhando o que
  `macos_ipc.rs` faz no WKWebView:
  - `BRIDGE_JS` detecta `window.chrome.webview` além do webkit e abstrai o envio
    (`sendNative`); Rust→página segue `eval`.
  - novo `windows_ipc.rs` (`#[cfg(target_os = "windows")]`): registra
    `add_WebMessageReceived` no `ICoreWebView2` (via `webview2-com` 0.38 +
    `windows` 0.61 — versões CASADAS com o wry 0.55) e repassa a string para
    `code_bridge::handle_message`. `with_webview` dá o controller; `take_pwstr`
    libera a string do WebView2.
  - `resolve_anna()` virou cross-platform: procura `anna(.exe)` **ao lado do app**
    (permite empacotar como resource), no **PATH** (`where` no Windows) e em
    `%LOCALAPPDATA%\Programs\anna` / `~/.local/bin` por SO.
- **Por que NÃO é comando Tauri (ADR-001):** `window.chrome.webview` é o canal
  nativo do **próprio WebView2**, não a IPC do Tauri — a superfície de comandos à
  página remota **continua fechada**, igual às pontes WebKit (TTS, Code no
  Linux/macOS).
- **Consequências / limites:** o **binário `anna.exe`** passa a ser um pré-requisito
  no Windows (o loop do Code roda no cliente). O `anna` (SHVIA-CODE) já é
  cross-platform no código (`session.rs` gateia o único unix-ism); adicionamos um
  **CI `build-windows.yml`** que compila e publica o `anna.exe`. Empacotar o
  `anna.exe` no instalador (para o usuário não instalar à mão) fica como próximo
  passo — hoje basta o `anna.exe` no PATH. **Validação:** `cargo check`/`clippy`
  **cruzados** para `x86_64-pc-windows-msvc` passam (tipos do WebView2 conferem);
  o **teste ao vivo é no Windows do Samir** (o build final não roda daqui).
