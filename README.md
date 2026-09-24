# ShvIA Desktop (`shvia-desktop`)

> **Blue3 internal project.** **Cross-platform desktop client** (macOS,
> Windows, Linux) for **ShvIA** — Blue3's internal AI platform
> (`https://ai.shvia.org`). Internal documentation — do not publish.

**See also:** [CLAUDE.md](CLAUDE.md) / [AGENTS.md](AGENTS.md) (code and agent
conventions) · [docs/README.md](docs/README.md) (technical documentation
index) · [.continue/escopo-projeto.md](.continue/escopo-projeto.md) (detailed
scope and phase plan) · [docs/decisoes.md](docs/decisoes.md) (ADRs).

---

## Table of Contents

1. [What it is](#what-it-is)
2. [Architecture decision](#architecture-decision)
3. [Stack](#stack)
4. [Organizational model](#organizational-model)
5. [Repository structure (target)](#repository-structure-target)
6. [Version (`version.md`)](#version-versionmd)
7. [Relationship with ShvIA and SHVTERM](#relationship-with-shvia-and-shvterm)
8. [Phased roadmap](#phased-roadmap)
9. [Current status](#current-status)

---

## What it is

**ShvIA Desktop** is a desktop app that delivers ShvIA with **the project's own look**
and the **same features** as the web app — packaged as a native application for
macOS, Windows and Linux, with its own window, icon, tray, OS notifications
and auto-update.

ShvIA itself **remains the hosted Laravel server** at
`https://ai.shvia.org`: AI chat (SSE streaming), model comparison,
workspaces/folders with files (RAG), knowledge base, skills, admin panel
and usage/token tracking. The desktop is the **client** of that instance — it does not
rewrite the backend or the frontend.

---

## Architecture decision

> Decision made on **2026-06-30**, after a multi-agent analysis (see
> [docs/decisoes.md](docs/decisoes.md) for the full ADRs).

**A thin Tauri 2 shell loading the remote ShvIA web (Blade).**

- **Base = SHVTERM** (`/Users/samir/Projetos/SHVTERM`), our **Tauri 2 + React**
  desktop client that is already cross-platform, with CI for the 3 OSes, an updater and
  packaging patterns ready to use. **The Claude Desktop fork was discarded** (archived in
  `archive/claude-fork` + tag `archive/claude-fork-v0.2.2`).
- **The Tauri window opens the hosted ShvIA** (`https://ai.shvia.org`). This way
  **"same features" is literal** — it is ShvIA's own Blade UI. Zero forked
  Laravel code, zero UI rewritten in Phase 1.
- **Remote server = source of truth** (data, passwords, permissions). The desktop
  **does not open any local database** — the "MariaDB/MySQL, never SQLite" rule is
  satisfied by construction (there is no DB on the client).
- **Auth = Sanctum session (cookie), same-origin.** Since we navigate the real FQDN, the
  login is the normal ShvIA screen and the session cookie authenticates everything, as in a
  browser. (A Bearer token only serves `/api/v1`; the `shvia://` SSO deep-link is the only
  case that requires extra handling — Phase 2.)
- **Thin native layer** (Rust/Tauri): window with ShvIA branding, tray, `shvia://`
  deep-link, OS notifications, auto-update and config/window persistence.

**Why not Electron, not NativePHP:**

| Alternative | Why it was discarded |
|-------------|--------------------|
| **Electron** (the fork) | Embedded Chromium (~120 MB/build) with no gain here; SHVTERM's Tauri is already ready. Kept only as a **fallback** if SSE streaming breaks on WebKitGTK (Linux). |
| **NativePHP** (local Laravel) | Its gain is local SQLite; ShvIA **requires MariaDB/MySQL and forbids SQLite**. Forcing MySQL on every laptop would fork the data layer — the opposite of reuse. |

**Signed trade-off:** the architecture is **online-first / effectively
online-only**. Acceptable for an AI chat app (inference is server-side
anyway), addressed with an **offline screen** carrying the ShvIA brand + retry.
Genuinely offline operation is a *killer* of this architecture. **Confirmed by the
owner on 23/09/2026** ("acceptable — document it"): no offline mode is planned. What
works without the server is what runs on the machine (the engines, the local shell).

---

## Stack

- **Tauri 2** (Rust core) + **the OS's native WebView** (WKWebView on macOS,
  WebView2 on Windows, WebKitGTK on Linux).
- **Shell frontend**: minimal (Vite/TS) — bootstrap/offline screen and URL
  configuration. The main UI is ShvIA's **remote Blade**.
- **Python sidecar** (PyInstaller) — pattern inherited from SHVTERM; **optional in
  F1**, structural in F2 (token vault in the keychain, SSO, native API actions).
- **Tauri plugins**: `updater`, `process`, `store`, `notification`, `deep-link`,
  `single-instance`.
- **Three runtimes** (like SHVTERM): `npm` (shell) · `cargo` (Rust) · `pip`
  (sidecar).
- **CI**: GitHub Actions runs tests/lint (`ci.yml`, since 2026-09). Release
  packaging/signing is still local by decision (`build-local.*`) — the
  `macos`/`windows`/`ubuntu` matrix via `tauri-action` was removed in 0.4.6 for
  cost and hasn't been brought back, since macOS runners bill at 10x even under
  the current GitHub Enterprise plan.

---

## Organizational model

- **Reused repository** — this same repo (`samirhvbr/SHVIA-DESKTOP`,
  `master` branch). The name fits: **SHVIA-DESKTOP = the ShvIA desktop**. The
  fork history was preserved in `archive/claude-fork` (+ tag) and
  pushed to `origin`.
- **Separate from the Laravel repo.** ShvIA (Laravel) remains **untouched** in its
  own repo. This is not a monorepo: the desktop is a client of an already-hosted server
  shared with the web app; merging them would only couple release cadences.
- **SHVTERM** remains a **sibling** repo (SSH client) — from it we **harvest
  assets** (CI, updater, packaging, conventions), without merging.
- **Branding:** ShvIA/Blue3 identity (icons/splash from `brand/`, coming from
  `/Users/samir/x/IA/brand/`). Suggested App ID: `cloud.blue3.shvia` (aligned with
  `cloud.blue3.shvterm`). **No** Claude/Anthropic branding in any artifact.
- **Commits:** `version - comment in Portuguese` (bump `version.md` in the same
  commit). No `feat:/fix:/chore:`.

---

## Repository structure (target)

> Target layout for Phase 1. The skeleton (`src/`, `src-tauri/`, `scripts/` and the
> configs) **already exists** since `0.2.0`; `sidecar/` and `.github/workflows/` come in
> the following phases. Detail and step-by-step in
> [docs/roteiro-fundacao.md](docs/roteiro-fundacao.md).

```
shvia-desktop/
├── version.md                  # single source X.Y.Z (0.1.0)
├── README.md                   # this file
├── CLAUDE.md / AGENTS.md       # agent conventions (mirrored)
├── brand/                      # ShvIA icons/splash (copy from IA/brand/)
├── src/                        # minimal web shell (bootstrap, offline screen, URL config)
├── src-tauri/
│   ├── tauri.conf.json         # version generated from version.md in CI
│   ├── src/                    # Rust: window, tray, deep-link, notifications, updater, store
│   ├── capabilities/           # per-window allowlist (security posture)
│   └── icons/
├── sidecar/                    # (F2) native/secure services in Python
├── scripts/packaging/          # appimage/deb/rpm adapted from SHVTERM
├── .github/workflows/          # ci.yml (tests/lint); release matrix still local
├── .claude/                    # agent profile: permissions + effort (no model)
├── .continue/                  # WIP: current-state + project scope
└── docs/                       # stable technical documentation
```

---

## Version (`version.md`)

`version.md` holds the version of the **desktop app** (its own line, independent of
the ShvIA server version), in the `X.Y.Z` format:

- **X** — stable version (manual).
- **Y** — new runtime capability, IPC redesign, auth-handoff change.
- **Z** — increment: visible UI/menu/window change, new packaging
  capability, build adjustment.

**Coupling with the server:** each release records the **minimum compatible ShvIA
server version**, checked at runtime by reading the `version` field of
`GET /api/v1/health`. In CI, `tauri.conf.json` receives the version from `version.md`
(single source). Bump **in the same commit** as the change.

---

## Relationship with ShvIA and SHVTERM

| Repo | Role here |
|------|------------|
| **ShvIA** (`/Users/samir/x/IA`, Laravel) | **Server/source of truth.** The desktop loads the Blade and consumes `/api/v1`. Untouched. |
| **SHVTERM** (`/Users/samir/Projetos/SHVTERM`, Tauri) | **Technical base.** We harvest CI, updater, packaging, sidecar and conventions. Sibling repo, no merge. |
| **archive/claude-fork** (in this repo) | Snapshot of the discarded Claude Desktop fork. History preserved. |

---

## Phased roadmap

Summary (detail in [.continue/escopo-projeto.md](.continue/escopo-projeto.md)):

| Phase | Deliverable |
|------|---------|
| **F0** | Decisions + harvesting SHVTERM assets + ShvIA branding. |
| **F1** | Walking skeleton: Tauri opens `ai.shvia.org`; **SSE streaming smoke-test on Linux** (risk #1); cookie login; ShvIA icon/title. **Already delivers "same features".** |
| **F2** | Native polish: tray, menu, About, window state, URL config, offline screen, `shvia://` deep-link + auth reconciliation, notifications. (Sidecar comes in here if needed.) |
| **F3** | Server version compatibility check (`/api/v1/health`). |
| **F4** | CI + signing/notarization (macOS, Windows EV, Linux) + auto-update. |
| **F5** | Beta on the 3 OSes + WebView quirk fixes + docs. |

**Estimated effort:** ~5–6 engineer-weeks for a signed/notarized 1.0 on the 3
OSes. The long pole on **schedule** (not eng): **Windows EV cert procurement** —
start on day 1.

---

## Current status

**2026-06-30 — first version released (`0.4.5`).** **Phase 1 complete and validated**
(app opens, logs in via cookie, **chat with SSE streaming** works) and **Phase 2** well
fleshed out: **multi-window** (Ctrl+N), **branding** (B&W Blue3 arrow + navy "AI"),
persisted window state, external links in the browser, **offline screen** and
**local** packaging (`.deb`/`.AppImage`/`.rpm` via `build-local`). Detail of what works in
[docs/funcionalidades.md](docs/funcionalidades.md); how to build in
[docs/build.md](docs/build.md).

**1 known pending issue** (WebKitGTK limitation on Linux — ADR-008): **mic
(voice)** doesn't work in Linux local packaging; macOS/Windows are likely to resolve it,
with an Electron fallback if it becomes a must-have. Living context and pending items in
[.continue/estado-atual.md](.continue/estado-atual.md). **Ctrl+V for images** is
implemented (`CLIPBOARD_IMAGE_PASTE_JS` injected on the server host) — pending docs
update in [.continue/estado-atual.md](.continue/estado-atual.md).

> **No doc, no deploy.** Every new feature becomes doc in `docs/` before it goes in.
