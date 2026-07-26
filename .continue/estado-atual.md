# ShvIA Desktop — Estado e Continuidade

> **Ler primeiro.** Notas de continuidade (WIP/pendências). O que **já está
> implementado** migrou para [`../docs/funcionalidades.md`](../docs/funcionalidades.md).
> Última atualização: **30/06/2026**.

## Onde estamos

> **0.8.0 (15/07/2026) — Notificações nativas dos alertas de preço.** O rastreador
> de preços do ShvIA (server) ganhou alertas; o desktop agora os mostra como
> **notificação nativa do SO** mesmo com a janela em segundo plano. Ponte reusa o
> canal do Modo Code + `tauri-plugin-notification` (só API Rust, sem capability —
> ADR-011). `cargo check`/`clippy` passam. **Pendente:** teste ao vivo no **`.app`
> build** (no macOS a notificação exige app empacotado/assinado — em `tauri dev`
> pode não aparecer).

**Versão lançada com 2 pendências conhecidas.** A **Fase 1** está completa e validada
(app abre, loga por cookie, chat com **streaming SSE** funciona) e a **Fase 2** está
bem encorpada (multi-janela, branding, estado de janela, links externos, tela
offline, **empacotamento local** — `build-local.*` nos 3 SOs). O que funciona em detalhe está em
[../docs/funcionalidades.md](../docs/funcionalidades.md); como buildar em
[../docs/build.md](../docs/build.md).

## Pendências ativas (os 2 problemas desta versão)

> Lançamos **com** esses dois em aberto — são **limitações do WebKitGTK no Linux**
> (ADR-008), não do nosso código (que faz a parte dele).

1. **Microfone (voz) não captura.** O shell habilita `getUserMedia`
   (`enable-media-stream`/`mediasource`/`webrtc` + concede o `permission-request`) e
   o WebKitGTK **enumera o device** (ex.: BRIO), mas a **captura efetiva** não vai.
2. **Ctrl+V de imagem não cola.** `javascript-can-access-clipboard` ligado, mas o
   WebKitGTK não expõe a **imagem** do clipboard à página (texto funciona).

**Caminhos:** (a) validar em **macOS (WKWebView)** e **Windows (WebView2/Chromium)** —
tendem a suportar; (b) se virarem **must-have no Linux**, **fallback Electron**
(Chromium — ADR-003/006/008). Decisão de produto.

## Modo Code no Windows (11/07/2026 — ADR-010)

Implementada a **ponte do Modo Code no Windows (WebView2)** — antes o toggle
Chat|Code só aparecia no Linux/macOS (a ponte só falava WebKit). Agora o
`BRIDGE_JS` também fala `window.chrome.webview`, com o novo `windows_ipc.rs`
(`add_WebMessageReceived`) espelhando o `macos_ipc.rs`, e `resolve_anna()`
cross-platform. `cargo check`/`clippy` **cruzados p/ windows-msvc passam**;
o **`anna.exe`** ganhou CI (`SHVIA-CODE/.github/workflows/build-windows.yml`).
**Pendente de você:** buildar o desktop no Windows + colocar o `anna.exe` no
PATH e validar o loop ao vivo. **Próximo passo (opcional):** empacotar o
`anna.exe` como resource do instalador (hoje precisa estar no PATH).

## Radar / próximos passos

- **F2 restante:** tray/menu/About; **config de URL** no 1º run (`tauri-plugin-store`);
  **offline v2** (ping no Rust, p/ quedas que o `navigator.onLine` não pega).
- **F4 (assinatura/release):** **macOS ✅** — Developer ID + notarização + staple já
  no `build-local.sh` (credencial no keychain, serviço `shvia-notarize`; ver
  [../docs/build.md](../docs/build.md#macos--implementado-no-build-localsh)). Impede o
  macOS de mandar o app baixado p/ a **lixeira**. **Pendente:** Authenticode **EV**
  Windows, chave do **updater** Tauri (`latest.json`). **CI (Actions) removida por custo.**
- **Roadmap (ideias do time, em [SAMIR-v1.md](SAMIR-v1.md)):** Anthropic como
  **infra/modelo** alternativo; **rotinas agendadas**; **conectores** (Carbonio mail,
  Google Calendar, etc.).

## Decisões em aberto (confirmar com o time)

- [ ] **Online-only é aceitável** como propriedade de produto? (toda a arquitetura
      fina depende disso). Ver [escopo](escopo-projeto.md#decisões-em-aberto).
- [ ] **Verba + dono** do cert EV Windows (~US$300–600/ano) e Apple Developer
      (US$99/ano), incl. rotação da chave do updater.
- [ ] **Funções idênticas ao web** ou haverá **telas desktop-only**?
- [ ] **URL de DEV** do ShvIA (além de produção `ai.shvia.org`).
- [ ] **App ID** `cloud.blue3.shvia` — confirmar (usado como default).

## Ponteiros

- O que já funciona: [../docs/funcionalidades.md](../docs/funcionalidades.md)
- Build/empacotamento: [../docs/build.md](../docs/build.md)
- Arquitetura: [../docs/arquitetura.md](../docs/arquitetura.md) · ADRs:
  [../docs/decisoes.md](../docs/decisoes.md)
- Escopo/fases: [escopo-projeto.md](escopo-projeto.md) · Roteiro F0/F1:
  [../docs/roteiro-fundacao.md](../docs/roteiro-fundacao.md)
- **A verificar no ShvIA (servidor):** middleware de auth de `/chat`
  (`routes/web.php`, ADR-005); streaming SSE em `public/js/app.js` (~linha 4003, ADR-006).
