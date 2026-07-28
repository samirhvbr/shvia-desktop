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

## ADR-018 — Gate de versão cliente↔servidor: avisa, nunca bloqueia

- **Data:** 27/07/2026 · **Status:** Aceito
- **Contexto:** o modal Sobre já lia `version.app` do `GET /api/v1/health` para
  mostrar a versão do ShvIA, mas o uso era **puramente informativo** — não havia como
  o servidor dizer "essa casca é velha demais para o que eu mudei". Item **D6** do
  [comparativo 9router × hermes](../../SHVIA-WEB/docs/comparativos/9router-hermes.md),
  e pré-requisito honesto do **D1** (auto-update): não se força atualização sem antes
  saber avisar.
- **Decisão:** o **servidor** declara o que espera, em
  `version.clients.desktop` do `/api/v1/health` (ShvIA 2.64.0):
  `min_version`, `latest_version`, `changelog_url`, `notice`. O cliente compara com o
  build do pacote e:
  - **abaixo do `min_version`** → tarja dispensável no rodapé, com o motivo e o link
    do changelog;
  - **abaixo do `latest_version`** → **nada na tela**. "Existe uma versão nova" não é
    problema, e virar tarja para isso é o caminho mais curto para o usuário aprender
    a ignorar tarjas.
- **AVISA, nunca bloqueia — e isso é decisão, não preguiça:** esta casca é **fina**,
  a UI é o Blade do próprio servidor. Na quase totalidade dos casos o cliente velho
  **funciona**, só perde uma ponte nativa nova. Bloquear transformaria um
  `min_version` digitado errado no servidor numa **interrupção total** de todo mundo.
  Quebra real de contrato se resolve na **rota específica** devolvendo erro, não num
  gate genérico de versão.
- **Detalhes que evitam gate irritante:**
  - **Dispensar é lembrado por versão DO SERVIDOR** (`shvia_vg_dismissed`), não por
    booleano. Se o servidor subir de novo pedindo outra coisa, o aviso volta — com um
    booleano, o usuário dispensaria uma vez e nunca mais seria avisado.
  - **Comparação numérica campo a campo.** Como string, `0.9.2 > 0.13.0` — é o erro
    clássico de comparar versão.
  - **Fail-open em cada passo:** sem canal nativo, sem rede, JSON inesperado, versão
    não-parseável → **no-op**. Gate que se engana e atrapalha é pior que gate nenhum,
    porque o custo cai em cima de quem está tentando trabalhar. O `min_version`
    default no servidor é `0.0.0`, então um deploy que esqueça de configurar não
    avisa nada.
  - O link do changelog é `https` externo, então o `on_navigation` já o manda para o
    **navegador do SO** — o usuário não perde a sessão do ShvIA.
- **Consequências / limites:** o gate roda só em **página remota** (`is_server_host`),
  como as outras pontes; da casca local não há sessão nem CORS. Não há
  auto-atualização — isso é o **D1**, e este ADR é o pré-requisito dele.
  **Validação:** `cargo clippy -D warnings` passa. O **teste ao vivo é na máquina do
  Samir**: subir `CLIENT_DESKTOP_MIN_VERSION=0.99.0` no servidor deve fazer a tarja
  aparecer; dispensar e recarregar não deve trazê-la de volta; bumpar a versão do
  servidor deve trazer.

## ADR-019 — Servidor configurável, probe no Rust e o CSP que destravou

- **Data:** 27/07/2026 · **Status:** Aceito
- **Contexto:** o endereço do ShvIA era a constante `SHVIA_URL` em `src/main.ts`.
  Item **D4** do [comparativo 9router × hermes](../../SHVIA-WEB/docs/comparativos/9router-hermes.md),
  e o item (a) que faltava da F2. Sem ele não há **on-prem** (cliente com o próprio
  ShvIA) nem apontar a casca para um servidor local em desenvolvimento.
- **O bloqueio não era de UI, era de CSP.** O `csp` do `tauri.conf.json` é
  **estático**, e o `connect-src` listava os FQDNs porque a casca fazia
  `fetch(.../api/v1/health)`. Um `connect-src` estático **não pode** listar uma URL
  que o usuário acabou de digitar. Enquanto o probe fosse JavaScript, servidor
  configurável era impossível — não por decisão, por CSP.
- **Decisão:** o probe **sai do JavaScript** e vira `TcpStream::connect_timeout` no
  Rust (`server::probe`, exposto como `shvia_server_probe`). Com isso o
  `connect-src` perdeu os três FQDNs e a casca lê o endereço de
  `shvia_server_config`, que resolve o `server.json` do diretório de config do app e
  cai no embutido quando não há nada.
- **Ganho de lado:** o timeout caiu de **15 s para 4 s**. Os 15 s do
  [ADR-012](#adr-012--timeout-do-ping-de-alcance-o-cold-start-do-webkitgtk-custa-5-6-s) existiam só para absorver o cold-start do WebKit — 5-6 s antes
  de qualquer resposta no primeiro request da engine "fria". O `connect` do Rust
  responde em dezenas de milissegundos, então o número que existia para esconder o
  problema saiu junto com o problema.
- **O host configurado passa a ser INTERNO** — a decisão mais pesada aqui. Ele
  recebe as **pontes nativas**: Modo Code (spawn de processo local, leitura de FS),
  notificação, badge, gate de versão, token de capacidade. Não existe meio termo
  útil: casca que abre o servidor do cliente **sem** as pontes entrega um navegador,
  não o ShvIA Desktop. O que torna isso aceitável, e o que precisa continuar
  valendo:
  - a URL só entra por uma tela **nativa da casca local**, digitada por quem está no
    teclado. **Página remota não alcança os comandos:** a capability `default` não
    declara `remote`, então o ACL do Tauri recusa `invoke` de origem remota — sem
    isso, um servidor comprometido se auto-configuraria como destino permanente do
    app. É o [ADR-001](#adr-001--base--shvterm-tauri-2-fork-claude-descartado) continuando a valer, agora com um `invoke_handler`
    no meio;
  - **`https` obrigatório** fora de loopback. `http` em rede entregaria a sessão em
    claro — e aqui rebaixa o perímetro inteiro, não só o transporte;
  - credencial embutida (`https://user:senha@host`) é **recusada**, não descartada
    em silêncio: descarte calado deixaria a pessoa achando que está autenticada;
  - a tela **diz em português** que o servidor recebe acesso nativo, antes de salvar.
- **Alcance não é identidade, e isso é escopo declarado.** O probe abre um TCP e
  fecha: responde "tem alguém escutando", exatamente como o `no-cors` respondia
  (resposta opaca não deixava ler nada). Não valida certificado nem confere que é um
  ShvIA. Falar HTTPS do Rust exigiria `reqwest` + rustls, e o binário ainda não paga
  esse custo — o **D1** (auto-update) trará um cliente HTTP de verdade e a validação
  de identidade entra com ele. Até lá o feedback é a própria página: URL errada abre
  um site errado, e isso é visível na hora.
- **Sem assistente de primeiro uso.** A linha "Servidor: host — trocar" fica visível
  no splash e na tela offline, e é o ponto de entrada. Um wizard bloqueante puniria
  os 99% que usam o padrão para servir os 1% de on-prem; a linha também responde
  "para onde este app vai entrar?", que é informação útil sempre.
- **Fail-open na leitura:** `server.json` ausente, com JSON corrompido ou com URL que
  não passa na validação de **hoje** cai no embutido. Um arquivo editado à mão com
  lixo dentro não pode virar tela branca. A revalidação na leitura também significa
  que endurecer a regra desqualifica o que uma versão antiga gravou.
- **Consequências / validação:** `cargo test` 21/21 (9 de `server::normalize`/`probe`
  e o que trava o perímetro: host configurado entra, a vizinhança dele não, e `http`
  continua fora mesmo sendo o host configurado), `cargo clippy -D warnings` e `tsc`
  limpos. Entrou `@tauri-apps/api` no `package.json` — a casca não tinha como falar
  com o Rust (`withGlobalTauri: false` e nenhum comando existia até aqui).
  **Teste ao vivo na máquina do Samir:** `?server` abre o formulário; salvar um host
  inalcançável deve cair em offline com auto-retry; "Voltar ao padrão" deve reaparecer
  só quando não é o embutido; e o Modo Code deve continuar funcionando no host padrão.

## ADR-020 — Checksums e `release.json` saem do build LOCAL; assinatura no Windows

- **Data:** 27/07/2026 · **Status:** Aceito
- **Contexto:** item **D9** do
  [comparativo 9router × hermes](../../SHVIA-WEB/docs/comparativos/9router-hermes.md),
  e **insumo do D1** (auto-update): sem manifesto, o updater não tem o que ler. O
  estado até aqui: o macOS assinava e notarizava no `build-local.sh`, mas havia
  **zero checksum em qualquer plataforma** e o `build-local.ps1` não tinha **uma
  linha** de assinatura.
- **A restrição que define tudo:** a **CI foi removida na 0.4.6** por custo, e o
  build é 100% local por decisão. Então cada SO é empacotado numa **máquina
  diferente**, e nenhuma vê os artefatos das outras. O hermes gera o manifesto num
  `scripts/release.py` de CI; aqui isso não existe para copiar.
- **Decisão:** `scripts/release-manifest.mjs` (Node puro, uma implementação para os
  três SOs — mesmo padrão do `git-sync.mjs` e do `sync-version.mjs`), chamado no fim
  dos dois scripts de build. Ele produz:
  - um **`.sha256` ao lado de cada instalador**, no formato que `sha256sum -c` e
    `shasum -a 256 -c` leem direto. Quem baixa não precisa saber que existe um
    manifesto para verificar um arquivo;
  - o **`release.json`** na raiz: versão, data e, por plataforma, nome/tamanho/hash
    de cada artefato.
- **O manifesto MESCLA, não sobrescreve.** É a consequência direta de um build por
  máquina: sobrescrever faria o build do Windows **apagar a entrada do macOS**, e o
  D1 leria um manifesto que promete uma plataforma só. Cada build atualiza apenas a
  sua e preserva as outras, e o script **diz quais faltam** — sem esse aviso,
  publicar uma release com uma plataforma só é erro silencioso.
- **Versão diferente descarta o manifesto inteiro.** Misturar artefatos de versões
  no mesmo `release.json` é pior que recomeçar: o updater baixaria 0.15.0 no macOS e
  0.14.0 no Windows achando que são a mesma release. Pelo mesmo motivo o script
  **ignora artefato que não é da versão atual** — pego rodando, um
  `ShvIA_0.8.2.dmg` esquecido no bundle dir entrou no manifesto da 0.15.0.
- **O hash vem DEPOIS de assinar.** Assinatura e `stapler` **alteram os bytes**; um
  sha256 calculado antes descreveria um arquivo que não existe mais, e o updater
  recusaria o download por hash divergente. Nos dois scripts o manifesto é o último
  passo.
- **Assinatura no Windows:** `signtool` sobre o `.msi` **e** o `-setup.exe` — são
  dois instaladores distintos, e assinar só um deixa metade dos usuários vendo
  "Editor desconhecido" no SmartScreen (o equivalente Windows do "app danificado"
  que o macOS mostra sem Developer ID; ele esconde o botão de instalar atrás de
  "Mais informações", e a maioria desiste ali).
  - `/tr` (timestamp RFC3161) **não é opcional**: sem ele a assinatura expira junto
    com o certificado, e um instalador de hoje deixa de ser confiável no dia em que
    o cert vencer.
  - `signtool verify /pa` roda sempre depois: assinar sem conferir deixa passar cert
    expirado ou cadeia incompleta, que o usuário descobre no SmartScreen.
  - **Credencial nunca no repo**, e a preferência é
    `SHVIA_WIN_CERT_THUMBPRINT` (cert no repositório do Windows, chave privada não
    vira arquivo) sobre `SHVIA_WIN_PFX` + senha em env de sessão.
  - **Sem certificado o build segue e AVISA em amarelo.** Mesma postura do macOS —
    mas com aviso explícito, porque build sem assinatura que parece normal é o que
    faz alguém publicar e descobrir pelo relato do usuário.
- **`release.json` é gitignorado.** É artefato de build, não fonte: cada máquina
  regenera para a sua plataforma e o conteúdo depende de binários não versionados.
  Versionar traria conflito em todo build e um hash no git que não corresponde a nada.
- **Consequências / validação:** `bash -n build-local.sh` e `node --check` passam, e
  o manifesto foi exercitado com artefato de mentira (mesclagem, hash, filtro de
  versão e o aviso de plataforma faltando). **NÃO validado:** o `build-local.ps1`
  (não há PowerShell nesta máquina) e a assinatura de verdade (não há certificado).
  **O teste real é na máquina Windows do Samir**, e ele deve rodar `-NoSign` uma vez
  antes para ver o aviso amarelo.
- **De passagem:** os dois scripts diziam "replicando o que
  `.github/workflows/build.yml` faz nos runners". Esse arquivo **não existe desde a
  0.4.6** — a referência morta saiu.
- **Adendo 0.17.0 (item D1):** o `release-manifest.mjs` passou a carregar também a
  **assinatura minisign** — o `<artefato>.sig` que o Tauri grava quando
  `TAURI_SIGNING_PRIVATE_KEY` está no ambiente. É o campo que o
  `tauri-plugin-updater` exige; sem ele o plugin **recusa** o update, e o endpoint do
  ShvIA (v2.74.0) trata o artefato como inexistente — melhor que o app baixar 80 MB
  para depois rejeitar. O script **avisa quando NADA foi assinado**, porque o sintoma
  sem esse aviso seria "o auto-update nunca oferece nada": silencioso e difícil de
  rastrear até aqui.

  **O plugin ainda NÃO está ligado, e é decisão.** Ele exige o par minisign, que só o
  operador gera:

  ```bash
  npx tauri signer generate -w ~/.shvia/updater.key   # UMA vez, FORA do repo
  ```

  A **pública** vai em `tauri.conf.json` (`plugins.updater.pubkey`) e **é
  versionada** — chave pública é para ser pública. A **privada** fica fora da árvore e
  entra no ambiente do build.

  Ligar o plugin com uma pubkey placeholder seria **pior que não ligar**: ou falha em
  silêncio e ninguém descobre até precisar de um update urgente, ou alguém esquece de
  trocar e a verificação de assinatura vira teatro. O lado do servidor já está pronto
  e documentado em
  `SHVIA-WEB/docs/INFRA/AUTO-UPDATE-DESKTOP.md`.

## ADR-021 — O `anna` viaja no instalador (`externalBin`), e o empacotado vence o do PATH

- **Data:** 27/07/2026 · **Status:** Aceito
- **Contexto:** item **D5**. O Modo Code é a feature mais cara de construir do
  produto, e o `anna` era **pré-requisito externo**: quem instalava o ShvIA Desktop
  **não tinha Modo Code** até rodar um `install.sh` de outro repo (ou pôr um `.exe`
  no PATH, no Windows). É o gargalo de adoção — a funcionalidade existe e a maioria
  nunca chega nela.
- **Decisão:** o `anna` vai no bundle como **`externalBin`** (sidecar do Tauri),
  preparado por `scripts/stage-anna.mjs` antes do `tauri build`.
- **`externalBin` e não `resources`, por dois motivos concretos:**
  1. o `externalBin` põe o binário **ao lado do executável do app**
     (`Contents/MacOS/` no macOS), que é exatamente o **primeiro** lugar onde o
     `resolve_bin()` já procurava. Com `resources` ele iria para
     `Contents/Resources/` e o lookup existente não o acharia;
  2. ele entra na **assinatura do bundle**. Isso não é conforto: um executável **não
     assinado** dentro de um `.app` assinado **reprova na notarização** da Apple — e
     o sintoma seria o **app inteiro** sendo recusado, não o `anna`.

  O preço é o nome com **target triple** (`anna-aarch64-apple-darwin`), que é
  justamente o que o script resolve.
- **O empacotado VENCE o do PATH.** Ele é o que foi testado com **esta** versão do
  app. Um `anna` velho esquecido no PATH — o caso comum de quem instalou à mão meses
  atrás — passaria a decidir o comportamento do Modo Code, e o sintoma seria
  "funciona na sua máquina" sem ninguém suspeitar do PATH. A ordem do `resolve_bin()`
  já era essa; o que mudou é que agora **existe** um empacotado, então a ordem passou
  a ter consequência e virou decisão documentada.
- **Ausência não derruba o build.** Sem `anna` no PATH (ou com `--no-anna`), o app
  sai sem o motor e o `resolve_bin()` continua procurando no PATH em runtime — o
  comportamento de antes deste item. Derrubar o build aqui transformaria "não
  consegui melhorar a adoção" em "não consegui empacotar o app".
- **Mas binário que não responde `--version` é ERRO.** Provavelmente é de outra
  arquitetura ou está corrompido; empacotar assim entregaria um Modo Code quebrado
  **por dentro de um app que parece completo** — pior que não empacotar.
- **A versão empacotada SEMPRE aparece no log do build.** Empacotar "o que estiver
  instalado" é como uma versão velha vai parar dentro de um release; o número tem de
  estar no log para alguém poder conferir depois.
- **Handshake de prontidão** (`engineStatus` na ponte): devolve `found`, `bundled`,
  `version` e `path`. `found` e `version` são campos **separados** de propósito —
  `found: true` com `version: null` é "está lá e não roda", diagnóstico diferente de
  "não está lá", e sem essa distinção o usuário fica reinstalando o que já está
  instalado. É também o insumo do **D7** (doctor).
- **`src-tauri/binaries/` é gitignorado:** artefato de 8 MB, e a versão certa depende
  de qual `anna` a máquina de build tem.
- **Consequências / validação:** `cargo clippy -D warnings`, `cargo test` 21/21,
  `bash -n`, `node --check`, e o `stage-anna.mjs` exercitado de verdade (empacotou o
  `anna 0.8.5` desta máquina). **NÃO validado:** o bundle final com o sidecar dentro
  (exige `npx tauri build`, que é pesado) e o `build-local.ps1`. **O teste real é
  instalar o `.dmg` numa máquina SEM `anna` e abrir o Modo Code.**

## ADR-022 — Auto-update ligado: o par minisign existe, e o updater é dirigido pelo Rust

- **Data:** 2026-07-28 · **Status:** aceito · **Versão:** 1.0.0 · **Item:** D1
- **Contexto:** o servidor já servia o manifesto desde a v2.74.0 do ShvIA e o
  `release-manifest.mjs` já capturava o `.sig` desde a 0.17.0 (ADR-020). Faltava **uma
  ação de operador**: gerar o par minisign. O Samir gerou em 28/07/2026 na máquina que
  também tem o material da Apple, e a pública entrou no `tauri.conf.json`.

### O que foi decidido

- **A chave privada vive em UMA máquina** (a de release, que é a que tem o certificado
  da Apple) **+ o cofre**. Não é espalhada pelas máquinas de desenvolvimento. O
  `.pub` é versionado — chave pública é para ser pública.
- **Nenhuma capability para a página remota.** A capability `default` **não** declara
  `updater:default`. O plugin é dirigido só pela API Rust, como `dialog` e
  `notification` já eram. Updater é o pior candidato possível a exceção do ADR-001:
  um servidor comprometido que alcança `install` **escolhe qual binário roda na
  máquina do usuário** — deixa de ser XSS e passa a ser execução de código nativo.
- **O endpoint é remontado em runtime a partir do servidor CONFIGURADO.** O
  `plugins.updater.endpoints` do `tauri.conf.json` é estático, e desde o D4 (ADR-019)
  o servidor é configurável. Endpoint fixo faria uma instalação **on-prem** consultar
  o `ai.shvia.org` e instalar o build de outra infraestrutura — o mesmo tipo de furo
  de perímetro que o `is_server_host` fecha na navegação. O valor do
  `tauri.conf.json` vale só como default.
- **`createUpdaterArtifacts: true` no bundle.** Sem isso o Tauri **não gera** o
  `.app.tar.gz` / `.AppImage.tar.gz` nem os `.sig` — e o sintoma seria o
  `release-manifest.mjs` avisando "nada assinado" para sempre, com a chave já no
  lugar. É a peça que faz a assinatura existir, não só ser lida.
- **AVISA e pergunta; não instala sozinho.** Uma pergunta só, com o reinício dito na
  própria pergunta. Update silencioso que reinicia o app no meio de uma conversa com
  a Anna é pior que o problema que resolve. E **uma** pergunta e não duas
  ("baixar?" → "reiniciar?") porque no Windows o instalador toma a mão do processo:
  a segunda pergunta seria uma promessa que não se pode cumprir.
- **"Depois" é gravado por VERSÃO** (`updater.json`), não como booleano. Booleano
  faria um "Depois" clicado uma vez desligar o updater para sempre — a falha
  silenciosa que este item existe para não ter. Mesmo raciocínio do "dispensar por
  versão de servidor" do gate de compatibilidade (ADR-018).
- **Guarda de reentrância** (`EM_ANDAMENTO`): o timer de 6 h e o clique no menu são
  independentes e podem coincidir. Sem a guarda, dois downloads de ~80 MB e duas
  chamadas concorrentes de `install` sobre o mesmo bundle.
- **Checagem automática é silenciosa; a manual sempre responde.** Quem não pediu para
  checar não recebe diálogo de erro por falha de rede. Mas menu que não dá sinal
  nenhum parece quebrado, então a manual responde até "você já está na mais recente".
- **Thread própria, não `async_runtime::spawn`.** O ciclo bloqueia em diálogo nativo
  por tempo indeterminado (esperando o usuário); num worker do runtime compartilhado
  isso prenderia um slot do resto do app.

### Consequências

- **Esta é a última instalação manual.** Quem está na 0.18.0 ou anterior **não tem
  updater** e precisa instalar a 1.0.0 à mão; da 1.0.0 em diante o app se mantém.
  É o que faz os itens seguintes da trilha desktop (D2, D3, D7) chegarem aos usuários
  sem reinstalação em cada máquina — e é o motivo do bump para **1.0.0**.
- **Perder a chave privada é irreversível na prática:** a pubkey fica compilada no
  binário, então todo install existente recusa update assinado por chave nova. O
  caminho de recuperação é reinstalação manual em cada máquina.
- **Validação:** `cargo clippy` limpo, `cargo test` 22/22 (+1: o formato do path do
  endpoint, que se errado devolveria 404 em vez do manifesto).
- **✅ Ciclo de ponta a ponta VALIDADO em 28/07/2026 (1.0.0 → 1.0.1).** A 1.0.1 foi
  publicada em `ai.shvia.org` e a 1.0.0 instalada neste Mac achou, baixou, instalou e
  reiniciou pelo menu **Ajuda → Verificar atualizações…**; depois do reinício o modal
  Sobre mostra `v1.0.1` e a mesma checagem responde "você já está na versão mais
  recente". A **1.0.1 não tem mudança de código** — é um bump puro, feito só para
  exercitar o updater, e está registrada como tal.
- **🐛 O que o teste real pegou, e nenhum teste automatizado pegaria:** o
  `.app.tar.gz` **não subiu** para o servidor na primeira tentativa. O artefato do
  updater fica em `bundle/macos/` e o `.dmg` em `bundle/dmg/` — um upload de um
  diretório só perde exatamente o único arquivo que o updater baixa. E o sintoma
  engana: o endpoint devolve **200 com `signature` e `url` preenchidos**, porque o
  manifesto está correto; a falha só aparece quando o app tenta baixar. Por isso o
  passo de publicação virou checklist com verificação de `sha256` **pela URL
  pública** em `SHVIA-WEB/docs/INFRA/AUTO-UPDATE-DESKTOP.md` §4 — vai acontecer de
  novo na máquina Windows.

## ADR-023 — Publicar é passo do build (`--publish`), e o manifesto mescla pelo servidor

- **Data:** 2026-07-28 · **Status:** aceito · **Versão:** 1.0.2 · **Item:** D1
- **Contexto:** a 1.0.1 validou o auto-update, mas o passo de publicação era `scp` à
  mão — e na primeira tentativa o `.app.tar.gz` **ficou de fora**. O endpoint
  continuou devolvendo `200` com `signature` e `url` preenchidos (o manifesto estava
  correto), então nada acusou o problema até o app tentar baixar.

### O que foi decidido

- **A lista de arquivos sai do `release.json`, não de um glob de diretório.** É a
  correção direta da causa: no macOS o artefato do updater está em `bundle/macos/` e o
  instalador humano em `bundle/dmg/`; no Linux, `.AppImage.tar.gz` e `.AppImage` no
  mesmo diretório mas com papéis diferentes. Qualquer lista escrita à mão erra em
  algum SO. Derivando do manifesto, **o que sobe é por construção o que o manifesto
  declara** — e artefato declarado que não existe no disco **aborta** a publicação,
  porque publicar manifesto apontando para arquivo ausente é exatamente o 404 que só
  aparece no download.
- **O manifesto publicado é baixado ANTES de gerar o novo.** O merge de plataformas
  passa a acontecer sozinho, contra o que está no ar. Isso **substitui** o passo
  manual de "copiar o `release.json` de uma máquina para a próxima antes de buildar
  lá", que era frágil pela pior razão possível: esquecê-lo não quebra nada visível —
  publicar do macOS apagaria a entrada do Windows e só os usuários de Windows
  parariam de receber update, sem sintoma no build nem no endpoint.
- **Verificação pela URL pública, não pelo diretório do servidor.** Baixa o artefato
  assinado e compara o `sha256` com o do manifesto. Conferir no diretório provaria
  só que o arquivo existe; pela URL prova que o Apache o serve **e** que chegou
  inteiro. Upload truncado dá `200` com bytes errados, e o sintoma seria falha de
  assinatura no cliente — mensagem que não aponta para o upload.
- **A senha do scp nunca entra em variável nem em arquivo.** Um `scp` só com todos os
  arquivos (uma conexão, um prompt), e `ssh-copy-id` documentado para quem quiser
  eliminar o prompt. Guardar senha de root em env de build seria trocar um
  inconveniente de 5 segundos por um segredo em texto claro na máquina de release —
  a mesma que guarda a chave do updater.
- **Destino e base pública são constantes documentadas** (`SHVIA_PUBLISH_DEST`,
  `SHVIA_PUBLIC_BASE`), sobrescrevíveis por flag. Não são segredo: o destino é host
  da tailnet e a base é a URL que o app já usa. On-prem sobrescreve as duas.

### ⚠️ CORRIGIDO na 1.1.1 — este diagnóstico estava ERRADO (ver ADR-025)

> O texto abaixo ficou aqui porque a conclusão errada é a parte instrutiva: o
> **sintoma** (Linux em 204) era real, a **causa** que eu apontei não era.
>
> Eu escrevi que o `EXTENSOES.linux` do `release-manifest.mjs` precisava de
> `.AppImage.tar.gz` e que sem ele o artefato de updater do Linux não entrava no
> manifesto. **O `.AppImage.tar.gz` não existe.** Ele é o formato LEGADO
> (`createUpdaterArtifacts: "v1Compatible"`); com `true`, o Tauri 2 assina os bundles
> **direto**, e um build real de Linux (1.1.0, 28/07) produziu `.deb.sig`, `.rpm.sig`
> e `.AppImage.sig` — nenhum tarball. O manifesto do Linux sempre esteve **correto**,
> com os três artefatos assinados.
>
> A causa real era do **lado do servidor**: o `artefatoPara` do ShvIA procurava
> `.AppImage.tar.gz`. Corrigido na v2.86.6 do SHVIA-WEB. A entrada
> `.AppImage.tar.gz` que adicionei ao `EXTENSOES.linux` ficou — é inofensiva e serve
> se algum dia o bundle voltar a `v1Compatible` — mas **não** era o que faltava.
>
> **De onde veio o erro:** li o docblock do `install_appimage` do plugin, que desenha
> a estrutura `[App].AppImage.tar.gz → [App].AppImage`, e tratei o desenho legado
> como requisito. O código logo abaixo desmente: `if infer::archive::is_gz(bytes)`
> extrai, **senão grava os bytes direto** — o plugin aceita as duas formas. Doc de
> dependência descreve o que já existiu, não o que o build gera hoje; o que decide é
> a saída do build, e ela estava disponível.

### Consequências

- **Validação:** `bash -n`, `node --check`, `--help` conferido, e os três trechos
  `node -e` do publish exercitados contra o `release.json` real da 1.0.1 — a lista
  montada acha os dois artefatos nos **dois diretórios diferentes** (que é o bug que
  o item existe para impedir), e a verificação de `sha256` pela URL pública passa
  contra o que está publicado agora.
- **NÃO validado:** o `scp` em si (exigiria republicar) e o caminho do Linux (não há
  build de Linux nesta máquina). **O `build-local.ps1` continua sem `--publish`** —
  publicação no Windows segue manual, e está avisado em `docs/build.md`.

## ADR-024 — Reusar build da mesma versão, e exigir a chave do updater ANTES de compilar

- **Data:** 2026-07-28 · **Status:** aceito · **Versão:** 1.0.3 · **Item:** D1
- **Contexto:** dois atritos reais no mesmo dia. (a) Esquecer o `--publish` custava
  um rebuild inteiro só para subir arquivo que já existia no disco. (b) Na máquina
  **Linux**, o build compilou por **2m01s**, gerou `.deb`/`.rpm`/`.AppImage` e então
  abortou com `A public key has been found, but no private key` — porque
  `createUpdaterArtifacts: true` (ADR-022) faz o Tauri **exigir**
  `TAURI_SIGNING_PRIVATE_KEY`, e ele só verifica isso no fim do empacotamento.

### Reuso de build — e por que "o arquivo existe" não serve como teste

O macOS mostra o problema: o artefato do updater é `ShvIA.app.tar.gz`, **sem versão
no nome**. Presença não distingue o build de agora de sobra de um build anterior. A
identidade vem do `sha256` que o `release.json` gravou, então o teste é:

1. `release.json` existe e é da versão de `version.md`;
2. todo artefato declarado para esta plataforma existe no disco;
3. o `sha256` de cada um **ainda confere** — é isto que dá identidade ao arquivo sem
   versão no nome;
4. **nenhuma fonte é mais nova que o artefato mais antigo.**

**O passo 4 é o que torna o atalho seguro, e sem ele o atalho seria uma armadilha:**
editar código sem bumpar a versão passa em 1–3 — o `release.json` antigo continua
descrevendo os binários antigos *corretamente* — e publicaríamos **binário velho,
assinado, como se fosse a versão nova**. Falha silenciosa e com selo de autenticidade,
que é pior que erro. Fontes observadas: `src/`, `src-tauri/src/`,
`src-tauri/capabilities/`, `src-tauri/binaries/` (o `anna` do D5 não aparece em
nenhuma outra), `index.html`, os manifests e `tauri.conf.json`.

`scripts/` **não** entra de propósito: mudar o gerador de manifesto não invalida os
binários, e o manifesto é regenerado em toda execução de qualquer forma.

Incluir os manifests só é seguro porque o `sync-version.mjs` **escreve apenas quando
o conteúdo muda** — se reescrevesse sempre, o passo 4 daria falso positivo eterno e o
reuso nunca aconteceria.

### A chave do updater é verificada no primeiro segundo

`check_updater_key` roda depois do `preflight`, antes de qualquer compilação. Além de
falhar em milissegundos em vez de minutos, a mensagem diz **onde a chave mora** — a do
Tauri diz apenas que ela falta.

- **`--no-sign` passou a significar o mesmo nos três SOs:** "build de teste, não
  publicável". Sem chave e com `--no-sign`, o build sai **sem artefato de updater**
  (via `--config '{"bundle":{"createUpdaterArtifacts":false}}'`) em vez de abortar.
  Antes o flag só afetava o `codesign` do macOS.
- **A chave é UMA para as três máquinas.** O par é único (ADR-022): a mesma chave que
  assina o release do macOS assina o do Linux e do Windows. A máquina Linux precisa
  dela no ambiente — não é uma chave por SO.

### Nota de bash: nada de array de argumentos

A chamada do `tauri build` tem quatro braços explícitos em vez de montar um array. O
bash do macOS é **3.2**, e ali `"${arr[@]}"` de array **vazio** com `set -u` aborta com
`unbound variable` (verificado nesta máquina). Mesmo motivo do `_sha256()`, que escolhe
entre `shasum` e `sha256sum`: sem ele o caminho de Linux morreria justamente na
verificação que o `--publish` existe para fazer.

### Consequências

- **Validação:** `bash -n` nos dois bashes (3.2 e 5.x), e o reuso exercitado **de
  verdade** — execução completa em **2s** em vez de recompilar, com o manifesto
  regenerado. As cinco decisões testadas em isolamento: reusa no estado bom; **não**
  reusa com `--force`, com fonte mais nova (arquivo novo em `src/`), com
  `version.md` divergente do manifesto, e com artefato corrompido de mesmo nome
  (sha256 divergente). O artefato foi restaurado e reconferido.
- **NÃO validado:** o caminho de Linux de ponta a ponta (não há build de Linux nesta
  máquina) e o `--config` que desliga o artefato de updater no `--no-sign`.
- **Efeito colateral aceito:** bumpar a versão invalida o reuso (passo 1), então o
  primeiro build de cada versão nova compila inteiro — que é o correto.

---

## ADR-024 — Bandeja/menubar, e fechar a janela deixa de encerrar o app

- **Data:** 28/07/2026 · **Status:** Aceito · **Item:** **D2** do
  [comparativo 9router × hermes](../../SHVIA-WEB/docs/comparativos/9router-hermes.md)
  · **Fecha o buraco de:** [ADR-011](#adr-011--notificações-nativas-dos-alertas-de-preço-ponte-via-canal-do-modo-code)

### Contexto

O ADR-011 entregou notificação nativa dos alertas de preço e resolveu, nas palavras
dele, "o caso do meio": **janela aberta, mas em segundo plano**. O caso de baixo ficou
aberto — **janela fechada**. Sem janela o app saía; sem app, nada notifica. O alerta só
chegava por **Telegram**, que era justamente a dependência que o ADR-011 quis dispensar.

Um alerta que só existe com a janela na frente do usuário não é alerta, é badge.

E havia um agravante que ninguém tinha escrito: **o comportamento divergia por SO**. No
macOS fechar a janela nunca encerrou o app (o ícone segue no Dock, e o `RunEvent::Reopen`
recria a janela). No Windows e no Linux, fechar mata o processo. Então o mesmo produto
entregava alerta com a janela fechada num sistema e não nos outros — **sem erro, sem
tela, sem log**. Só a notificação que não chega.

### Decisão

Três peças, e nenhuma resolve sozinha:

1. **Ícone de bandeja/menubar** (`tray.rs`, feature `tray-icon` do Tauri). Um processo
   que roda sem janela **e** sem ícone é um processo que o usuário mata no gerenciador
   de tarefas achando que é vírus.
2. **Fechar a ÚLTIMA janela recolhe** em vez de encerrar (`CloseRequested` →
   `prevent_close()` + `hide()`). É o que mantém o processo vivo para o alerta chegar.
3. **"Iniciar com o sistema"** (`tauri-plugin-autostart`): LaunchAgent no macOS, chave
   `Run` no Windows, `.desktop` em `~/.config/autostart` no Linux. É o que faz o app
   estar de pé depois do boot, quando ninguém lembrou de abri-lo.

#### `hide()`, não destruir

Recolher tem de devolver a janela **como ela estava**. Destruir e recriar no clique da
bandeja recarregaria a página remota: o usuário perderia a rolagem da conversa e pagaria
um page load para "voltar" de algo que nunca deveria ter saído.

#### Só a ÚLTIMA janela

O app é multi-janela (`Cmd+N`). Com duas abertas, `Cmd+W` fecha aquela ali, como em
qualquer app — recolher a primeira de duas seria um bug com cara de feature. A checagem
é `webview_windows().len() <= 1`, com `<=` e não `==` porque a contagem durante o
fechamento depende do SO, e **errar para o lado de recolher é melhor que errar para o
lado de sair devendo um alerta**.

#### Default LIGADO, com três saídas

`close_to_tray = true` por default não é agressividade: sem ele o item não entrega nada,
e é o comportamento que o macOS já tinha — o default faz os outros dois SOs pararem de
divergir. O incômodo conhecido ("app que não morre") é respondido de três jeitos, porque
um só não bastaria:

- **`Sair do ShvIA`** na bandeja e `Sair` no menu `Arquivo` — a saída de verdade
  continua a um clique. Os dois usam o `PredefinedMenuItem::quit`, que encerra pela via
  oficial do Tauri, não pelo caminho que interceptamos.
- **Aviso nativo na primeira vez**, uma única vez na vida da instalação. Sem ele a
  janela desaparece e a conclusão natural é "o app travou". Repetido, viraria ruído que
  o usuário aprende a ignorar — e ele precisa ser lido exatamente na primeira vez.
- **A preferência é desligável na própria bandeja**, onde a confusão acontece.

#### O estado é LIDO, não lembrado

O menu é reconstruído a cada mudança, consultando o estado real: `is_enabled()` do
autostart e o host configurado do [ADR-019](#adr-019). Guardar cópias dos itens e
atualizá-las deixaria a bandeja **mentir** no dia em que algo mudasse por fora —
alguém apagando o LaunchAgent na mão, por exemplo. Bandeja que mostra estado errado é
pior que bandeja sem estado: a primeira faz o usuário confiar.

O menu mostra **`Servidor: <host>`** desabilitado no topo. Numa instalação on-prem,
"para qual servidor este app está apontando?" é a primeira pergunta de qualquer suporte,
e até aqui só o modal "Sobre" respondia.

#### Sem capability para a página remota

Igual ao updater ([ADR-022](#adr-022)): o plugin de autostart é dirigido **só pelo
Rust**. Autostart alcançável pelo servidor deixaria um servidor comprometido decidir que
o app sobe com a máquina — o [ADR-001](#adr-001) segue valendo.

#### `tray.json`, separado do `server.json`

Preferência da bandeja num arquivo próprio. O `server.json` é o **perímetro** (o host
que a navegação considera interno), e JSON corrompido lá é tela branca. Misturar
preferência cosmética no mesmo arquivo aumentaria as chances de mexer no que não pode
quebrar. Leitura nunca falha: ausente, ilegível ou com lixo cai no default.

### Consequências

- **A partir da 1.1.0 o app é residente.** Quem fechava a janela para "sair" agora
  precisa usar `Sair` — e é avisado na primeira vez.
- **Validado nesta máquina:** `cargo check`, `cargo clippy --all-targets` limpo,
  `cargo test` **27/27** (+5 novos), `npm run build`. Os testes novos cobrem a parte que
  falha em silêncio: um `unwrap_or(false)` no lugar do default desligaria o item inteiro
  sem erro nenhum, e o alerta de preço voltaria a não chegar.
- **NÃO validado:** o comportamento de bandeja em Windows e Linux (não há build desses
  SOs aqui) e o autostart de verdade nos três — exige instalar e reiniciar a máquina.
  O que dá para afirmar é que o plugin é o oficial e a leitura de estado é do próprio
  plugin, não uma cópia nossa.
- **Dívida de asset, não de código:** no macOS a barra de menus espera um ícone
  **template** (monocromático, que inverte com o tema). Usamos o ícone colorido do app:
  funciona e fica fora do padrão do sistema. Trocar é substituir o png.

## ADR-025 — No Linux o artefato de updater é o bundle INSTALADO, e o cliente diz qual

- **Data:** 2026-07-28 · **Status:** aceito · **Versão:** 1.1.1 · **Item:** D1
- **Contexto:** com a 1.1.0 publicada (macOS + Linux, os cinco artefatos assinados no
  manifesto), o macOS atualizava e o **Linux dizia "você já está na versão mais
  recente (1.0.3)"**. `curl` confirmou: `darwin-aarch64` → 200, `linux-x86_64` → 204.

### A causa, e por que o diagnóstico anterior estava errado

O `artefatoPara` do ShvIA (v2.74.0) procurava `.AppImage.tar.gz` no Linux. **Esse
arquivo não existe.** O tarball é o formato **legado**
(`createUpdaterArtifacts: "v1Compatible"`); com `true`, o Tauri 2 assina os bundles
direto. O build real da 1.1.0 no Linux imprimiu:

```
Finished 3 updater signatures at:
  ShvIA_1.1.0_amd64.deb.sig
  ShvIA-1.1.0-1.x86_64.rpm.sig
  ShvIA_1.1.0_amd64.AppImage.sig
```

Na ADR-023 eu atribuí o Linux em 204 ao `release-manifest.mjs` e "corrigi" o lado
errado. O manifesto sempre esteve certo. **A lição não é sobre AppImage:** eu li o
docblock do `install_appimage` do plugin, que desenha
`[App].AppImage.tar.gz → [App].AppImage`, e tratei um desenho **legado** como
requisito. O código três linhas abaixo desmente — `if infer::archive::is_gz(bytes)`
extrai, senão **grava os bytes direto**. Doc de dependência descreve o que já
existiu; o que decide é a saída do build, e ela estava na tela.

### Não bastava trocar o sufixo

O plugin despacha a instalação pelo bundle do app **em execução**:

```rust
match installer_for_bundle_type(bundle_type()) {
    Some(Installer::Deb) => self.install_deb(bytes),
    Some(Installer::Rpm) => self.install_rpm(bytes),
    _ => self.install_appimage(bytes),
}
```

Quem instalou pelo `.deb` e recebesse o AppImage quebraria — **depois** de baixar
tudo. Então o cliente informa o formato em `?bundle={{bundle_type}}`.

- **Na QUERY, não como segmento novo do path.** As instalações ≤ 1.1.0 já existem e
  continuam pedindo a rota antiga; um path novo as deixaria tomando **404** — erro no
  log, sem nada a fazer — em vez do **204** que elas sabem tratar. Query desconhecida
  o servidor antigo ignora, e o servidor novo trata a ausência como AppImage. O plugin
  substitui o placeholder na query igual ao path (verificado no `updater.rs` dele).
- **Ausente → AppImage**, não 204: é o formato portável e o mais provável de quem
  baixou do site. Quem instalou por `.deb` numa versão antiga atualiza à mão uma vez.
- **Bundle desconhecido também cai no AppImage.** Um cliente novo com bundle que o
  servidor não conhece não pode ficar sem update por causa de um rótulo.

### O teste que duplicava o que verificava

O `endpoint_tem_target_e_arch_no_mesmo_segmento` remontava o formato da URL à mão em
vez de chamar a função. Teste que copia o que verifica **passa verde com o endpoint
errado** — e era o caso: ele estava verde enquanto o Linux não atualizava. Extraí
`endpoint_para(base)` e o teste passou a exercitar a string de verdade.

### Consequências

- **Validação:** `cargo test` 29/29 (+2: bundle na query, base on-prem com porta) e
  `clippy` limpo aqui; `DesktopUpdateManifestTest` 22/22 (+5) no SHVIA-WEB (v2.86.6),
  cobrindo AppImage sem bundle (a regressão), `deb`, `rpm`, bundle desconhecido e
  "sem assinatura continua 204 mesmo com bundle certo".
- **NÃO validado:** o ciclo de update de Linux de ponta a ponta — exige publicar a
  1.1.1 e atualizar a partir da 1.1.0 numa máquina Linux. **O teste real é
  `curl {BASE}/api/v1/desktop/update/linux-x86_64/1.1.0?bundle=deb` devolver 200 com
  a URL do `.deb`**, e depois o menu Ajuda na máquina Linux.
- **Nota de operação:** o servidor precisa estar em ≥ 2.86.6 antes de a 1.1.1 ser
  publicada. Na ordem inversa nada quebra (o cliente novo manda uma query que o
  servidor antigo ignora e segue em 204), mas o Linux continua sem atualizar.
