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

## ADR-011 — Notificações nativas dos alertas de preço (ponte via canal do Modo Code)

- **Data:** 15/07/2026 · **Status:** Aceito
- **Contexto:** O ShvIA ganhou o **rastreador de preços** (SHVIA 2.18–2.19), com
  alertas quando um preço bate o alvo/atinge novo mínimo/cai. In-app há um badge;
  fora do app, o **Telegram** cobre a janela fechada. Faltava o caso do meio: a
  **janela do desktop aberta mas em segundo plano** — o usuário não vê o badge e
  não quer depender do Telegram. Notificação nativa do SO era item planejado da
  **F2** (tray/notificações), até aqui não construído (zero código de notificação
  no `src-tauri`).
- **Decisão:** disparar **notificações nativas do SO** para os alertas de preço,
  reusando a infraestrutura que já existe — **nenhum bridge nativo novo**:
  - **Plugin:** `tauri-plugin-notification`, usado **só pela API Rust**
    (`app.notification().builder().title().body().show()`) em
    `code_bridge::notify`. **Nenhuma capability** é adicionada — a origem remota
    **não** ganha acesso ao comando (mantém o ADR-001, igual às pontes TTS/Code).
  - **Canal página→Rust:** o **mesmo** message-handler do Modo Code (`shviaCode`
    no WebKit, `window.chrome.webview` no WebView2), que já converge para
    `code_bridge::handle_message` em todos os SOs. Bastou uma **ação `notify`**
    (fire-and-forget, sem `reqId`/reply) no dispatch — os 3 bridges nativos
    (`configure_linux_webview`, `macos_ipc`, `windows_ipc`) **não** foram tocados.
  - **Gatilho (página):** um shim injetado em `on_page_load` **só nas páginas
    remotas** (`PRICE_ALERT_NOTIFY_JS`) faz *polling* de
    `GET /api/v1/price-alerts?unread=1` (cookie same-origin) a cada 60 s e, ao ver
    ids novos, posta `{action:'notify', title, body}` no canal nativo. Dedup
    **persistente por id em `localStorage`** (`shvia_pt_notified`) evita repetir e
    coordena múltiplas janelas (a 1ª a gravar o id ganha); >3 novos viram **uma
    notificação-resumo** (anti-blast). Sem canal nativo (navegador puro/mobile) o
    shim é **no-op** (o badge in-app cobre).
- **Por que NÃO é comando Tauri (ADR-001):** o canal é o **message-handler nativo
  do próprio WebView** (não a IPC do Tauri), e a API do plugin é chamada **só do
  Rust** — a superfície de comandos à página remota **continua fechada**, como nas
  pontes de TTS e Modo Code.
- **Consequências / limites:** o *polling* é do lado da página (o cliente não tem o
  cookie de sessão no Rust), throttle de 60 s — alertas de preço não são
  tempo-real, então é de sobra. Notificação é **informativa** (v1 sem
  clique→abrir `/precos`; o badge navega). No **macOS**, notificações exigem app
  **empacotado/assinado** (bundle id `cloud.blue3.shvia`) — em `tauri dev` podem
  não aparecer; teste real é no `.app` build. **Validação:** `cargo check`/`clippy`
  passam; o **teste ao vivo é na máquina do Samir** (o build final não roda daqui).

## ADR-012 — Timeout do ping de alcance: o cold-start do WebKitGTK custa ~5-6 s

- **Contexto/Problema:** a casca de bootstrap (`src/main.ts`) faz um ping em
  `/api/v1/health` antes de navegar; alcançável → `location.replace`, senão → tela
  "offline" com auto-retry (backport do splash do SHVIA-MOBILE). Em produção o app
  caía em "offline" **mesmo com o servidor a <1 ms** (rede local). Reproduzido no
  **mesmo webkit2gtk-4.1** do wry, a partir da origem real `tauri://localhost`
  (registrada secure/local):
  - Fora da engine: DNS resolve em <10 ms (só registro A, sem AAAA); `curl` faz
    TLS + HTTP 200 em ~0,35 s. Rede e servidor estão perfeitos.
  - **Dentro do WebKit:** o **1º request depois que o processo de rede sobe "frio"**
    custa **~5-6 s** (medido 5,1 / 5,6 / 6,3 s) ANTES de qualquer resposta; aquecido,
    cai p/ 50-400 ms. **Independe do modo** — invertendo a ordem, quem stalla é
    sempre o 1º request (`no-cors` OU `cors`); não é DNS nem o servidor, é o
    cold-start da engine (provável revalidação de certificado + init do NetworkProcess
    na 1ª conexão TLS).
  - O gate tinha timeout de **6000 ms** → o custo frio batia na trave, o
    `AbortController` abortava e o `catch` mandava pra "offline".
- **Decisão:** subir `REACHABLE_TIMEOUT_MS` de **6 s → 15 s** (~2,5× o pior custo
  frio medido, cobrindo a contenção de CPU/IO do launch — Tauri + WebKit + app web
  subindo juntos). Mantido o `no-cors` (o modo não é a causa; a resposta opaca basta
  pra "está alcançável"). Auto-retry (5 s) e "Abrir mesmo assim" seguem como escape.
- **Consequências/limites:** offline **de verdade** rejeita na hora (DNS/rota falha),
  então a folga de 15 s não pesa no caso comum — só um servidor "presente mas mudo"
  esperaria os 15 s (e o escape está sempre visível). O custo frio de ~5-6 s **é
  inerente** ao WebKitGTK: a navegação pro FQDN pagaria o mesmo se o ping não
  pagasse — o ping funciona como warm-up e o app carrega instantâneo em seguida.
  **Não relitigar** baixando o timeout "porque parece muito": ele existe pra caber o
  cold-start (ver ADR-006/008 — WebKitGTK é o risco recorrente do Linux). Se o custo
  frio for atacado na fonte (ex.: desligar revalidação de certificado na engine), aí
  sim dá pra reduzir. Harness de repro: `gjs` + WebKit2-4.1 (registra `tauri://`,
  roda os fetches, exfiltra o tempo por `document.title`).
## ADR-013 — `is_internal` precisa aceitar o esquema `tauri://` (casca empacotada)

- **Contexto/Problema:** o 0.9.0 endureceu `is_internal` (lib.rs) trocando a
  checagem "só host" por uma allowlist **host + esquema** — para impedir que um
  subdomínio `*.blue3.com.br` comprometido carregasse dentro do app e ganhasse a
  ponte nativa. A allowlist aceitava `https` (servidor) e `http`
  (`localhost`/`tauri.localhost`). No macOS o app empacotado passou a abrir com a
  **janela totalmente BRANCA**.
- **Causa:** a URL da casca empacotada **muda por SO** (Tauri
  `AppManager::tauri_protocol_url`, tauri 2.11.3 `src/manager/mod.rs:339`):
  `http://tauri.localhost` **só** no Windows/Android; em **macOS e Linux** é
  `tauri://localhost` — esquema **`tauri`**, não `http`. Com o esquema fora da
  allowlist, `is_internal` devolvia `false` para a **própria navegação inicial**
  (`on_navigation` roda no load inicial, não só em links), o handler negava a
  navegação e ainda mandava `tauri://localhost/index.html` pro `opener` do SO
  (no-op) — resultado: WebView vazia, sem erro, sem log.
- **Por que passou no dev:** em `tauri dev` a janela abre o `devUrl`
  (`http://localhost:1420`), que **está** na allowlist. O bug é **invisível fora
  do build empacotado** — e invisível no Windows, o único SO cuja casca prod usa
  `http`.
- **Decisão:** aceitar `"tauri" => host == "localhost"` na allowlist, junto de
  `http` (dev + prod Windows/Android) e `https` (servidor). O endurecimento
  continua de pé: o esquema `tauri` só vale para o host `localhost` (a casca
  embutida no binário), e a ponte nativa segue injetada **apenas** em
  `SERVER_HOST` no `on_page_load` — a casca local nunca a recebe.
- **Consequências/limites:** cobertos por teste de unidade
  (`tests::casca_local_e_interna_nos_tres_sos`), que trava as **três** URLs de
  casca — `tauri://localhost`, `http://tauri.localhost`, `http://localhost:1420`.
  **Não relitigar** removendo o esquema `tauri` "porque não parece http": ele é a
  origem real do app empacotado no macOS/Linux. Regra geral ao mexer em
  `is_internal`: rodar `cargo test` e validar num **build empacotado**, nunca só
  em `tauri dev`.

## ADR-014 — Motor paralelo "Claude Code (assinatura)" no Modo Code

- **Contexto/Problema:** o Modo Code roda sobre o `anna` (loop no cliente,
  inferência pelo **gateway** do SHVIA — auditoria/quota/LGPD/custo herdados).
  Quer-se também aproveitar a **assinatura Pro/Max** do usuário, usando o cliente
  oficial do Claude Code. A via de "OAuth de assinatura" de gateways como o
  9router foi **descartada**: o mecanismo dela é falsificação de identidade +
  evasão anti-abuso (billing header falso, device/account fabricados, decoy
  tools) — o próprio repo de origem a marca `RISK_NOTICE/deprecated`.
- **Decisão:** um **motor paralelo** — `claude-runner` (Node + Claude **Agent
  SDK**) que orquestra o **cliente oficial** e fala o **mesmo NDJSON do `anna`**
  (`SHVIA-CODE/docs/embedding.md`). O `code_bridge.rs::spawn` escolhe pelo campo
  `engine` (`'claude'` → runner; ausente/`'gateway'` → `anna`); `resolve_bin`
  generaliza o antigo `resolve_anna`. **Bridge e UI de cards ficam intactos** (é
  drop-in do protocolo). Auth = **assinatura** via `claude login`/`setup-token`
  (o runner **remove `ANTHROPIC_API_KEY`** p/ forçar o fallback; **sem API key**);
  o app **nunca embute o login** (ToS: a Anthropic proíbe terceiros oferecerem
  login claude.ai). Política de permissão = **PreToolUse hook + `settingSources:
  []`** (autoridade única: bypassa allow-rules e não herda o `~/.claude/
  settings.json` pessoal — garante "nada roda/escreve sem o dev ver").
- **Trade-off (consciente):** este motor **sai do gateway** — a inferência vai
  direto do `claude` à Anthropic, então **não há** auditoria/quota/LGPD/medidor
  neste modo. É intrínseco a "usar a assinatura". Por isso é **toggle paralelo**
  ("Motor: SHVIA gateway | Claude Code assinatura"), **não** substituto do `anna`.
- **Consequências/limites:** o patch do bridge é **retrocompatível** (sem
  `engine` = `anna`, comportamento idêntico; `cargo check`/`clippy` verdes). O
  runner é instalado no **padrão anna** (`claude-runner/install.sh` →
  `~/.local/bin/claude-runner`, resolvido por PATH/local). Pré-req: **Node 18+** e
  Claude Code **autenticado**. O seletor de **modelo** do Claude (opus/sonnet/…)
  e o **toggle na UI** (SHVIA-WEB) com aviso "fora do gateway" ficam para o passo
  seguinte. Validado ao vivo (spike): auth por assinatura, streaming, e gates nos
  dois sentidos (leitura=auto; `Bash`/`Write`/`Edit`=card; approve executa,
  reject bloqueia).

---

## ADR-015 — `saveFile` na ponte: salvar artefato gerado com diálogo nativo

- **Contexto/Problema:** com a geração de imagem no ar (SHVIA 2.48/2.49), o
  artefato passou a ser um **resultado que o usuário quer guardar** — imagem,
  SVG, markdown, script. O menu de contexto do WebView ("Baixar imagem") não
  resolvia: no macOS o WKWebView baixa **sozinho para `~/Downloads`**, sem
  perguntar e sem avisar (o usuário clicou e concluiu que "não aconteceu nada"),
  e num `<a download>` o comportamento varia por SO. Não havia como **escolher
  onde salvar**, que é o que se espera de um app nativo.
- **Decisão:** uma ação nova na ponte — `saveFile({name, dataBase64})` → diálogo
  nativo (`dialog().file().set_file_name().save_file()`, o irmão do `pickFolder`
  do ADR-005) → escreve onde o usuário escolher e responde `{saved, path}`.
  **Quem lê os bytes é a PÁGINA**, não o Rust: o artefato vive em
  `/api/v1/files/{id}`, que é **autenticado por sessão**, e a sessão mora na
  WebView. Mandar a URL obrigaria o lado nativo a reproduzir autenticação; com os
  bytes prontos ele só abre o diálogo e escreve — mantém o Rust ignorante de
  auth, que é a mesma postura do resto da ponte.
- **Segurança:** nada muda no perímetro — mesma mensagem nativa, mesmo token de
  capacidade por sessão (iframe cross-origin segue descartado), **sem comando
  Tauri e sem capability nova** (ADR-001 de pé). O `name` vem da página, então é
  sanitizado para **basename** (sem `/`, `\`, `..`, `:` do macOS, controles e
  curingas) — o diálogo escolhe a pasta e o campo do nome não pode reintroduzir
  caminho por cima dela. Teto de 50 MB no artefato.
- **Consequências/limites:** retrocompatível nos dois sentidos. O web testa
  `typeof __shviaCode.saveFile === 'function'` e, sem a ponte nova, cai em
  `showSaveFilePicker` (Chromium) e depois em `<a download>` + toast — ou seja,
  **desktop antigo continua funcionando** com o comportamento atual. Cobertura:
  4 testes de `sanitize_filename` (`cargo test`). Cancelar o diálogo **não é
  erro** (`{saved:false}`, sem toast). O ganho só chega às máquinas com **build
  novo** — ciclo diferente do deploy web, que é imediato.

---

## ADR-016 — Domínio próprio `ai.shvia.org`: dual-host por allowlist exata, e host deixa de decidir marca

- **Contexto/Problema:** o ShvIA saiu do domínio corporativo (`ia.blue3.com.br`)
  para o próprio (`ai.shvia.org`) — a Blue3 ficou como financiadora, não como
  marca do produto. O apontamento estava espalhado em quatro repositórios
  versionados independentemente (este, SHVIA-MOBILE, SHVIA-CODE e SHVIA-WEB), e
  três dos pontos são **blocantes silenciosos**: `SHVIA_URL` (o destino), a
  allowlist de navegação interna (`SERVER_HOSTS`, antes `SERVER_HOST` único) e o
  `connect-src` da CSP da casca. Errar qualquer um não dá erro legível: com o
  host fora da allowlist, `on_navigation` classifica a **própria navegação
  inicial** como link externo e o ShvIA abre no navegador do SO com a janela
  presa no splash; com o host fora do `connect-src`, o ping de alcance é barrado
  pelo WebView e a casca fica em "Sem conexão" para sempre, com o servidor no ar.
- **Decisão:** **dual-host durante a transição**, por allowlist EXATA de FQDN.
  `SERVER_HOST` (canônico, o que o app abre e o que o modal "Sobre" informa na
  casca) = `ai.shvia.org`; `SERVER_HOSTS` (o que é aceito como interno e recebe
  as pontes nativas) = `ai.shvia.org`, `ia.shvia.org` e `ia.blue3.com.br`. As três
  foram verificadas por DNS: o canônico e o legado respondem no **mesmo IP**
  (200.36.196.254) e `ia.shvia.org` é CNAME do canônico. Desligar o domínio
  legado é **remover uma linha** de `SERVER_HOSTS` (e a gêmea em
  `windows_ipc.rs::ALLOWED_MESSAGE_ORIGINS`) — nada mais.
- **O ápex `shvia.org` fica FORA, de propósito:** ele resolve para outro IP
  (170.233.231.20) e serve a landing, não o app. `is_internal` é a **única**
  fronteira que decide quem recebe `window.__shviaCode` — spawn de processo
  local, leitura de FS, token de capacidade. Pôr um host que não é o app nessa
  lista é exatamente o buraco que o endurecimento do 0.9.0 fechou. Coberto por
  teste (`tests::apex_shvia_org_e_externo`).
- **Marca no splash:** o `<span>Blue3</span>` do rodapé saiu e o `brand-mark.png`
  (a seta da Blue3 em P&B + "AI") virou `brand-mark.svg` — a mesma marca do
  favicon e do badge da sidebar do web. O splash é **pré-login** e aparece para
  todo usuário, inclusive quem não é `@blue3.com.br`; a regra da migração é que a
  Blue3 só apareça **depois** do login e só para e-mail dela
  (SHVIA-WEB/`config/brand.php`). SVG e não PNG porque o WebView renderiza vetor
  nativamente — nítido em qualquer DPI, transparente por natureza (o splash é
  escuro; PNG com fundo branco apareceria como caixa).
- **Modal "Sobre":** a linha "Servidor" passou a mostrar `location.hostname` da
  página remota, em vez de casar o host contra uma lista fixa — durante a
  migração a pergunta do suporte é "esse binário aponta pra onde?", e uma lista
  fixa mentiria no dia em que um host novo entrasse. O fetch de
  `/api/v1/health` virou **sempre relativo** e só roda em página remota: tentar
  FQDN absoluto da casca local (origem `tauri://localhost`) é beco sem saída — o
  CORS do servidor só libera `FRONT_DOOR_ORIGINS` (SHVIA-WEB/`config/cors.php`),
  a casca não está nem deve estar nessa lista, então o navegador barra a leitura
  e a linha cai em "—" de qualquer jeito. Na casca não há versão de servidor para
  mostrar, e "—" é a resposta honesta.
- **O que NÃO muda:** o `identifier` `cloud.blue3.shvia` fica. Ele é a chave do
  sandbox/app-data em todos os SOs — trocar zera cookie de sessão, geometria de
  janela (`tauri-plugin-window-state`) e o `modo-code-bindings.json` (os vínculos
  projeto→pasta do Modo Code), e no macOS o SO passa a tratar como app NOVO
  (assinatura/notarização e permissões pedidas de novo). Desde a 2.51.0 do
  SHVIA-WEB há um acoplamento **servidor→bundle** novo: `APNS_BUNDLE_ID` tem de
  ser igual ao bundle id, então renomear agora quebraria push no iOS **em
  silêncio**. Se um dia for feito, é em commit próprio e deliberado, nunca junto
  de uma troca de domínio.
- **Consequências/limites:** trocar de origem **desloga todo mundo uma vez** — o
  cookie de sessão é por origem, e o mesmo vale para `localStorage`/`IndexedDB`
  do WebView (inclusive o dedupe `shvia_pt_notified` dos alertas de preço, que
  pode disparar uma notificação-resumo no primeiro acesso). O binário antigo
  instalado continua abrindo o host legado: é por isso que o legado **precisa**
  seguir no ar até a frota atualizar. No mobile a atualização passa por **review
  da Apple**, então o build novo tem de ser submetido ANTES de qualquer redirect
  do host antigo, não depois. Cobertura: `cargo test` (allowlist, incluindo ápex
  e sufixo-armadilha `ai.shvia.org.evil.com`) e `npm run build`.

## ADR-017 — Notificação: badge no ícone entra, clique→navegar não é possível com o plugin

- **Data:** 27/07/2026 · **Status:** Aceito · **Revisa:** [ADR-011](#adr-011--notificações-nativas-dos-alertas-de-preço-ponte-via-canal-do-modo-code)
- **Contexto:** o ADR-011 entregou notificação nativa e registrou como limite "v1 sem
  clique→abrir `/precos`; o badge navega", deixando o clique como coisa a fazer
  depois. Ao ir implementar (item **D8** do
  [comparativo 9router × hermes](../../SHVIA-WEB/docs/comparativos/9router-hermes.md)),
  a leitura do `tauri-plugin-notification` **2.3.3** mostrou que não é questão de
  esforço: o `desktop.rs` expõe só `title`, `body`, `icon`, `sound` e `show`. Não há
  callback de clique nem ação. O `register_action_types` e o handler de ação existem
  **apenas no `mobile.rs`**.
- **Em paralelo,** o servidor mudou: o ShvIA 2.60–2.63 passou a produzir notificação
  para resultado de **rotina**, fim de **lote** e aviso de **destino de entrega
  morto**, todos pelo mesmo `DeliveryRouter`, e expôs `GET /api/v1/notifications`. O
  shim daqui pollava `/api/v1/price-alerts?unread=1` — conhecia **um** tipo de evento
  e não veria nenhum dos novos.
- **Decisão:**
  1. **Poll na rota genérica** `/api/v1/notifications?unread=1`. O desktop passa a
     acompanhar o servidor sem precisar de um poll novo por feature. Título e corpo
     vêm **achatados** do servidor, então a casca não conhece a estrutura do Laravel.
  2. **Badge no ícone** via `WebviewWindow::set_badge_count`, nova ação `badge` na
     ponte (fire-and-forget, como o `notify`). Postado em **todo** poll, não só
     quando há novidade — é o que faz a contagem **zerar** quando o usuário lê no
     painel.
  3. **Clique→navegar fica FORA**, e não como dívida: é limitação do plugin no
     desktop. Se algum dia importar de verdade, o caminho é trocar de plugin ou
     chamar a API do SO direto — decisão de outra ordem, não um "to-do".
- **Consequências / limites:**
  - **Windows não tem badge.** `set_badge_count` é `Unsupported` lá; o caminho é
    `set_overlay_icon`, que pede uma **imagem** com o número desenhado, não um
    inteiro. Ficou de fora conscientemente: renderizar dígito em `Image` a cada
    mudança de contagem é trabalho de outra ordem para retorno pequeno.
  - Sem clique, **o badge é o único sinal persistente** depois que o toast do SO
    desaparece. É por isso que ele deixou de ser enfeite e passou a ser a peça
    central do item.
  - Erro de `set_badge_count` é **silencioso**: ambiente sem suporte (Windows, alguns
    WMs de Linux) não é motivo para poluir o log a cada 60 s.
  - No **macOS**, notificação continua exigindo app empacotado/assinado — em
    `tauri dev` pode não aparecer (limite herdado do ADR-011).
  - **Validação:** `cargo clippy -D warnings` passa. O **teste ao vivo é na máquina do
    Samir**: com uma rotina do ShvIA entregando em `inapp`, a notificação e a
    contagem no dock têm de aparecer com a janela em segundo plano.
