# ShvIA Desktop — Estado e Continuidade

> **Ler primeiro.** Notas de continuidade. Última atualização: **30/06/2026**.

## Onde estamos

Repo pivotado do fork `claude-desktop-debian` para o **cliente desktop do ShvIA**.
Além da **fundação documental** (30/06), o **esqueleto andante da Fase 1** já está
no repo: um app **Tauri 2** que abre uma janela com a marca ShvIA e navega para o
ShvIA hospedado.

**Feito até agora (30/06/2026):**
- Arquitetura decidida (ver [escopo-projeto.md](escopo-projeto.md) e
  [../docs/decisoes.md](../docs/decisoes.md)).
- **Fork arquivado** em `archive/claude-fork` + tag `archive/claude-fork-v0.2.2`
  (push no `origin` — nada perdido).
- **Documentação de fundação** (README, CLAUDE/AGENTS, `.claude/`, `.continue/`,
  `docs/`, `version.md`).
- **Esqueleto Tauri 2 (Fase 1) — `0.2.0`:**
  - Scaffold Tauri 2 (vanilla-ts) integrado **sem tocar na documentação** existente.
  - `tauri.conf.json`: `productName`/título **ShvIA**, `identifier`
    `cloud.blue3.shvia`, janela única 1280×800 (mín. 800×600),
    `withGlobalTauri: false`, CSP da casca local.
  - Casca de **bootstrap** (`index.html` + `src/main.ts`): splash com a marca que
    **redireciona o WebView para `https://ia.blue3.com.br`** — daí a UI é o Blade
    do ShvIA ("mesmas funções", ADR-002).
  - **Rust mínimo** (`src-tauri/src/lib.rs`): builder Tauri + plugin `opener`;
    **nenhum comando nativo exposto à página remota** (menor privilégio). A demo
    `greet` do scaffold foi removida.
  - **Ícones ShvIA** gerados de `IA/SITE/public/logo.svg` (teal `#1f8a70` + "S"),
    set desktop em `src-tauri/icons/` (mobile descartado — projeto é desktop-only).
  - **`scripts/sync-version.mjs`** + hook `prebuild`: `version.md` vira **fonte
    única** da versão (propaga p/ `package.json`, `tauri.conf.json`, `Cargo.toml`,
    lock files). Modelado no SHVTERM.

## Verificações

- ✅ **Frontend compila**: `npm run build` (sync-version + `tsc` + `vite build`)
  passou e gera `dist/`.
- ✅ **Rust compila** (`0.2.1`): `cargo check` passa (~33s) — valida também o
  `tauri.conf.json` (lido por `generate_context!`). **Precisou pinar `time`:**
  `wry 0.55 → cookie 0.18.1` usa a API antiga de `Parsable::parse` (1 arg), e o
  `time` novo (0.3.52) mudou a assinatura → fixado `time = "=0.3.41"` no
  `Cargo.toml` (com `Cargo.lock` versionado p/ build reproduzível). Remover o pin
  quando wry/cookie subirem.
- ✅ **App roda e renderiza** (`0.2.2`, `npm run tauri dev`): janela abre com
  título **ShvIA**, o WebView **renderiza** e navega para `ia.blue3.com.br`. O
  **login por cookie de sessão funciona** (a janela abriu **já logada** — auth
  same-origin, ADR-005) e o **chat com a ANNA respondeu** com stats de geração
  (`pensou/total/tokens/tok-s`).
- ✅ **STREAMING SSE CONFIRMADO** (`0.2.3`) — **risco #1 (ADR-006) derrubado** no
  Linux/WebKitGTK: num prompt de resposta longa (`2.224 tokens`, ~30 tok/s), o
  **raciocínio e a resposta pintaram token-a-token** (capturas em sequência mostram
  o texto crescer frame a frame; botão **Parar** dinâmico). Quirks de numeração/
  contexto observados são **do modelo** (`ShvIA:G4v5`), não do cliente thin-shell.
- ⚠️ **Quirk WebKitGTK (render por GPU)**: em ambiente **remoto/VM/NVIDIA sem
  acesso a DRM**, o renderer DMABUF/GBM falha (`GBM-DRV error`,
  `DRM_IOCTL_MODE_CREATE_DUMB: Permission denied` → janela em branco). **Fix:**
  rodar com `WEBKIT_DISABLE_DMABUF_RENDERER=1` (render por software) — com ele,
  **zero erros**. É só do ambiente sem GPU; em máquina normal renderiza acelerado.
- ⏳ **F2:** o WebView repassou um pedido de **câmera** (`getUserMedia`) como
  diálogo nativo → definir **política de permissões de mídia** (câmera/mic/notif/
  geo) na F2.

## Decisões travadas

1. **Base = SHVTERM** (Tauri 2 + React, multiplataforma). Fork Claude descartado.
2. **Arquitetura F1 = shell fino Tauri** carregando o ShvIA web (Blade) remoto.
3. **Servidor remoto = fonte da verdade**. Sem banco no cliente.
4. **Repo reaproveitado** (`samirhvbr/SHVIA-DESKTOP`, `master`), histórico mantido.
5. **Auth = cookie de sessão Sanctum same-origin** na F1 (login = tela do ShvIA).

## Próximos passos (Fase 1, continuação)

> Passo-a-passo completo em [../docs/roteiro-fundacao.md](../docs/roteiro-fundacao.md).

1. ✅ **Smoke-test SSE — FEITO** (`0.2.3`): streaming token-a-token confirmado no
   Linux/WebKitGTK. **Risco #1 derrubado.**
2. **F2 — polish nativo (em andamento):**
   - ✅ **multi-janela** (`0.3.0`): `Arquivo → Nova janela` (`Ctrl/Cmd+N`), janelas
     `win-*` que **compartilham a sessão** — conversas/projetos lado a lado.
   - ✅ **branding** (`0.3.1`): seta Blue3 P&B + "AI" azul sobre navy.
   - ✅ **estado da janela persistido** + **links externos no navegador** (`0.4.0`):
     janelas criadas **no Rust** (`build_shvia_window`) com `on_navigation` (origens
     fora de `*.blue3.com.br` abrem no navegador do SO; login é same-origin, não
     quebra) e `tauri-plugin-window-state` (tamanho/posição entre reinícios).
   - ✅ **tela offline v1** (`0.4.1`): **tarja vermelha "Sistema Offline"** injetada
     em cada página (`on_page_load` + `eval`), via `navigator.onLine`/eventos
     `online`/`offline`; some ao reconectar e é clicável p/ recarregar.
   - ✅ **mídia + clipboard (Linux)** (`0.4.3`): liga `enable-media-stream` /
     `enable-mediasource` (habilita `getUserMedia` — mic/câmera) e
     `javascript-can-access-clipboard` (colar/copiar, ex.: Ctrl+V de print) no
     WebKitGTK, e concede o `permission-request` de mídia. (`0.4.2` só tratava o
     signal; faltava ligar as settings, que o WebKitGTK deixa off por padrão.)
     macOS/Win = caminhos próprios (Info.plist / WebView2), ao empacotar lá.
   - ⏳ **Restante (radar):**
     - **tray/About**, **config de URL** no 1º run.
     - **offline v2** — ping no Rust (p/ quedas que o `navigator.onLine` não pega:
       servidor inalcançável com a interface de rede ainda up).
3. Validar **persistência do cookie de sessão** entre reinícios (fechar/reabrir e
   continuar logado).
4. **F4 (paralelo):** colher CI do SHVTERM (matriz mac/win/linux + updater +
   packaging) e **procurement do cert EV Windows** (long pole de prazo).

## Pendências / decisões em aberto (confirmar com o time)

- [ ] **Online-only é aceitável** como propriedade de produto? (toda a arquitetura
      fina depende disso). Ver [escopo](escopo-projeto.md#decisões-em-aberto).
- [ ] **Verba + dono** do cert EV Windows (~US$300–600/ano) e Apple Developer
      (US$99/ano), incl. rotação da chave do updater.
- [ ] **Funções idênticas ao web** ou haverá **telas desktop-only**?
- [ ] **Sidecar Python na F1?** A análise indica que **não** (auth é cookie
      same-origin) — confirmar que entra só na F2.
- [ ] **URL de DEV** do ShvIA (além de produção `ia.blue3.com.br`) para testar.
- [ ] **App ID** `cloud.blue3.shvia` — confirmar (usado como default já).

## Notas

- **CSP × página remota:** o `security.csp` do `tauri.conf.json` governa **só a
  casca local** (splash/offline). Quando o WebView navega para o FQDN, vale o **CSP
  do próprio servidor ShvIA**. Não contar com o CSP do app para "proteger" a página
  remota.
- **Branding (`0.3.1`):** ícone = **marca real da Blue3** (a seta) em **P&B** +
  **"AI"** no azul-claro `#24b0e5`, sobre quadrado **navy** `#0d1b2a`. Fontes em
  `brand/` (`shvia-desktop-icon-1024.png` = ícone; `blue3-logo-source.png` = seta
  original Blue3; `shvia-mark.png` = marca transparente). Splash + favicon na mesma
  paleta (`public/brand-mark.png`, `public/favicon.png`). O "S" teal placeholder
  (do `logo.svg`) foi descartado.
- **A verificar em código (ShvIA) na F1:** (a) middleware de auth de `/chat` em
  `routes/web.php` (ADR-005); (b) trecho de streaming SSE em `public/js/app.js`
  (~linha 4003, `fetch` + `ReadableStream.getReader()`, ADR-006).
