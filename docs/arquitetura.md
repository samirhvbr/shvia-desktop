# Architecture — ShvIA Desktop

ShvIA Desktop loads the hosted ShvIA Web interface and adds native capabilities through a
Rust bridge. The server owns accounts and shared application data; local engines, authorized
folders, browser profiles and assessment artifacts remain on the computer. See
[the current capability map](project-state.md) and [the decision records](decisoes.md).

> **Reviewed against source on 2026-10-15, Desktop 1.29.0.** This describes implementation
> contracts, not a deployed server or installed-client acceptance. Dated measurements below
> retain their original scope. The original Python-sidecar/F2 sketch is superseded: there is
> no Python `sidecar/` in the current app. Native services are Rust; local Code uses anna and
> the Node-based Claude/Codex runners.

---

## Layers

```
Local bootstrap (src/, Vite/TypeScript)
  └─ Rust server configuration and TCP reachability probe
      └─ Native WebView navigates to the configured ShvIA server
          ├─ Shared Blade/JavaScript UI, Web session and API token
          ├─ HTTPS → ShvIA Web (Laravel, MariaDB, inference gateway)
          └─ Custom native bridge → Rust capability dispatch
              ├─ Authorized local folder → anna / claude-runner / codex-runner
              ├─ Native file dialogs, notifications, device and browser actions
              └─ Controlled Strix assessment → private local artifacts/outbox

Remote Code is a separate path owned by the Web UI:
  shared Code screen → ShvIA Web work channel → Workspace service → remote files/engine

Rust also owns windows/tray, quick window, shvia:// navigation, autostart,
window state, server.json and the update client.
```

The active Web knowledge retrieval is in MariaDB. A separate PostgreSQL/pgvector path in
the Web repository is dormant CLI functionality; it is not a Desktop runtime dependency.

---

## Authentication and navigation

1. The WebView navigates to the configured server (default `https://ai.shvia.org`).
2. The server's own sign-in page establishes the Web session cookie for that origin.
3. The shared Web UI also uses its panel Bearer token for API calls. The native shell does not
   implement a second account database or a general token vault.
4. Local Claude/Codex engines use their official clients' authentication. Choosing a local
   Claude profile resolves a local ID to the environment of that child process; it does not
   sign the ShvIA account into that provider.

`shvia://` is a closed navigation scheme (`esquema.rs`): bring the app forward, toggle the quick
window, open Chat with an optional **unsent** prompt, or open a settings section. It is not an
implemented SSO token/cookie handoff. A link cannot select a server, execute a request or approve
a gate. The configured server remains the destination authority.

The native capability for connecting Claude for **remote** Workspace execution is a different
credential flow: when called by a compatible Web page, `claude setup-token` runs after native
confirmation on Linux/macOS, and the shell sends the
token to the configured ShvIA server without exposing it to the page. The server encrypts its
copy. The reviewed Web source (`25704dbc`) has the manual token form but does not call these
native methods yet. Windows uses that paste path. See
[the subscription contract](code/ASSINATURA-NO-WORKSPACE.md) and
[local account profiles](code/CONTAS-CLAUDE.md).

---

## Segurança (CSP / capabilities)

- **CSP** da casca local (`security.csp` em `tauri.conf.json`): `default-src
  'self'` + `connect-src` com **apenas** `'self'` e `ipc:`/`http://ipc.localhost`
  para o bridge Tauri. Modelada do CSP do SHVTERM (`gui/src-tauri/tauri.conf.json`).
  Os FQDNs do ShvIA **saíram** do `connect-src` na 0.15.0: a casca não faz mais
  `fetch` no servidor (o probe é Rust), e um `connect-src` **estático** não poderia
  listar uma URL digitada pelo usuário — era esse o bloqueio do servidor
  configurável. Ver [ADR-019](decisoes.md#adr-019--servidor-configurável-probe-no-rust-e-o-csp-que-destravou).
  **Importante:** esse CSP governa **só a casca local** (splash/offline). Quando o
  WebView **navega para o FQDN**, a página passa a valer sob o **CSP do próprio
  servidor ShvIA** (cabeçalhos HTTP do Laravel) — o CSP do app **não** restringe
  nem protege a página remota.
- **Capabilities por janela** (`src-tauri/capabilities/`): expor ao WebView só os
  comandos/plugins necessários. Postura de menor privilégio. A capability `default`
  **não declara `remote`**, e isso é a guarda que faz o ADR-001 continuar valendo
  agora que existe um `invoke_handler`: o ACL do Tauri recusa `invoke` vindo de
  página remota, então um servidor comprometido **não** consegue chamar
  `shvia_server_set` e se tornar o destino permanente do app.
- **Build credentials stay outside the repository.** Local release builds use the operator's
  signing material. macOS prefers an App Store Connect API key for notarization; the historical
  keychain password is a fallback. Windows uses its certificate store or a `.pfx` outside the
  tree. The current paths and publication checks belong to [build.md](build.md).

The generic Tauri `invoke` boundary above is separate from the **custom Code bridge**. The
trusted configured origin receives `window.__shviaCode`; every message carries the process's
bridge token (`code_bridge/shim.rs`). File actions and local spawn require a folder authorized
by the native picker (`code_bridge/cerca.rs`). Native confirmations and tray switches guard the
specific device, browser, login and assessment actions. Changing the configured server changes
which origin receives those capabilities; the remote UI is therefore part of the trust boundary.

---

## Endereço do servidor (item D4)

O FQDN não é constante da casca desde a **0.15.0**. O Rust resolve, nesta ordem:

1. `server.json` no diretório de config do app (gravado pela tela de servidor);
2. o embutido `https://ai.shvia.org` (`server::DEFAULT_URL`).

```
casca local (index.html + main.ts)
  │  invoke shvia_server_config   ──► server::load    (fail-open: lixo → padrão)
  │  invoke shvia_server_probe    ──► TcpStream::connect_timeout (4 s)
  │  invoke shvia_server_set      ──► server::normalize + gravar + trocar perímetro
  │  invoke shvia_server_reset    ──► apagar → volta ao embutido
  ▼
alcançável → location.replace(url)   |   não → estado offline com auto-retry (5 s)
```

**O probe é Rust, e não é preferência de linguagem.** Dois motivos, na ordem de
importância: (1) `connect-src` estático não pode listar URL digitada — era o
bloqueio real do item; (2) o primeiro request do WebKit "frio" custava 5-6 s antes
de qualquer resposta ([ADR-012](decisoes.md#adr-012--timeout-do-ping-de-alcance-o-cold-start-do-webkitgtk-custa-5-6-s)),
o que forçava timeout de 15 s — agora são **4 s**.

**Reachability is not identity:** the probe opens TCP and closes it. It does not validate a
certificate or establish that the host is ShvIA. Navigation and the updater make their own
HTTPS requests; a successful TCP probe alone is not an application-health check.

The probe accepts domain names, IPv4 and bracketed IPv6 server URLs such as
`http://[::1]:8000` (loopback development) or `https://[2001:db8::1]:8443`
(address-format example). Since 1.30.8, IPv6 literals are resolved as IP addresses
instead of sending their URL brackets to DNS. Explicit ports and the URL scheme's
default port are preserved.

**O host configurado vira INTERNO** (`is_server_host`), logo recebe as pontes
nativas: Modo Code, notificação, badge, gate de versão. É decisão de confiança
deliberada, cercada por: `https` obrigatório fora de loopback, credencial na URL
recusada, comandos inalcançáveis por página remota, e um aviso em português na
tela. Ver [ADR-019](decisoes.md#adr-019--servidor-configurável-probe-no-rust-e-o-csp-que-destravou).

**Onde mexer:** [`src-tauri/src/server.rs`](../src-tauri/src/server.rs) (regra e
persistência), [`src/main.ts`](../src/main.ts) (estados da casca),
[`index.html`](../index.html) (formulário).

### Pointing the app at DEV (`dev.shvia.org`)

> **Historical environment snapshot, 2026-09-24.** The facts in the next three bullets were
> recorded for that DEV installation; this source review did not verify its current data,
> credentials, branch, integrations or workers. Confirm them with the operator before using it.

`https://dev.shvia.org` was recorded as a DEV copy of SHVIA-WEB on the production host, with a
database of its own:

- **Its data is a copy of production** (taken 24/09/2026), with every credential the app
  encrypts cleared: BYOK provider keys, OAuth, GitHub and bot tokens, and the CPF. Log in with
  your production account, and enter provider keys again there. On-prem inference (Ollama)
  works without one.
- **The integrations that act outside are off:** WhatsApp, IXC, Apple push, the shared memory,
  the workspace, Telegram alerts. Registration is closed, and no scheduler runs.
- **It runs `master`** unless someone put a branch there with `shvia-dev-atualizar <branch>` on
  the host. That is how a SHVIA-WEB change is tested with the desktop before production.

No DEV build is needed: the address is configuration, the mechanism above.

- **From the app.** The splash and the offline screen show the server's address with a
  **trocar** button. Type `https://dev.shvia.org`, then **Salvar e conectar**: the app probes it
  (4 s) and opens it. With production reachable the splash is short, so the offline screen is
  the easy moment to catch.
- **While editing.** Opening the server form suspends automatic navigation. Replies from
  earlier connection probes are ignored, including after saving a different server or
  canceling the form. Saving waits for the new address's own probe; canceling resumes
  connection checks for the current address.
- **By file**, with the app closed. Write the address to `server.json` in the app's config
  directory (identifier `cloud.blue3.shvia`):

  | OS | file |
  |---|---|
  | Linux | `~/.config/cloud.blue3.shvia/server.json` |
  | macOS | `~/Library/Application Support/cloud.blue3.shvia/server.json` |
  | Windows | `%APPDATA%\cloud.blue3.shvia\server.json` |

  ```json
  {"url": "https://dev.shvia.org"}
  ```

  A file the app cannot use (not JSON, not `https`, credentials in the URL) falls back to
  production, so a typo opens production, never a blank window.
- **Back to production:** **trocar** → **Voltar ao padrão**, or delete `server.json`.

What changes while DEV is configured: DEV becomes an internal host and gets the native bridges
(Code mode, notifications, badge), and `ai.shvia.org` stays internal too (`SERVER_HOSTS`). The
login is DEV's own, because the session cookie belongs to each origin. **Update checks follow
the configured server too:** `updater::endpoint` rebuilds the endpoint from `server::load()`.
The compiled `tauri.conf.json` endpoint is a default, not a promise that DEV receives production
updates. The selected server must serve compatible signed update artifacts.

---

## IPC / bridge

- The remote WebView uses the configured ShvIA origin. The custom JS/Rust bridge is injected
  into trusted internal hosts, with its own token and action checks; generic Tauri plugin
  capabilities remain unavailable to the remote page.
- Eventos da página → **notificação de SO + contagem no ícone** via
  `tauri-plugin-notification` e `set_badge_count`, ponteados pelo shim
  `NATIVE_NOTIFY_JS` (injetado em `on_page_load`), que faz *polling* de
  `GET /api/v1/notifications?unread=1` e posta `{action:'notify'}` /
  `{action:'badge'}` no **mesmo canal do Modo Code**. Nenhuma capability nova é
  exposta à origem remota (ADR-001).
  - A rota é **genérica**: todo evento do `DeliveryRouter` do servidor (alerta de
    preço, resultado de rotina, fim de lote, destino de entrega morto) chega sem
    precisar de um poll novo por feature.
  - **Clique→navegar não existe**, e não é dívida: o `desktop.rs` do plugin não tem
    callback de ação. **Windows não tem badge** (`set_badge_count` é `Unsupported`
    lá). Ver [ADR-017](decisoes.md#adr-017--notificação-badge-no-ícone-entra-cliquenavegar-não-é-possível-com-o-plugin).
- **Version gate** (`VERSION_GATE_JS`): reads `clients.desktop` from the authenticated
  `/api/v1/version`, falling back to the older `/api/v1/health` shape, and when this build is
  below the server's `min_version`, shows a dismissible banner with the reason and changelog. **Avisa, nunca
  bloqueia** — a casca é fina e o cliente velho quase sempre funciona. Dispensar é
  lembrado por versão DO SERVIDOR, então o aviso volta se o servidor subir pedindo
  outra coisa. Pré-requisito do auto-update (D1). Ver
  [ADR-018](decisoes.md#adr-018--gate-de-versão-clienteservidor-avisa-nunca-bloqueia).

### Where the bridge code lives (since 1.6.41)

The native side of `window.__shviaCode` was one 3,760-line file. It is split by subject; every
path used from outside (`code_bridge::handle_message`, `::reply`, `::Sidecars`, …) is unchanged.

| file (`src-tauri/src/`) | what it holds |
|---|---|
| `code_bridge.rs` | the dispatch (`handle_message`), `reply`, `fora_da_ui`, `saida_com_prazo` and the deadlines, `notify`/`badge` |
| `code_bridge/shim.rs` | the JS shim (`BRIDGE_JS`) and the session's capability token |
| `code_bridge/sessao.rs` | one engine process per session of a window (`Sidecars`, keyed by window + `sessao` since 1.8.0), `spawn`, `send`, ending it; what survives a reload since 1.8.2 (`Registro`: the numbered log, busy state, open requests; `soltar_janela`, `listar`, `anexar`, ADR-037) |
| `code_bridge/motores.rs` | which binary each engine is, finding it, catalogues, exit codes, installing the runner |
| `code_bridge/login.rs` | the Claude Code login driven from the screen, and its native confirmation |
| `code_bridge/assinatura.rs`, `code_bridge/assinatura/` | connecting a Claude subscription to the configured server; native consent, Unix PTY, bounded screen parser and private token delivery |
| `code_bridge/atualizacao.rs` | installing missing runners and updating installed runners from bundled sources |
| `code_bridge/cerca.rs` | the fence (ADR-031) and the project→folder bindings |
| `code_bridge/painel.rs` | the folder panel: `git status`, tree, diff, file read |
| `code_bridge/dialogos.rs` | the native pick/save dialogs |
| `contas_claude.rs` | local account registry, alias/function discovery, child environment and isolated usage cache |
| `code_bridge/aparelho.rs` | tray-enabled device info, notifications, native file picking and consented screen capture |
| `code_bridge/navegador.rs`, `code_bridge/navegador.js` | the agent's separate browser/profile, site and interaction confirmations |
| `code_bridge/pentest.rs`, `code_bridge/pentest/` | controlled assessments, runtime compatibility, consent, recovery, telemetry, review/retest, private sync and external viewer |

Each file's tests sit at its end. The source rulers read the file that holds what they check:
the dispatch rulers read `code_bridge.rs`, the login rulers read `login.rs`.

### Engine sessions and responsibility for background work

Local sessions are keyed by window and session name, with at most eight per window. Switching
projects can park an agent while the shared Web screen keeps serving its events. A session
spawned with `retomavel: true` survives a document reload: Rust buffers numbered events and the
next page calls `sessions()` and `attach(session, since)`.

- The log holds up to 8 MB per session; `attach` reports `lacuna` if earlier output was dropped.
- Without an attached page, a gate waiting or a turn ending produces a native notice after
  15 seconds. A working agent is not ended merely because its page is absent. An idle detached
  session ends after ten minutes; ended records remain for up to 30 minutes.
- Closing a window ends its local sessions; quitting ends all of them. Reload recovery does
  not imply survival of an app restart, a different computer or a closed window.
- The Web screen owns approval decisions, the queue, Run and transcript recording. Rust
  keeping an engine process alive does not supply a server worker for subsequent gates/turns.

Remote sessions live in the Workspace service. ShvIA Web's composed bridge can display them
in this same Code screen and transfer screen ownership between devices. A saved Code transcript
is a server-side reading record, not a local folder upload or proof of a live engine. The
[Web Remote Code contract](https://github.com/samirhvbr/shvia-web/blob/master/docs/FRONTEND/REMOTE-CODE.md)
owns this distinction.

### Device, browser and assessment boundaries

Device actions and the agent's browser have separate native tray switches, off until the person
enables them. Screen capture, file picking and browser interactions follow their specific native
consent rules; the agent's browser uses a separate profile and cannot call back into the app.
These are distinct from Code's approval level.

Controlled assessment uses the installed SHVIA Strix fork on Linux/macOS. The current probe
requires a plain fork version `1.2.39 <= version < 2.0.0`, using `--fork-version` before native
consent; Windows is refused. Start, resume and retest repeat the check. Private saved-run and
authorization identity stay native-owned. The viewer opens separately in the OS browser after
consent, and its capability URL is not returned to the Web page. See
[authorization](PENTEST/authorization.md), [compatibility](pentest-compatibility.md) and
[viewer lifecycle](pentest-viewer.md). Source checks do not replace the installed operator pilot.

---

## Build & empacotamento

> ⚠️ **Testes rodam via CI** (`.github/workflows/ci.yml`, desde 01–02/09/2026), mas o
> **build de release não.** A matriz de empacotamento/assinatura foi removida na **0.4.6**
> por custo, e segue **local por decisão** mesmo com a conta em GitHub Enterprise (50.000
> min/mês) — runners macOS contam minutos a 10x, o que ainda pesa numa matriz de release.
> Esta seção descrevia uma pipeline de GitHub Actions de release que não existe mais (matriz
> de runners, `tauri-action`, Azure Trusted Signing, `latest.json` em Releases) — corrigido
> na 0.16.0, junto do item D9. Trazer essa matriz de volta é decisão em aberto, não
> automática.

- **Onde roda:** `build-local.sh` (macOS/Linux) e `build-local.ps1` (Windows). **Cada
  SO é empacotado numa máquina diferente**, e essa é a restrição que molda o resto.
- **Versão:** `version.md` é a fonte única; o `scripts/sync-version.mjs` a propaga
  para `package.json`, `tauri.conf.json`, `Cargo.toml` e os locks (roda no `prebuild`).
- **Targets:** macOS (`.dmg` + `.app.tar.gz`), Windows (`.msi` WiX + `-setup.exe`
  NSIS), Linux (`.AppImage` + `.deb` + `.rpm`).
- **Motor empacotado** (item D5): o `anna` viaja no instalador como `externalBin`
  (sidecar), preparado por `scripts/stage-anna.mjs` antes do `tauri build`. Era o
  **gargalo de adoção** do Modo Code — o binário era pré-requisito externo. O
  empacotado **vence o do PATH** (é o testado com esta versão do app), e o
  `engineStatus` da ponte responde `found`/`bundled`/`version`/`path`. Ver
  [ADR-021](decisoes.md#adr-021--o-anna-viaja-no-instalador-externalbin-e-o-empacotado-vence-o-do-path).
- **PATH do sidecar** (`src/user_env.rs`, 1.1.24): app de GUI não herda PATH de
  shell — no macOS o launchd entrega o mínimo do sistema, e as ferramentas que o
  agente roda com `sh -c` não encontram `npx`/`node`/`cargo`/`php`. O PATH vem do
  `$SHELL -ilc` (uma vez por processo, com watchdog e validação) e é aplicado no
  `spawn` e na sonda `command -v`. Ver
  [ADR-030](decisoes.md#adr-030--o-sidecar-recebe-o-path-do-shell-de-login-não-o-do-launchd).
- **Assinatura:**
  - **macOS** — Developer ID, `notarytool` and `stapler`; the App Store Connect API key is
    preferred for notarization, with the historical keychain password as fallback. See [build.md](build.md).
  - **Windows** (item D9) — `signtool` no `.msi` **e** no `-setup.exe`, com
    timestamp RFC3161 (`/tr`). Sem `/tr` a assinatura expira junto com o
    certificado. Credencial por env: `SHVIA_WIN_CERT_THUMBPRINT` (preferido — a
    chave privada não vira arquivo) ou `SHVIA_WIN_PFX` + senha.
  - **Linux** — sem assinatura hoje. O `.AppImage` e o `.deb` saem crus.
  - A local unsigned build and a publishable artifact are different results. The publication
    script checks signing, updater keys and the manifest; an unsigned macOS build is refused
    for publication. See [build.md](build.md) for the exact per-platform checks.
- **Checksums e manifesto** (item D9, `scripts/release-manifest.mjs`): um `.sha256`
  ao lado de cada instalador e o **`release.json`** na raiz, que é o manifesto que o
  update server reads. Ele **mescla** por plataforma — build por máquina
  significa que sobrescrever apagaria a entrada dos outros SOs — e **avisa quais
  faltam**. O hash é calculado **depois** de assinar, porque assinar altera os bytes.
  `release.json` é **gitignorado**: artefato de build, não fonte. Ver
  [ADR-020](decisoes.md#adr-020--checksums-e-releasejson-saem-do-build-local-assinatura-no-windows).
- **Auto-update exists** (`updater.rs`, ADR-022): it checks the configured server, asks before
  installation/restart and verifies the updater signature. Automatic checks are quiet on
  failure; manual checks respond with their outcome. Package-manager and bundle limitations
  have specific instructions (including Arch/pacman). Publishing a GitHub Release alone does
  not publish the update server's manifest or installers.

---

## Client/server compatibility

The server declares the minimum **Desktop** build in `clients.desktop.min_version`; the app
compares its own build and warns without blocking. It does not enforce a general minimum
server release. The shared Web UI checks individual bridge methods and `recursos` flags,
because the page, installed shell and runners update independently. A capability flag says
the shell implements a path; it does not prove the external client is installed or logged in.

The authenticated `/api/v1/version` is the primary source. `/api/v1/health` is a fallback
for older servers and may require a health token on an installation; a failed read must not
be reported as a confirmed compatible server.

---

## Local persistence

| Data | Location / owner | Boundary |
|---|---|---|
| Configured server | App config `server.json` (`server.rs`) | Per computer; invalid input falls back to the built-in server |
| Window size/position | `tauri-plugin-window-state` | Quick window is excluded from normal window-state restore |
| Quick-window shortcut and dismissed update | App config `janela-rapida.json`, `updater.json` | Native preferences, not account settings on the server |
| Web session cookie, panel token and cached assets | WebView origin storage | Web authentication; not a native keychain token vault |
| Authorized folder roots and project bindings | App config `pastas-autorizadas.json`, `modo-code-bindings.json` | A server project ID maps to a path on this computer only |
| Claude profile registry | App config `contas-claude.json` | IDs, labels, directories and variable names; no provider credential in this registry |
| Official-client credentials/history | Claude/Codex client's own storage | Local engine authentication and context; ownership remains with those clients |
| Credential-profile usage cache | App cache `uso-claude/<account id>` | Isolates the usage read; it does not move the person's configuration home |
| Live local sessions and replay log | Rust process memory | Reload continuity within a window, not app-restart persistence |
| Page snapshot, queue and unsent Code transcript | Web `sessionStorage` / `localStorage`, by their contracts | Client state/outboxes; saved transcript blocks are also stored on ShvIA Web |
| Agent-browser profile | App data `navegador-do-agente` outside macOS; dedicated WebKit data-store ID on macOS | Separate site cookies/logins, not the ShvIA window's profile |
| Controlled assessment | Private app config `pentest/` | Grants, recovery descriptor, runs/artifacts and outgoing snapshots; upload requires its native consent/receipt contract |

No relational database runs in this client. That does not mean it is stateless: local files,
client credentials, browser state and private assessment artifacts have their own lifecycles.
The Workspace's remote filesystem is a separate service; it is not a synchronized copy of every
authorized folder on this computer.
