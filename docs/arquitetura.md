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
│  • tray / menu nativo                        • tela offline + auto-retry    │
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
- **Segredos** (chaves de assinatura/updater) **nunca** no repo — secrets de CI.

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

---

## Build & empacotamento

- **Versão:** `tauri.conf.json` recebe o valor de `version.md` na CI (fonte única,
  como o `config/app.php` do ShvIA lê `version.md`).
- **Targets:** macOS (`.dmg`, universal arm64+x64), Windows (`.msi` WiX + NSIS),
  Linux (`.AppImage` + `.deb`, `.rpm` opcional).
- **Assinatura:** macOS Developer ID + `notarytool` + `stapler`; Windows
  Authenticode **EV** (Azure Trusted Signing); Linux GPG.
- **Auto-update:** Tauri updater com `latest.json` assinado em GitHub Releases
  (fluxo colhido do SHVTERM).
- **CI:** GitHub Actions, matriz `macos`/`windows`/`ubuntu` (`tauri-action`).
  Sem serviço de banco (o cliente não tem DB).

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
