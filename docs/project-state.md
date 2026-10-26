# ShvIA Desktop: current product and capability map

**Reviewed:** 2026-10-26 · **Implementation baseline:** `280a85d`, version 1.30.6.
This review describes source behavior; it does not certify an installed application,
a signed installer, a live login or the current hosted server.

## What the application is today

ShvIA Desktop is a Tauri 2 client for ShvIA. Its local Vite/TypeScript shell starts the
application and handles the offline/server selection experience; the main interface is
loaded from the configured ShvIA Web server. Rust provides native windows, the constrained
bridge and local capabilities. Code uses SHVIA's `anna` engine plus the Claude Code and
Codex runners. There is no implemented Python sidecar in this repository.

| Component | Owns | Update boundary |
|---|---|---|
| ShvIA Web | Account, permissions, shared interface, stored conversations/Code transcript, gateway and workflow APIs | A server deployment can change the interface without changing the installed shell |
| Desktop shell | Native bridge, windows, menus, server selection, files/Git, OS controls, local process supervision, updater and assessment integration | Requires an installed compatible Desktop build |
| Coding engines | `anna`, Claude runner/SDK and Codex runner/client execution | Bundled sources and runtime installation/update paths have their own versions and requirements |
| SHVIA-WORKSPACE | Remote repository environments and agent execution | Remote Code belongs to the Web/Workspace integration, not a local Desktop folder binding |
| External providers | Subscription login, model availability and quota reports | Provider/client state is not established by a local `found` or version check |

The Web screen checks bridge capabilities (`recursos`) before offering native features.
A new server can meet an older shell, so capability checks are more precise than assuming
all actions exist because the page is current. The separate compatibility banner compares
the installed Desktop version with the server's `clients.desktop.min_version`; it warns
and can be dismissed. It is not a hard startup block or a minimum-server-version gate.

## Current capabilities and documentation

| Area | Implemented capability | Evidence | Guide / limits |
|---|---|---|---|
| Shared application | Server-loaded ShvIA UI, same-origin session cookies, splash/offline and configured server | `src/main.ts`, `src-tauri/src/server.rs`, `lib.rs` | [Architecture](arquitetura.md); account/database remain on Web |
| Window management | Multiple windows, single-instance focus, saved geometry and menus/tray | `lib.rs`, `tray.rs` | [Features](funcionalidades.md); local sessions belong to their owning window |
| Quick access | Quick window/global shortcut and constrained `shvia://` routes | `rapida.rs`, `esquema.rs` | [Features](funcionalidades.md), ADR-038 in [decisions](decisoes.md) |
| Startup and navigation | OS autostart integration, external links opened by the OS browser | `tray.rs`, `lib.rs` | [Features](funcionalidades.md); OS permissions and actual registration need installed testing |
| Diagnostics and updates | App/server information, health/version reading, automatic/manual update checks, confirmed installation/restart | `diagnostico.rs`, `updater.rs`, `lib.rs` | [Build/distribution](build.md), [architecture](arquitetura.md); updater uses the configured server and signed artifacts |
| Native trust boundary | Fixed bridge actions, per-process (app-session) capability token and native folder fence; remote page has no general Tauri command surface | `code_bridge.rs`, `code_bridge/shim.rs`, `code_bridge/cerca.rs` | [Architecture](arquitetura.md); origin/token and folder checks do not establish that all server content is safe |
| Local project files | Native folder/file picker, device-local project bindings, tree/file preview and Git status/diff | `code_bridge/dialogos.rs`, `painel.rs`, `cerca.rs` | [Local Code contract](code/MODO-CODE-20260709.md); previews are bounded, not a general file editor |
| Artifact save | Native save dialog for bytes supplied by the authenticated page | `code_bridge/dialogos.rs` | [Features](funcionalidades.md); runtime WebView behavior requires installed checks |
| Local coding engines | SHVIA/anna gateway, Claude Code and Codex, with distinct catalogs/credentials and runner installation/update paths | `code_bridge/motores.rs`, `claude-runner/`, `codex-runner/` | [Claude runner](../claude-runner/README.md), [Codex runner](../codex-runner/README.md), [Codex engine](code/MOTOR-CODEX-20260909.md) |
| Agent supervision | Approval/stop requests, autonomous Run handshake, plan turns and steering when the engine supports them | `code_bridge.rs`, `shim.rs`, runner policy/protocol modules | [Run design and delivered blocks](code/RUN-20260910.md), [Run acceptance procedure](code/RUN-ORCHESTRATOR.md); interface orchestration still comes from Web |
| Session continuity | Named sessions, per-project background work, bounded replay and reattachment after document reload for resumable sessions | `code_bridge/sessao.rs` | [Features](funcionalidades.md); window closure ends its sessions; reload persistence is not app-restart persistence |
| Claude profiles/login | Registered directory/credential profiles, explicit alias discovery, native login flow and account selection | `contas_claude.rs`, `code_bridge/login.rs`, `motores.rs` | [Claude accounts](code/CONTAS-CLAUDE.md), [login diagnosis](code/MOTOR-CLAUDE-DIAGNOSTICO.md); profile presence is not proof of login |
| Engine plan usage | Claude profile and Codex account usage read through the official clients | `code_bridge/motores.rs`, runner `--uso` paths | [Account usage](code/USO-DAS-CONTAS.md); percent used, unknown stays unknown |
| Claude subscription handoff | Native `claude setup-token`, progress phases and direct submission to the configured Web server | `code_bridge/assinatura.rs`, `assinatura/` | [Subscription connection](code/ASSINATURA-NO-WORKSPACE.md); Linux/macOS, explicit native consent; requires a Web page that calls the capability, real sign-in remains an acceptance step |
| CLI configuration | Generate/merge supported client settings locally after native confirmation; replace through a private staged file without creating adjacent backups | `cli_config.rs`, `cli_config/storage.rs` | [ADR-026 update](decisoes.md#credential-file-replacement-2026-10-15); existing backups remain untouched and snapshot checks are not a concurrent-write lock |
| Device commands | A fixed set of device operations behind native tray enablement and command-specific consent | `code_bridge/aparelho.rs`, `tray.rs` | [Features](funcionalidades.md); `recursos.aparelho` reports implementation, not whether it is enabled |
| Agent browser | Separate local browser window/profile; bounded read/click/type/open/close; site and per-action consent | `code_bridge/navegador.rs`, `navegador.js` | ADR-042 in [decisions](decisoes.md), [features](funcionalidades.md); not control of the person's existing Chrome session |
| Controlled assessments | Native authorization/consent, one supervised local process, status/stop/control, recovery/review/retest and private history/report outbox | `code_bridge/pentest.rs`, `pentest/` | [Authorization](PENTEST/authorization.md), [compatibility](pentest-compatibility.md); supported Unix path only, fresh consent and runtime/version checks |
| Assessment viewer | Separate OS browser, loopback/private viewer session and child cleanup | `code_bridge/pentest/viewer.rs` | [Viewer](pentest-viewer.md); Web does not receive viewer capabilities or local paths |
| Branding | Shield assets, app/tray/splash icon usage and regeneration | `brand/`, `src-tauri/icons/`, `public/brand-mark.svg` | [Brand guide](brand.md), [asset provenance](../brand/README.md); earlier Portal assets are historical |

## Platform and lifecycle limits

| Capability | Linux | macOS | Windows |
|---|---|---|---|
| Shared interface/native shell | WebKitGTK path | WKWebView path | WebView2 path |
| Local Code bridge/runners | Implemented | Implemented | Implemented with Windows transport/install paths; use the Windows acceptance procedure |
| Claude subscription connect through the app | Implemented | Implemented | Native capability off; use the Web's manual token flow |
| Controlled assessment execution/recovery | Implemented with runtime preflight | Implemented with runtime preflight | Explicitly refused |
| Microphone/image paste | WebView/system behavior needs the [Linux procedure](roteiro-microfone-e-colar.md) | OS permission/WebView acceptance required | OS permission/WebView acceptance required |
| Packaging/updater | Local packaging and publication path | Local signing/notarization/publication path | Local packaging/signing/publication path |

These are implementation boundaries, not a claim that every platform has passed an
installed-device pilot. Platform CI checks compilation/lint; it does not click a native
consent dialog, verify microphone access or prove a shipped installer contains every runtime.

Local session defaults from `sessao.rs`: at most **eight per window**, an **8 MiB** replay
log per session and cleanup of an idle detached session after **ten minutes**. A process
may survive a document reload only when started as resumable. Closing its window ends it;
an app restart is not general local-Code recovery. Controlled assessments have their own
private restart-recovery design and must not be used to imply Code-session persistence.

Codex's external-sandbox mode is an engine deployment option: the host must provide the
read-only boundary and the runner proves it at startup. Its read-only Plan mode is refused
there. This option does not by itself make remote Codex available in ShvIA Web.

## Where data and credentials live

The server owns account data and stored application content. The client still keeps local
state: configured server URL, WebView cookies/storage, project bindings and authorized
folders, Claude profile metadata, engine caches and the agent browser's separate profile.
Assessment descriptors/outboxes are a separate native private store. See
[architecture and persistence](arquitetura.md) for exact files and boundaries.

At the separately reviewed Web baseline (`956c134f`, 2026-10-26), Settings → Engines
calls the Desktop subscription-connect methods in `public/js/motores.js` when the
bridge advertises `recursos.assinatura`; the manual token flow remains available.
This supersedes the missing-caller observation from 2026-10-15. Source availability
does not establish deployment of that page or acceptance of a real sign-in.

A local Claude profile/login is distinct from the subscription token submitted to Web for
remote execution. The latter does not pass through the page in the native connection flow;
its progress notifications do not prove a completed login until the native flow confirms it.
Never describe “no application database” as “no local state” or “no credentials on disk”.

## Pending work and cross-repository requirements

The following PRs were still open on 2026-10-26 and are not part of this baseline:

- [#154](https://github.com/samirhvbr/shvia-desktop/pull/154): discard superseded
  connection probes when the configured server changes.
- [#155](https://github.com/samirhvbr/shvia-desktop/pull/155): release the Codex
  message queue when the app-server rejects turn startup.
- [#156](https://github.com/samirhvbr/shvia-desktop/pull/156): probe servers at IPv6
  literal addresses correctly.
- [#151](https://github.com/samirhvbr/shvia-desktop/pull/151) and
  [#160](https://github.com/samirhvbr/shvia-desktop/pull/160): native traffic capture
  and IPv4 range scanning, respectively. Neither is a current released capability
  established by this source baseline.

Pentest's native scope-only consent is implemented, but the current build does not
bundle Strix. Web's quick-start contract at `956c134f` requires Desktop **1.32.0+**
and engine **1.2.43+**, and quick start is disabled by default. Desktop 1.30.6 does
not satisfy that requirement. See [compatibility](pentest-compatibility.md#current-build-and-web-quick-start)
before equating native consent support with an available quick-start installation.

## Build, release and acceptance

[`version.md`](../version.md) is the version source. The synchronization script propagates
it to six carriers, checked by `npm run prova:bump`. Documentation review does not
establish that a new binary was built or distributed.

Read [build.md](build.md) before packaging. A frontend build, Rust tests/Clippy, a native
package, OS signature/notarization, published artifacts, a GitHub Release, a matching update
manifest and installed-client acceptance are distinct outcomes. The CI workflows do not
replace the local signed-release process. No installer or runtime update was produced by
this documentation review.

Start from [the documentation index](README.md) for procedures and dated design history.
The [2026-10-15 review record](documentation-review-20261015.md) preserves earlier
findings with follow-up status; it is not an approved implementation roadmap.
