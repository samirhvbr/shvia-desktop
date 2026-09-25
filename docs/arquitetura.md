# Arquitetura — ShvIA Desktop

Shell fino em Tauri 2 que carrega o ShvIA web (Blade) hospedado; o servidor
Laravel é a fonte da verdade. Decisões e o porquê em [decisoes.md](decisoes.md).

---

## Camadas

```
┌─────────────────────────── App Desktop (Tauri 2) ──────────────────────────┐
│                                                                            │
│  src-tauri/ (Rust)                          src/ (casca web mínima, Vite)  │
│  ─────────────────                          ─────────────────────────────  │
│  • criação da janela + branding             • bootstrap/splash             │
│  • tray/menubar + autostart ✅               • tela offline + auto-retry    │
│  • deep-link  shvia://                        • config de URL (1º run)      │
│  • updater (latest.json)                                                    │
│  • store (URL servidor + estado janela)                                    │
│  • notifications de SO                                                      │
│  • single-instance                                                         │
│                                                                            │
│            WebView nativo (WKWebView / WebView2 / WebKitGTK)                │
│            ───────────────────────────────────────────────                 │
│            navega ──► https://ai.shvia.org  (UI Blade do ShvIA)         │
│                                                                            │
│  [F2] sidecar/ (Python, PyInstaller) — opcional:                           │
│        keychain de token · SSO handoff · chamadas a /api/v1                 │
└────────────────────────────────────────────────────────────────────────────┘
                         │ HTTPS (cookie de sessão; SSE em /chat)
                         ▼
        Servidor ShvIA (Laravel 13 + MariaDB + Postgres RAG) — FONTE DA VERDADE
```

---

## Fluxo de autenticação (F1)

1. App abre → WebView navega o servidor configurado (padrão `https://ai.shvia.org`
   — ver [Endereço do servidor](#endereço-do-servidor-item-d4)).
2. Usuário cai na **tela de login Breeze** do próprio ShvIA.
3. Login → ShvIA seta **cookie de sessão Sanctum** (guard `web`) para o FQDN.
4. Toda navegação/requisição subsequente vai **autenticada** (como num browser).
5. **Persistência:** o WebView guarda o cookie; validar que sobrevive ao
   **restart** do app (incl. "remember me"/lifetime de sessão).

**Bearer token (`/api/v1`)** não autentica rotas Blade — só serve para **ações
nativas** que falem com a API (F2+, via sidecar + keychain).

**Deep-link SSO `shvia://`** (F2) é a **única origem não-FQDN**; exige
`SANCTUM_STATEFUL_DOMAINS` / handoff de cookie. Tratar explicitamente.

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
- **Segredos** (chaves de assinatura/updater) **nunca** no repo. Enquanto o build de
  release for local por decisão (ver `docs/build.md`), não há "secrets de CI" onde
  guardá-los para esse fluxo: no macOS a senha de notarização vive no **keychain** da
  máquina; no Windows o certificado vive no **repositório de certificados do SO** (ou
  um `.pfx` fora da árvore do repo), apontado por variável de ambiente da sessão.

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

**Alcance ≠ identidade:** o probe abre um TCP e fecha. Não valida certificado nem
confere que é um ShvIA — escopo declarado, entra com o cliente HTTP do D1.

**O host configurado vira INTERNO** (`is_server_host`), logo recebe as pontes
nativas: Modo Code, notificação, badge, gate de versão. É decisão de confiança
deliberada, cercada por: `https` obrigatório fora de loopback, credencial na URL
recusada, comandos inalcançáveis por página remota, e um aviso em português na
tela. Ver [ADR-019](decisoes.md#adr-019--servidor-configurável-probe-no-rust-e-o-csp-que-destravou).

**Onde mexer:** [`src-tauri/src/server.rs`](../src-tauri/src/server.rs) (regra e
persistência), [`src/main.ts`](../src/main.ts) (estados da casca),
[`index.html`](../index.html) (formulário).

### Pointing the app at DEV (`dev.shvia.org`)

`https://dev.shvia.org` has been live since 24/09/2026. It is a DEV copy of SHVIA-WEB on the
production host, with a database of its own. Before testing against it:

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
login is DEV's own, because the session cookie belongs to each origin. **Updates keep coming
from production**, because the updater's endpoint is compiled in (`tauri.conf.json`,
`plugins.updater.endpoints`).

---

## IPC / bridge

- O WebView remoto (Blade) roda no FQDN do ShvIA; o bridge JS↔Rust fica
  disponível só onde habilitado por capability.
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
- **Gate de versão** (`VERSION_GATE_JS`): lê `version.clients.desktop` do
  `/api/v1/health` e, se este build está abaixo do `min_version` que o servidor
  declara, mostra tarja dispensável com o motivo e o changelog. **Avisa, nunca
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
| `code_bridge/sessao.rs` | one engine process per session of a window (`Sidecars`, keyed by window + `sessao` since 1.8.0), `spawn`, `send`, ending it |
| `code_bridge/motores.rs` | which binary each engine is, finding it, catalogues, exit codes, installing the runner |
| `code_bridge/login.rs` | the Claude Code login driven from the screen, and its native confirmation |
| `code_bridge/cerca.rs` | the fence (ADR-031) and the project→folder bindings |
| `code_bridge/painel.rs` | the folder panel: `git status`, tree, diff, file read |
| `code_bridge/dialogos.rs` | the native pick/save dialogs |

Each file's tests sit at its end. The source rulers read the file that holds what they check:
the dispatch rulers read `code_bridge.rs`, the login rulers read `login.rs`.

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
  - **macOS** — Developer ID + `notarytool` + `stapler`, com a senha de app no
    **keychain** (serviço `shvia-notarize`), nunca no repo.
  - **Windows** (item D9) — `signtool` no `.msi` **e** no `-setup.exe`, com
    timestamp RFC3161 (`/tr`). Sem `/tr` a assinatura expira junto com o
    certificado. Credencial por env: `SHVIA_WIN_CERT_THUMBPRINT` (preferido — a
    chave privada não vira arquivo) ou `SHVIA_WIN_PFX` + senha.
  - **Linux** — sem assinatura hoje. O `.AppImage` e o `.deb` saem crus.
  - Em qualquer SO, **sem credencial o build segue e avisa** — mas avisa em amarelo,
    porque build não assinado que parece normal é o que faz alguém publicar e
    descobrir pelo relato do usuário.
- **Checksums e manifesto** (item D9, `scripts/release-manifest.mjs`): um `.sha256`
  ao lado de cada instalador e o **`release.json`** na raiz, que é o manifesto que o
  auto-update (D1) vai ler. Ele **mescla** por plataforma — build por máquina
  significa que sobrescrever apagaria a entrada dos outros SOs — e **avisa quais
  faltam**. O hash é calculado **depois** de assinar, porque assinar altera os bytes.
  `release.json` é **gitignorado**: artefato de build, não fonte. Ver
  [ADR-020](decisoes.md#adr-020--checksums-e-releasejson-saem-do-build-local-assinatura-no-windows).
- **Auto-update:** **ainda não existe** — é o item **D1**, e o `release.json` acima é
  o pré-requisito dele.

---

## Compatibilidade de versão de servidor (F3)

No launch, o app lê o campo `version` de `GET /api/v1/health` e compara com a
**versão mínima de servidor** que ele exige. Divergência → aviso "atualize o
servidor / atualize o app". Isso desacopla as cadências de release do desktop e
do Laravel.

---

## Persistência local (o que mora no cliente)

| Dado | Onde | Por quê |
|------|------|---------|
| URL do servidor | `tauri-plugin-store` (JSON) | Configurável no 1º run. |
| Estado da janela (tamanho/posição) | `tauri-plugin-store` | UX. |
| Cookie de sessão | partição do WebView | Auth (ADR-005). |
| Cache de assets | WebView | Performance. |
| Token Bearer (F2, opcional) | **keychain do SO** | Ações nativas na API. |

**Nada relacional. Nenhum SQLite. Nenhum segredo do servidor.**
