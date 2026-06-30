# Roteiro de Fundação (F0 / F1) — ShvIA Desktop

Passo-a-passo concreto para tirar o projeto do zero. Faça na ordem — o smoke-test
de streaming vem **antes** de qualquer scaffolding, porque é o maior risco.

> Pré-requisitos por SO: Rust + toolchain Tauri 2, Node 20+, e as deps de WebView
> (WebView2 no Windows; `webkit2gtk`/`libwebkit2gtk-4.1` no Linux; Xcode CLT no
> macOS). Conferir o `build.md` do SHVTERM como referência.

---

## Passo 0 — Em paralelo, no dia 1 (não bloqueia o código)

- [ ] **Iniciar procurement do cert EV Windows** (Azure Trusted Signing ou EV
      tradicional). É o **long pole de prazo** (dias a semanas de onboarding).
- [ ] Confirmar **Apple Developer Program** ativo (US$99/ano) e quem é o **dono**
      da pipeline de assinatura + rotação da chave do updater.

---

## Passo 1 — SMOKE-TEST do streaming SSE no Linux (RISCO #1)

**Objetivo:** provar que o `/chat` do ShvIA transmite (SSE / `fetch` +
`ReadableStream`) dentro do **WebKitGTK**, antes de investir no resto.

Caminho mais rápido (sem Tauri ainda): num **Linux** com WebKitGTK, carregar
`https://ia.blue3.com.br`, logar e enviar uma mensagem no `/chat`, confirmando
que a resposta **aparece token a token** (não de uma vez no fim, não trava).

- [ ] Se **OK** → seguir para o Passo 2 (Tauri).
- [ ] Se **falhar** → acionar ADR-006: avaliar fallback **Electron** (Chromium
      pinado), ainda thin-shell hospedado. **Não** ir para NativePHP.

> Verificar também o trecho de streaming no ShvIA: `public/js/app.js` (~linha
> 4003, `fetch` + `ReadableStream.getReader()`), e o middleware de auth de `/chat`
> em `routes/web.php` (ADR-005).

---

## Passo 2 — Scaffolding Tauri 2  ✅ (0.2.0)

> Feito com `npm create tauri-app@latest -- --template vanilla-ts`, scaffoldado em
> dir temporário e integrado ao repo **sem sobrescrever** README/CLAUDE/docs.

- [x] **Uma janela** carregando a casca local, que redireciona para a URL do
      servidor; default = `https://ia.blue3.com.br` (constante em `src/main.ts`;
      configurável via store na F2).
- [x] `identifier` = `cloud.blue3.shvia`, `productName`/título = `ShvIA`.
- [x] **CSP da casca local** (modelada do SHVTERM). **Atenção:** a página remota
      do ShvIA roda sob o **CSP do servidor**, não o do app (ver
      [arquitetura.md](arquitetura.md#segurança-csp--capabilities)).
- [ ] Plugins: `store`, `process`, `updater`, `notification`, `deep-link`,
      `single-instance`. **(F2 — só `opener` entrou na 0.2.0.)**
- [x] Versão sincronizada de `version.md` por `scripts/sync-version.mjs` (hook
      `prebuild`). Integrar à CI fica para a F4.

---

## Passo 3 — Auth + persistência de sessão

- [ ] Logar no ShvIA pela janela (tela Breeze).
- [ ] **Fechar e reabrir o app** → confirmar que **continua logado** (cookie de
      sessão persistido na partição do WebView). Testar com e sem "remember me".
- [ ] Documentar onde o WebView guarda a sessão por SO (para a tela de "sair").

---

## Passo 4 — Branding mínimo (entrega a F1)  ⚙️ parcial (0.2.0)

- [x] Ícones gerados de `IA/SITE/public/logo.svg` (o `brand/` documentado não
      existia neste checkout). Gradiente achatado p/ teal sólido `#1f8a70` na
      rasterização (ImageMagick não renderiza `url(#gradient)`). Trocar quando
      houver brand final.
- [x] Título da janela + ícone do app = ShvIA.
- [ ] **F1 PRONTO quando:** app abre, loga, `/chat` transmite (SSE), sessão
      persiste, cara do ShvIA. Falta: `cargo check` + abrir o app + smoke-test SSE
      (Passo 1) + persistência de sessão (Passo 3). Cada avanço = commit
      `0.x.y - <descrição>` com bump de `version.md`.

---

## Passo 5 — Colher ativos do SHVTERM (preparar F2/F4)

Copiar e re-targetar do SHVTERM (`/Users/samir/Projetos/SHVTERM`):

- [ ] `.github/workflows/` — matriz de build Tauri (`tauri-action`).
- [ ] Fluxo do **updater** (`TAURI_SIGNING_PRIVATE_KEY`, `latest.json`).
- [ ] `scripts/packaging/{appimage,deb,rpm}` + `metainfo.xml`.
- [ ] Padrões de `tauri-plugin-store`, single-instance, tray.

---

## Depois (F2+)

- Tray/menu/About, estado de janela, config de URL no 1º run, **tela offline**.
- **Deep-link `shvia://`** + `SANCTUM_STATEFUL_DOMAINS` (SSO).
- Notificações de SO de eventos da página.
- **(Decisão)** sidecar Python só se precisar de vault de token/keychain ou ações
  nativas na `/api/v1`.
- Check de versão de servidor via `GET /api/v1/health` (F3).
- CI completa + assinatura/notarização + auto-update (F4).

> Estado vivo e pendências: [`../.continue/estado-atual.md`](../.continue/estado-atual.md).
