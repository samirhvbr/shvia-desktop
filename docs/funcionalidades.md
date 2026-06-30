# Funcionalidades implementadas — ShvIA Desktop

Registro estável do que o app **já faz**, por versão. (WIP e pendências vivem em
[`.continue/estado-atual.md`](../.continue/estado-atual.md); decisões em
[decisoes.md](decisoes.md); como buildar em [build.md](build.md).)

## Fase 1 — esqueleto andante (validado)

- **Shell fino Tauri 2** (`0.2.0`): janela com título/ícone **ShvIA**,
  `identifier` `cloud.blue3.shvia`. Uma casca local (`index.html` + `src/main.ts`)
  mostra um splash e **redireciona o WebView para `https://ia.blue3.com.br`** — daí
  a UI é o próprio Blade do ShvIA ("mesmas funções", ADR-002). **Menor privilégio:**
  nenhum comando nativo exposto à página remota; `withGlobalTauri: false`.
- **`version.md` como fonte única** (`0.2.0`): `scripts/sync-version.mjs` propaga a
  versão para `package.json`/`tauri.conf.json`/`Cargo.toml`/lock files (hook
  `prebuild`).
- **Builds verdes** (`0.2.1`): `npm run build` (tsc + vite) e `cargo check`. Precisou
  pinar `time = "=0.3.41"` (o `wry 0.55 → cookie 0.18.1` quebra com o `time` novo);
  `Cargo.lock` versionado p/ build reproduzível.
- **App roda, loga e renderiza** (`0.2.2`): a janela abre, o WebView renderiza e o
  **login por cookie de sessão Sanctum** funciona (same-origin, ADR-005) — abriu já
  logada.
- ✅ **Streaming SSE confirmado** (`0.2.3`) — **risco #1 (ADR-006) derrubado** no
  Linux/WebKitGTK: a resposta do chat **pinta token-a-token** (validado num prompt
  longo, ~30 tok/s). Era o maior risco multiplataforma.

## Fase 2 — polish nativo (em andamento)

- **Multi-janela** (`0.3.0`): menu `Arquivo → Nova janela` (`Ctrl/Cmd+N`) abre
  janelas extras (`win-*`) que **compartilham a sessão** (cookie) — conversas/
  projetos lado a lado. `Ctrl/Cmd+W` fecha; `Ctrl/Cmd+Q` sai.
- **Branding** (`0.3.1`): ícone = a **seta da Blue3** em **P&B** + **"AI"** no
  azul-claro `#24b0e5` sobre quadrado **navy** `#0d1b2a`. Splash + favicon na mesma
  paleta. Fontes em `brand/`; assets servidos de `public/`.
- **Estado da janela persistido** + **links externos no navegador** (`0.4.0`): as
  janelas são criadas **no Rust** (`build_shvia_window`); `tauri-plugin-window-state`
  guarda tamanho/posição entre reinícios, e `on_navigation` manda origens fora de
  `*.blue3.com.br` para o **navegador do SO** (login é same-origin, não quebra).
- **Tela offline v1** (`0.4.1`): **tarja vermelha "Sistema Offline"** injetada em
  cada página (`on_page_load` + `eval`, via `navigator.onLine`); some ao reconectar
  e é clicável p/ recarregar.
- **Empacotamento local** (`0.4.5`): `tauri build` gera os instaladores
  (`.deb`/`.AppImage`/`.rpm` validados). Scripts `build-local.{sh,ps1,cmd}` cobrem os
  3 SOs — ver [build.md](build.md). (CI/Actions omitida de propósito, por custo.)

## Limitações conhecidas

- ⚠️ **Mic e Ctrl+V de imagem no Linux** (`0.4.4`, **ADR-008**): o shell habilita
  `getUserMedia` e o clipboard no WebKitGTK (o device é até enumerado), mas a
  **captura do microfone** e o **paste de imagem** **não funcionam** — limitação do
  **WebKitGTK**, não do código. **macOS/Windows** (WKWebView/WebView2) tendem a
  resolver; fallback **Electron** se virar must-have no Linux. **Pendência ativa** —
  ver [`.continue/estado-atual.md`](../.continue/estado-atual.md).

## Notas técnicas

- **CSP × página remota:** o `security.csp` do `tauri.conf.json` governa **só a casca
  local**; na página remota vale o **CSP do servidor ShvIA**.
- **Render em VM/sem GPU:** rodar com `WEBKIT_DISABLE_DMABUF_RENDERER=1` (ADR-006 /
  [build.md](build.md)).
