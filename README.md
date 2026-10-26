# ShvIA Desktop (`shvia-desktop`)

[Português (Brasil)](README_br.md)

ShvIA Desktop is the native **Tauri 2 + Rust** client for ShvIA on macOS, Windows
and Linux. It opens the hosted ShvIA interface and adds local Code engines, native
file dialogs, desktop integration and controlled access to this machine.
The default server is [ai.shvia.org](https://ai.shvia.org); the owner can configure
another trusted ShvIA server. Internal project documentation.

**Reviewed on 2026-10-26 against runtime baseline 1.30.6 (`280a85d`).** The current repository
version is in [`version.md`](version.md). This overview describes implemented
capabilities, not a claim that every platform or installed build passed acceptance.

Start with [Project state](docs/project-state.md), [documentation review](docs/documentation-review-20261015.md)
and the [documentation index](docs/README.md). Development conventions are in
[AGENTS.md](AGENTS.md); decisions are in [the ADR register](docs/decisoes.md).

## Product and architecture

The main window loads the server's own Laravel/Blade interface. Chat, projects,
bots, automations, knowledge, meetings, comparison, account settings and administration
therefore follow the configured Web deployment and the signed-in user's permissions.
The local TypeScript/Vite shell provides startup, connection feedback, server
configuration and recovery screens; it is not a second implementation of the Web app.

The server owns application data, accounts and permissions. The desktop does not
run a local application database. It does retain WebView cookies, native preferences,
folder permissions, engine/account configuration and the private local records needed
by the assessment workflow. An offline screen is available; offline AI chat and
replication of the server are not implemented.

The remote page uses a bounded native bridge, with origin checks, a session capability
token, project-folder checks and native confirmation where required. General Tauri
IPC and updater control are not exposed to the remote page. Selecting a custom
server is a trust decision: that server receives the app's supported native bridges.
The connection probe checks reachability, not that the remote site is a ShvIA instance.

| Execution surface | Where work runs | Credential/accounting boundary |
|---|---|---|
| Web features inside the app | Configured ShvIA server and its providers | The normal same-origin web login; server permissions and accounting |
| Local Code, gateway engine | `anna` on this machine, inference through ShvIA | Short session/delegated credentials; independent `anna` binary |
| Local Code, Claude engine | `claude-runner` and the Claude Agent SDK on this machine | Selected local Claude account/subscription; inference goes directly to the provider |
| Local Code, Codex engine | `codex-runner` → `codex app-server --stdio` on this machine | The account stored by `codex login`; its subscription, not an injected OpenAI API key |
| Remote Code | The Workspace service configured by ShvIA Web | Account-bound remote project/session; files and execution remain remote |

Local and remote Code share Web UI concepts but use different bridges, filesystems
and credential flows. A remote Workspace file preview does not grant this desktop's
local filesystem access. See [Code mode](docs/code/MODO-CODE-20260709.md) and
[Project state](docs/project-state.md) for the current boundaries.

## Implemented native capabilities

| Area | Current capability | Details |
|---|---|---|
| Window and OS integration | Multiple windows, single-instance handoff, persisted geometry, tray/menu, configurable close-to-tray and autostart | [Features](docs/funcionalidades.md) |
| Quick access | Quick Chat, selectable global shortcut and approved `shvia://` deep links | [Architecture](docs/arquitetura.md), ADR-038 |
| Notifications and updates | Native notification polling, supported icon badges, automatic/manual update checks and confirmation before installation | [Build and distribution](docs/build.md), ADR-011/022/028 |
| Files and tools | Native pick/save dialogs, local project tree/status/diff/read, artifact saving, image-paste support and native CLI configuration | [Code mode](docs/code/MODO-CODE-20260709.md), [architecture](docs/arquitetura.md) |
| Code supervision | Engine/model/effort selection, per-project sessions, reload reattachment, approval cards, Plan, steering and supervised Run | [Run](docs/code/RUN-20260910.md), [Codex](docs/code/MOTOR-CODEX-20260909.md) |
| Engine accounts | Claude account profiles and login flow, runtime model catalogs, plan usage reads; bundled runner sources and install/update support | [Claude accounts](docs/code/CONTAS-CLAUDE.md), [plan usage](docs/code/USO-DAS-CONTAS.md) |
| Claude for remote Workspace | Native bridge implemented: consent, `claude setup-token` in a private terminal and direct token submission to the configured server; requires a compatible Web page to invoke it | [Subscription handoff](docs/code/ASSINATURA-NO-WORKSPACE.md) |
| Agent device/browser tools | Opt-in device operations, file picker and screen capture; opt-in isolated browser with confirmed actions | [Features](docs/funcionalidades.md), ADR-039/042 |
| Controlled assessments | Native consent/runtime checks, control/recovery, private review/retest, durable history, report synchronization and separate local viewer | [Authorization](docs/PENTEST/authorization.md), [compatibility](docs/pentest-compatibility.md), [viewer](docs/pentest-viewer.md) |

Web's Settings → Engines calls the subscription bridge when the app advertises it
(Web source `956c134f`, reviewed 2026-10-26); the manual token flow remains available.
A deployed compatible page and a real sign-in still need separate acceptance. See
[Project state](docs/project-state.md#pending-work-and-cross-repository-requirements)
for open work and Pentest's stricter quick-start version requirements.

Runner approval policies differ. Claude has its own tool policy and secret-path
gates. Codex uses its sandbox's writable set; writes within that set can run without
a card, and local secret reads are not filtered like Claude's. Plan changes the
turn's policy. The external-container Codex mode is explicit and bounded to its
configured directory. Read the [Codex runner contract](codex-runner/README.md) before
assuming one engine's approvals or privacy controls apply to another.

Direct subscription engines do not pass their inference through ShvIA's gateway:
gateway quota, PII masking and `inference_requests` accounting do not cover those
calls. Client-reported plan usage is a separate measure from server API spend.

## Platform boundaries

| Platform | Implementation and limits |
|---|---|
| macOS | WKWebView; local engines and native bridge; Claude subscription handoff and controlled assessment use the Unix implementation. Signing/notarization and installed acceptance are separate release steps |
| Windows | WebView2; native bridge and PowerShell runner installers exist. Claude subscription handoff and the controlled assessment profile are unsupported in this version. Native badge count is unsupported; use the [Windows validation guide](docs/roteiro-validacao-windows.md) for installed behavior |
| Linux | WebKitGTK 4.1; native media/paste and TTS integration exist, with microphone and real image-paste acceptance tracked in the [media checklist](docs/roteiro-microfone-e-colar.md). On Wayland, bind a system shortcut to `xdg-open shvia://rapida`; do not assume the app can register a global shortcut |

Screen capture needs OS permission/portal support. The agent browser has an isolated
profile, blocks pop-ups/downloads and may not support login flows that require them.
A VM/GPU environment that shows a blank Linux window can use the documented
`WEBKIT_DISABLE_DMABUF_RENDERER=1` workaround. Implemented platform code, compile
checks and a successful installed-user workflow are different evidence.

## Develop from source

The app uses Rust stable and Node **20.19+ or 22.12+** for the Vite toolchain
(CI uses Node 22). Install the platform's Tauri prerequisites: Xcode command-line
tools on macOS; MSVC C++ tools and WebView2 on Windows; WebKitGTK/GTK and the
packaging libraries on Linux. Exact packages are in [Build prerequisites](docs/build.md#pré-requisitos).

```bash
npm ci
npm run build
npm run prova:bump
npm run prova:politica
```

`npm run build` builds the local startup shell, not an installer. To develop the
native app with the gateway engine, stage a compatible `anna` from SHVIA-CODE first:

```bash
npm run anna:stage -- --from /absolute/path/to/anna
npm run tauri dev
```

If no `anna` is being bundled, Tauri's `externalBin` declaration must be overridden;
a clean clone does not contain a staged binary. For shell/other-engine development:

```bash
TAURI_CONFIG='{"bundle":{"externalBin":[]}}' npm run tauri dev
```

On PowerShell, set `$env:TAURI_CONFIG` to the same JSON before `npm run tauri dev`.
The override omits the bundled gateway engine; an installed external `anna` can
still be resolved at runtime. Never create a fake engine to ship a release.

The Claude runner requires Node 18+ and its SDK dependencies; the Codex runner
requires Node 18+ and an installed, authenticated Codex CLI. The app can install/update
runners from bundled sources; see [Claude runner](claude-runner/README.md) and
[Codex runner](codex-runner/README.md). There is no planned Python/PyInstaller
sidecar in the current architecture. Optional tools and assessment runtimes have
their own dependencies.

Rust checks run from `src-tauri` (`cargo test --locked`,
`cargo clippy --locked --all-targets -- -D warnings`), with the same engine staging
or `TAURI_CONFIG` override. [CI](.github/workflows/ci.yml) also runs focused runner,
installer, version, dependency and publication guards. Passing them does not build,
sign, publish or install a release.

## Version, build and distribution

[`version.md`](version.md) is the desktop version, independent of ShvIA Web and the
engines. `npm run version:sync` propagates it to the manifests/lockfiles;
`npm run prova:bump` verifies the carriers. A change records its version and changelog
in the same commit. Subjects and changelog headings use **`X.Y.Z - description in
English (US)`**. Z covers visible/packaging/build changes; Y covers new runtime
capabilities, IPC or auth-handoff changes; X is a manual stable-version change.

The runtime compatibility notice compares this installed desktop **BUILD** with
`clients.desktop.min_version` returned by authenticated `GET /api/v1/version`
(fallback to the old health payload). It warns, can be dismissed and does not
block the app. It is not a minimum-server-version requirement; `latest_version`
alone does not trigger the warning. Individual native capabilities and assessment
runtime checks have their own gates.

Release packaging is local, on each target OS: `build-local.sh` for macOS/Linux,
`build-local.ps1` or `build-local.cmd` for Windows. Outputs include DMG/app archive,
MSI/NSIS, AppImage/DEB/RPM and the separate Arch packaging flow. Users receive
published installers/update manifests from the configured distribution server;
GitHub Releases record the version and published artifacts. Arch updates use pacman.
See [Build, signing and publishing](docs/build.md) for credentials, verification,
checksums and publication options.

A repository release, a packaged installer, valid signatures/notarization, server
publication and an installed-app acceptance run are separate results. Report each
explicitly; a new commit or green CI does not update anyone's installed app.

## Repository and identity

| Path | Responsibility |
|---|---|
| `src/`, `index.html` | Local TypeScript/Vite startup and connection shell |
| `src-tauri/src/` | Rust windows, bridge, engines, OS integration and assessment lifecycle |
| `claude-runner/`, `codex-runner/` | Node runners, protocol/policy modules and installers |
| `scripts/`, `build-local.*`, `packaging/` | Staging, validation, packaging and publication tooling |
| `src-tauri/icons/`, `brand/`, `public/` | Native exports and Shield assets |
| `docs/` | Current contracts, runbooks and decisions |
| `.continue/` | Pending work/design; its existence is not implementation evidence |

**Shield** is the active ShvIA identity: graphite/titanium shield, cobalt left
hemisphere and orchestration nodes. Keep ShvIA distinct from Blue3's corporate
identity. [Brand sources](brand/README.md) are canonical for this checkout;
`brand/versao2-portal/` preserves the prior artwork. SHVTERM is a separate sibling
project whose desktop patterns informed this client; the discarded Claude Desktop
fork remains historical, not the current architecture.
