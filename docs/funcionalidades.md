# Funcionalidades implementadas — ShvIA Desktop

Registro estável do que o app **já faz**, por versão. (WIP e pendências vivem em
[`.continue/estado-atual.md`](../.continue/estado-atual.md); decisões em
[decisoes.md](decisoes.md); como buildar em [build.md](build.md).)

## Fase 1 — esqueleto andante (validado)

- **Shell fino Tauri 2** (`0.2.0`): janela com título/ícone **ShvIA**,
  `identifier` `cloud.blue3.shvia`. Uma casca local (`index.html` + `src/main.ts`)
  mostra um splash e **redireciona o WebView para `https://ai.shvia.org`** — daí
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
  `SERVER_HOSTS` (`ai.shvia.org`, `ia.shvia.org`, `ia.blue3.com.br` — allowlist
  EXATA de FQDN + esquema desde o `0.9.0`, sem curinga; ver ADR-016) para o
  **navegador do SO** (login é same-origin, não quebra).
- **Tela offline v1** (`0.4.1`): **tarja vermelha "Sistema Offline"** injetada em
  cada página (`on_page_load` + `eval`, via `navigator.onLine`); some ao reconectar
  e é clicável p/ recarregar.
- **Empacotamento local** (`0.4.5`): `tauri build` gera os instaladores
  (`.deb`/`.AppImage`/`.rpm` validados). Scripts `build-local.{sh,ps1,cmd}` cobrem os
  3 SOs — ver [build.md](build.md). (CI/Actions omitida de propósito, por custo.)
- **Menu `Ajuda → Sobre o ShvIA Desktop`** (`0.5.3`): modal "Sobre" (estilo
  Help → About do VS Code) com o **build do desktop** (`version.md`) separado da
  **versão do ShvIA no servidor** — o rodapé da sidebar
  (`.account-mini__version`) dá o valor imediato e `GET /api/v1/health`
  (`version.app`) é consultado **sempre** como fonte viva (corrige se o servidor
  atualizou com a janela aberta); "—" só sem ambos (login/offline). Linha de
  ambiente (Tauri · WebView · SO) e botão **Copiar** p/ colar em chamados.
  Mesmo padrão das outras pontes: injetado por `eval` sob demanda, sem IPC à
  página remota (postura de menor privilégio — [arquitetura.md](arquitetura.md));
  visual pelos design tokens do ShvIA (`var(--token, fallback)`), Esc/backdrop
  fecham, `prefers-reduced-motion` respeitado.

## Phase 3 — Modo Code and the bridge (1.0 → 1.4)

> Written in English per `~/.claude/CLAUDE.md` (02/09/2026). The sections above predate that
> rule and stay in Portuguese until they are next edited.
>
> This section closes finding **F-22**: the document stopped at **0.18.1** while the
> repository was at **1.4.3** — an entire major line missing. The same file had already been
> sanitised once, on 07/08, for the same reason, which is why `scripts/prova-frescor-da-doc.mjs`
> now fails when the gap reopens rather than trusting anyone to notice.

- **Modo Code inside the app** (`1.0.x`): the desktop hosts `anna` as a sidecar and speaks its
  NDJSON protocol, so the agent runs on the developer's machine while the UI stays the ShvIA
  Blade. Two engines are selectable — the gateway, and **"Claude Code (assinatura)"** through
  the Agent SDK.
- **The model catalogue comes from the SDK, not from a list of ours** (`1.1.28`, `1.1.29`):
  keeping a copy meant the selector offered "Opus" = Opus 4.8 weeks after Opus 5 shipped.
  `--modelos` asks `supportedModels()` instead.
- **`gitDiff` on the bridge** (`1.2.0`) and **`readFile`** (`1.3.0`): the "Changes" tab and the
  "Files" tree have something to open. Both are read-only and confined — see the fence below.
- 🔒 **Authorised-folder fence** (`1.4.0`, finding F-12): `gitStatus`, `listTree`, `gitDiff`,
  `readFile` and `spawn` only reach paths the user picked through the **native dialog**. Before
  this, the bridge reached any path the process could.
- 🔒 **The runner stops being the back door** (`1.4.0`, F-13, **ADR-032**): the
  `claude-runner` policy became a testable module (`politica.mjs`), `WebFetch`/`WebSearch` left
  the read-only set, and `preToolUse` delegates to one decision function.
- **Sidecar PATH** (`1.1.24`, `1.4.0`): a GUI app does not inherit the shell PATH
  (**ADR-030**); the sidecar's PATH is the union of a base, the current one and the extras, so
  `node`, `cargo` and `php` are found.
- **Version carriers proven at build time** (`1.1.22`): six carriers agree or the build fails.
- **Native packaging on Arch, and update that works there** (`1.1.16` → `1.1.19`).
- **Publishing runs as `b3sys`** (`1.3.3`): root no longer opens SSH to the servers.
- **CI on every push** (`1.4.1`, finding G-24): `cargo test`, the runner policy proof, the
  version-carrier proof and `clippy -D warnings`.
- 🔒 **`target=_blank` windows go through the same builder** (`1.4.2`, F-15): they were born
  without `on_navigation`, so an external link navigated **inside** the app — a third-party
  site in a window titled "ShvIA".
- **RustSec advisories measured** (`1.4.3`, DEP-3/F-31): `cargo deny` in CI, and the first run
  found a real vulnerability in the pinned `time`.
- 🔴 **The permission hook comes back from the dead** (`1.4.13`): `preToolUse` referenced an
  `EDIT_TOOLS` that had vanished when the policy was extracted in `1.4.7`, so it threw on every
  tool call — the ADR-032 approval boundary was off the air and nothing said so. The proof
  missed it because it declared its own copy of the set; both sets now come from `politica.mjs`,
  and `prova:runner-version` is the trigger for the half that content tests cannot cover.
- **Subscription auth normalised before every SDK door** (`1.4.14`): the `ANTHROPIC_API_KEY`
  strip sat below the `--modelos` branch, so discovery answered under an API key while turns ran
  on the subscription — measured, the catalogue came back describing "$5/$25 per Mtok".
- **Claude Code account profiles** (`1.4.16`, **ADR-033**): a CONTA selector picks between the
  company and personal subscriptions. The page sends an ID from a closed list; Rust resolves it
  and sets `CLAUDE_CONFIG_DIR` **on the child**, so two windows can hold two accounts at once.
  Discovery and spawn share one resolver, and an ID that does not resolve fails instead of
  quietly landing on the default account. See [code/CONTAS-CLAUDE.md](code/CONTAS-CLAUDE.md).
- **The runner reports the turn errors the SDK actually emits** (`1.4.39`, block B0 of the
  Run): `success` with `is_error` and the `error_*` subtypes draw the error line; the two
  caps come out as `warn`. Before, a 401 ended a turn with no line at all.
- **The Run reaches the engine** (`1.5.0`, blocks B2 and B3 of
  [code/RUN-20260910.md](code/RUN-20260910.md), ADR-034): with `--parada host` the Claude
  runner asks the host before ending a turn (`stop_request`, blocking, 60 s ceiling → stop)
  and takes the run caps from the command line (`maxTurns`, `maxBudgetUsd`); the bridge
  turns the page's `autonomy` object into those flags and declares `recursos.run`. Nothing
  on screen yet — the page (SHVIA-WEB, blocks B4 and B5) is what arms a run.

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
