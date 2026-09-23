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
- **Portal branding** (`1.5.12`): cyan/navy Portal app icon, transparent monochrome tray template and matching splash symbol. Canonical artwork and historical snapshots live in SHVIA-WEB `brand/`; see [brand.md](brand.md) for regeneration.
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
- **The page can raise a native notification** (`1.5.3`): the shim exposes
  `__shviaCode.notify({title, body})`, fire-and-forget, carrying the bridge token like
  every other message. Until then only the shell's own poll could post `notify`
  (ADR-011); a page posting to the native handler by hand was dropped for lack of the
  token — and is banned on the web side (finding F-16). The Run's gate card (SHVIA-WEB
  block B5) uses it when the window is not in front. Presence of the method is the flag.

## Phase 4 — the Run in Code mode (1.5.x)

The plan is [`code/RUN-20260910.md`](code/RUN-20260910.md), the decision is
[ADR-034](decisoes.md#adr-034), and the screen is
[`code/modo-code-run-mockup.html`](code/modo-code-run-mockup.html).

**What the Run is.** Code mode continuing its own turns and stopping only where a person
is needed. It is not a new mode: it is a posture of the turn. The **Autonomia** pill, next
to Aprovação, chooses between *Um turno* and *Até terminar* per project; the approval gate
for commands keeps working inside a run exactly as outside it. Nothing runs or writes
without the person seeing it — the Run changes who types "continue", not who approves.

**Who decides, at every stop.** The page asks `POST /api/v1/code/orchestrate` and gets one
decision back — CONTINUE, ASK_HUMAN, DONE or STOP — signed by whoever took it. The rule
tier decides alone and costs nothing: markers first (a run that says it finished, finished),
then the caps, then announced irreversible actions (which never reach a model — policy is
code), then a question-versus-report classifier. Only where the rule stopped on a question
or a handoff, and only if the person pinned an **Orquestrador** profile on the project, is a
gateway model consulted — bounded at 20 s, audited as `origin = code-orch` like any other
inference, and any failure of it falls back to the human signed by the rule.

**What the person sees.** A run bar between turns (Pausar, Retomar, Parar), one line per
decision on the timeline saying who decided and why, a gate card with three exits when it
stops, and a summary when it ends. In the Histórico, a run is one group inside its session,
under the numbers the summary showed.

🔴 **State, honestly, on 12/09/2026.** The server and the page are **in production**
(SHVIA-WEB `2.110.254` through `2.110.258`). The NDJSON protocol that documents
`stop_request` merged in SHVIA-CODE `0.11.22` on 12/09
([#1](https://github.com/samirhvbr/shvia-code/pull/1)). The desktop side — the runner
reporting turn errors, asking the host before ending a turn, and carrying the run caps —
lands with this commit ([shvia-desktop #5](https://github.com/samirhvbr/shvia-desktop/pull/5)).
What is still **not** done is **B9**: the five real runs that measure the rule. Until B9 runs,
no default orchestrator profile exists and Q5 of the plan stays open.

**Not measured yet.** Five real runs on the three engines, rule only (block B9), counting
escalations by signal and false continues. Until that number exists there is **no default
orchestrator profile** — the pill opens on *Regra, sem modelo*.

## Phase 5 — the three gates a new user walks through (1.5.15 → 1.6.0)

Measured on 16/09/2026: someone who installs ShvIA and picks the Claude engine meets **three
gates in order**, and until this phase only the third had any answer on screen.

| gate | before | now |
|---|---|---|
| **1 · no runner** | *"run `claude-runner/install.sh`"* — a file only a cloner has | the source rides in the installer (~124 KB) and a button runs `npm ci` · **1.5.16** |
| **2 · no login** | `claude auth login` in a terminal; `/login` does not exist in the panel | URL and code field on screen, verdict from `claude auth status` · **1.5.17 → 1.6.0** |
| **3 · no account** | hand-edited JSON, or `npm run contas` from a clone | Settings detects the shell's aliases and registers the chosen ones · **1.4.28/1.4.29** |

Two defects were found **under** gate 1 while opening it, and both had been shipping silently:

- **1.5.15** — `install.sh` copies its files by a hand-written list that never gained
  `parada.mjs` (the Run's Stop hook, added in 1.5.0). Since then a fresh install refused at
  its own guard, and an existing one kept answering turns **without the Run**. Measured here:
  installed runner `1.4.20` against repository `1.5.14`. `prova:instalador` now reads the
  imports against both lists — the `cp` and `bundle.resources` — as text, because the
  installer's own guard only fires **during** an installation and CI does not install.
- **1.6.0** — the login flow has **two endings**, and 1.5.17 only handled one. The browser can
  finish it with no code pasted; an invalid code does not end the process; the exit code is
  `0` either way. The screen half is in `SHVIA-WEB`.

🔴 **Node is the cost of one choice, not a product requirement.** Measured: `claude` itself is
a self-contained Mach-O binary; the runner is a shell wrapper that `exec`s `node` with the
`.mjs` because it carries the Agent SDK. Whether the runner should speak to the native binary
instead is open, and it is the honest form of "gate 1 still needs Node".

## Phase 6 — hardening after the 23/09 review (1.6.1 → 1.6.26)

A four-front review on 23/09/2026 (bridge, Rust, runners, build/release) found defects that
were shipping silently; each fix below has a proof that fails without it. What changed for
someone who uses or builds the app:

- **Releases carry their installers** (`1.6.6`): `--publish` attaches what it sent to the
  server to the GitHub Release of the same version — the record of what went out, per OS.
- **Windows compiles again** (`1.6.8`): no Windows build was possible from 0.9.0; it is
  compile-checked now, still never run on a real Windows machine.
- **Linux can quit** (`1.6.9`): `Sair` and `Fechar janela` exist in the menu and the tray
  (the predefined items were silently dropped on GTK).
- **The Changes tab works with accents and spaces** (`1.6.10`).
- **Reloading or leaving the page ends that window's agent** (`1.6.11`).
- **The Claude engine's approval boundary holds in Auto mode** (`1.6.12` → `1.6.15`):
  network egress, protected paths and destructive commands always ask — through tools and
  through the shell; `~` and symlinks cannot walk out of the read fence; destructive commands
  are recognized by what they do, not how they are spelled.
- **The Codex engine ends a turn whose app-server died** (`1.6.17`), and a malformed host line
  no longer kills either runner (`1.6.18`).
- **Slow bridge work no longer freezes the windows** (`1.6.19`): installs, the login, account
  discovery and `git status` run off the UI thread, and every external call has a deadline.
- **Publishing refuses what it cannot vouch for** (`1.6.20` → `1.6.24`): a failed read of the
  published manifest, a signature from another key, an unsigned Mac build, a stale reused
  bundle, a stale bundled engine; and building without `anna` works.
- **Installing the runner from a terminal finishes** (`1.6.25`); **restarting the login keeps
  it** (`1.6.26`).

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
