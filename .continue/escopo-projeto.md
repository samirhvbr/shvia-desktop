# ShvIA Desktop — Escopo do Projeto

> Documento de escopo detalhado para iniciar a implementação. Base: análise
> multi-agente de 30/06/2026 + recon direto dos repos ShvIA e SHVTERM.
> Companheiros: [estado-atual.md](estado-atual.md) ·
> [../docs/decisoes.md](../docs/decisoes.md) ·
> [../docs/arquitetura.md](../docs/arquitetura.md) ·
> [../docs/roteiro-fundacao.md](../docs/roteiro-fundacao.md).

---

## 1. Objetivo

Entregar um **app desktop multiplataforma (macOS, Windows, Linux)** com **a cara
do ShvIA** e as **mesmas funções** do web app, empacotado como aplicativo nativo
(janela, ícone, tray, notificações, auto-update), reaproveitando ao máximo o que
já existe (SHVTERM para a casca Tauri; ShvIA hospedado para a aplicação).

**Não-objetivos (F1):** reescrever o backend Laravel; reescrever a UI em React;
operar offline; rodar qualquer banco no cliente.

---

## 2. Arquitetura (resumo executável)

**Shell fino em Tauri 2 carregando o ShvIA web (Blade) remoto.**

```
┌──────────────────────────── App Desktop (Tauri 2) ───────────────────────────┐
│  Núcleo Rust (src-tauri)                                                      │
│   • janela + branding ShvIA   • tray/menu   • deep-link shvia://              │
│   • updater (latest.json)     • store (URL+estado janela)  • notifications    │
│                                                                              │
│  WebView nativo do SO (WKWebView / WebView2 / WebKitGTK)                      │
│   └── navega  ──────────────►  https://ai.shvia.org  (ShvIA Blade)         │
│        (login Breeze → cookie de sessão Sanctum, same-origin)                 │
│                                                                              │
│  [F2] Sidecar Python (PyInstaller)  ── opcional ──► keychain / SSO / /api/v1  │
└──────────────────────────────────────────────────────────────────────────────┘
                                   │  HTTPS (cookie de sessão; SSE no /chat)
                                   ▼
                    Servidor ShvIA (Laravel + MariaDB) — FONTE DA VERDADE
                    chat · compare · workspaces · KB · skills · admin · usage
```

Detalhe técnico completo (fluxo de auth, CSP, IPC, build) em
[../docs/arquitetura.md](../docs/arquitetura.md).

### Decisões-chave e por quê

- **Tauri 2, não Electron:** o SHVTERM já é Tauri e pronto; economiza ~120 MB de
  Chromium por build. Electron fica como **fallback** se o streaming SSE quebrar
  no WebKitGTK (Linux).
- **Não NativePHP:** o ganho dele é SQLite local; ShvIA **exige MariaDB/MySQL e
  proíbe SQLite**. Embutir MySQL em cada laptop forkaria a camada de dados.
- **Servidor remoto = fonte da verdade:** o desktop **não abre banco**. A regra
  "nunca SQLite" é satisfeita por construção. `SESSION/QUEUE/CACHE` continuam
  server-side. Persistência local = só URL do servidor + estado da janela (store
  JSON do Tauri) + cache do WebView.
- **Auth = sessão Sanctum (cookie), same-origin:** navegando o FQDN real, o login
  é a tela normal do ShvIA e o cookie autentica tudo. **Bearer só serve
  `/api/v1`** — não autentica as rotas Blade. Logo, **F1 não precisa de plumbing
  de token**. O único caso que reintroduz origem não-FQDN é o **deep-link SSO
  `shvia://`** → tratar na F2 (`SANCTUM_STATEFUL_DOMAINS`/handoff de cookie).

---

## 3. Funções do ShvIA que o desktop entrega (de graça, via Blade)

Mapa extraído de `routes/web.php` e `routes/api.php` do ShvIA:

| Área | Onde (web) | Observações p/ o desktop |
|------|-----------|--------------------------|
| **Chat** | `/chat` (`DashboardController`) | Inferência via **SSE** (`POST /api/v1/chat`). **Risco #1 no WebKitGTK** — testar primeiro. |
| **Compare** | `/compare` (`CompareController`) | Comparação de modelos + voto. |
| **Workspaces / Pastas / Arquivos** | API `/api/v1/folders*`, `/workspaces*` | Upload de arquivos (RAG), citations, busca global. |
| **Base de Conhecimento (KB)** | `/admin/kb*` | Dashboard/entries/detalhe (fonte do RAG). |
| **Skills** | `/admin/skills`, API `/skills` | Sistema de skills. |
| **Perfis / Model-roles / Inference** | `/admin/profiles`, `/admin/model-roles`, `/admin/inference` | Config de modelos/perfis (admin). |
| **Uso / tokens** | `/admin/audit`, `/admin/users/{u}/tracking`, API `/usage` | Rastreamento de uso e tokens. |
| **Admin** | `/admin/users`, `/admin/logs`, `/admin/audit` | Gestão e auditoria. |
| **Conta / Perfil** | `/profile`, API `/auth/*` | Sanctum (web=sessão; API=token+apikey). |
| **Outros** | `transcribe`, `embeddings`, `agent/chat` | Áudio→texto, embeddings, agente. |

> Como a F1 carrega o Blade remoto, **todas** essas telas funcionam sem código
> novo. A camada nativa só adiciona janela/tray/deep-link/notificações/updater.

---

## 4. O que reaproveitar do SHVTERM (colher, sem merge)

- **Matriz CI Tauri** (`macos`/`windows`/`ubuntu` via `tauri-action`) — copiar e
  re-targetar.
- **Fluxo do updater Tauri** (chave `TAURI_SIGNING_PRIVATE_KEY`, custódia de
  secret, `latest.json` em GitHub Releases).
- **Scripts de packaging Linux** (`scripts/packaging/{appimage,deb,rpm}`),
  `metainfo.xml` (AppStream).
- **Padrões de UX nativa provados**: tray, single-instance, autostart, config em
  `~/.config`, `tauri-plugin-store`.
- **Padrão sidecar** (handshake `{port, token}`, canal WS de controle) — **se** e
  quando a F2 precisar de serviços seguros locais.
- **Convenções de casa**: `.continue/estado-atual.md`, perfil `.claude` opus-only,
  CLAUDE.md/AGENTS.md espelhados, branch `master`, autor `Samir Hanna Verza`.

**Não reaproveitar:** nada de marca Claude/Anthropic, nem empacotamento do app
Electron da Anthropic.

---

## 5. Plano em fases (cada fase entregável)

### F0 — Decisões + colheita (½ semana)
- Travar a arquitetura (este doc).
- Criar `version.md` (0.1.0) e regra de commit pt-BR. ✅ (feito hoje)
- Colher do SHVTERM: workflows de CI, fluxo do updater, scripts de packaging,
  ícones de referência. Copiar `brand/` do ShvIA (`/Users/samir/x/IA/brand/`).
- **Iniciar procurement do cert EV Windows** (long pole de prazo).

### F1 — Esqueleto andante (1 semana) — "começar AGORA"
1. **SMOKE-TEST #1 (crítico, antes de tudo):** abrir `/chat` no **Linux/WebKitGTK**
   e validar o streaming `fetch` + `ReadableStream.getReader()`. Maior risco
   multiplataforma, concentrado no Linux.
2. `npm create tauri-app@latest` (Tauri 2); **uma** WebView apontando para URL de
   servidor configurável (default = `https://ai.shvia.org`).
3. Validar **login Breeze → sessão Sanctum** no WebView e a **persistência do
   cookie entre reinícios** do app (incl. "remember me"/lifetime).
4. Título + ícone ShvIA. **Isto já entrega "mesmas funções garantidas".**

### F2 — Polish nativo + branding + SSO (1 semana)
- Ícone/menu/tray/About; estado de janela persistido; **config de URL no primeiro
  run** (`tauri-plugin-store`); **tela offline** com auto-retry.
- **Deep-link `shvia://`** + reconciliar com `SANCTUM_STATEFUL_DOMAINS` (callback
  SSO — única origem não-FQDN).
- Notificações de SO disparadas de eventos da página ("resposta pronta").
- **(Decisão)** Entrar com o **sidecar Python** se precisarmos de vault de token
  no keychain / ações nativas na `/api/v1`.

### F3 — Compatibilidade de versão de servidor (½ semana)
- No launch, ler `version` de `GET /api/v1/health` e comparar com o mínimo exigido
  pelo app; aviso "atualize o servidor / atualize o app" na divergência.

### F4 — CI + assinatura + auto-update (1½ semana)
- Matriz GitHub Actions (mac universal, win, linux).
- **macOS**: `codesign` (Developer ID) + hardened runtime → `notarytool` →
  `stapler staple`.
- **Windows**: `.msi` (WiX) + NSIS, Authenticode **EV** (Azure Trusted Signing
  preferível a `.pfx`; EV evita SmartScreen).
- **Linux**: AppImage + `.deb` (`.rpm` opcional), GPG-sign.
- Auto-update Tauri (`latest.json` assinado); release por tag = `version.md`.

### F5 — Beta + docs (1 semana)
- Beta interno nos 3 SOs; corrigir quirks de WebView (bootstrap evergreen do
  WebView2 no Windows; deps WebKitGTK no Linux); validar SmartScreen/Gatekeeper;
  docs de install/release.

**Esforço total:** ~5–6 semanas-engenheiro para 1.0 assinado/notarizado/auto-update
nos 3 SOs (1 dev confortável em Rust/Tauri + CI).

---

## 6. Riscos e custos

### Top riscos + mitigação
1. **Streaming SSE no WebKitGTK/Linux quebrar** (ponto fraco histórico do
   WebKitGTK). → **Smoke-test na F1, antes de tudo.** Fallback: Electron
   (Chromium pinado), ainda thin-shell hospedado — **nunca** NativePHP.
2. **Procurement do cert EV Windows estourar o cronograma** (dias–semanas de
   onboarding). → Iniciar **no dia 1 da F0**, em paralelo; até lá, build interno
   OV/não-assinado.
3. **Deep-link SSO `shvia://`** reintroduz origem não-FQDN que o cookie stateful
   não cobre. → Item explícito da F2 (`SANCTUM_STATEFUL_DOMAINS` ou troca
   token→sessão curta). Não assumir "não precisa de nada".

### Custos
| Item | Custo |
|------|-------|
| Tauri | **MIT, $0** (sem licença NativePHP, sem Chromium) |
| Apple Developer Program | ~**US$99/ano** |
| Authenticode **EV** Windows / Azure Trusted Signing | ~**US$300–600/ano** |
| Linux (GPG self-managed) | **$0** |

---

## 7. Decisões em aberto

> Confirmar com o time antes de avançar além da F1.

1. **Online-only é aceitável** como propriedade assinada do produto? Toda a
   arquitetura fina depende disso. Requisito futuro de **offline genuíno** ou
   **air-gapped** → mata esta arquitetura (cai em NativePHP+MySQL embutido,
   footprint absurdo, ou reescrita — outro projeto).
2. **Verba + dono** do cert EV Windows e do Apple Developer, incl. **rotação da
   chave do updater** (responsabilidade perpétua). Ou descopar para distribuição
   interna não-assinada (com avisos SmartScreen/Gatekeeper — prejudica "a cara do
   projeto IA").
3. **Funções idênticas ao web** ou haverá **telas desktop-only**? Idênticas →
   thin-shell perfeito. Divergir → exige cliente SPA real sobre `/api/v1` (mais
   código/manutenção, "modo B").
4. **Sidecar Python na F1?** Análise indica **não necessário** na F1 (auth é
   cookie same-origin). Confirmar: entra só na F2 (vault/SSO/ações nativas) ou há
   razão pra antecipar?
5. **App ID e URL de DEV:** confirmar `cloud.blue3.shvia` (sugerido) e o domínio
   de DEV do ShvIA para testes.

---

## 8. Critérios de pronto (Definition of Done)

- **F1 pronto quando:** o app abre, faz login no ShvIA, o `/chat` **transmite
  (SSE) nos 3 SOs**, a sessão **persiste** entre reinícios, e a janela tem
  título/ícone ShvIA.
- **1.0 pronto quando:** instaladores **assinados/notarizados** nos 3 SOs,
  **auto-update** funcionando, tela offline, deep-link `shvia://`, e check de
  versão de servidor — validados em beta interno.
