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

1. App abre → WebView navega `https://ai.shvia.org`.
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
  'self'` + `connect-src` liberando o FQDN do ShvIA, `ipc:`/`http://ipc.localhost`
  para o bridge Tauri. Modelada do CSP do SHVTERM (`gui/src-tauri/tauri.conf.json`).
  **Importante:** esse CSP governa **só a casca local** (splash/offline). Quando o
  WebView **navega para o FQDN**, a página passa a valer sob o **CSP do próprio
  servidor ShvIA** (cabeçalhos HTTP do Laravel) — o CSP do app **não** restringe
  nem protege a página remota.
- **Capabilities por janela** (`src-tauri/capabilities/`): expor ao WebView só os
  comandos/plugins necessários (store, notification, updater, deep-link). Postura
  de menor privilégio.
- **Segredos** (chaves de assinatura/updater) **nunca** no repo — secrets de CI.

---

## IPC / bridge

- O WebView remoto (Blade) roda no FQDN do ShvIA; o bridge JS↔Rust fica
  disponível só onde habilitado por capability.
- Eventos da página → notificações de SO (ex.: "resposta pronta") via
  `tauri-plugin-notification`, ponteados por um shim mínimo (a definir na F2 —
  pode usar `eval`/inject controlado ou um observer no DOM da página).

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
