# Build & Empacotamento — ShvIA Desktop

Como gerar os instaladores do app nos 3 SOs. O ShvIA Desktop é **shell fino**
(Tauri 2, sem sidecar), então o build é direto: `tauri build`. Modelado no
SHVTERM, porém enxuto.

## TL;DR

| Onde | Como | Saída |
|------|------|-------|
| **CI (3 SOs)** | tag `v*` ou *workflow_dispatch* → [`.github/workflows/build.yml`](../.github/workflows/build.yml) | artefatos por SO + GitHub Release (em tag) |
| **Local — Linux/macOS** | `./build-local.sh` | `src-tauri/target/release/bundle/` |
| **Local — Windows** | `.\build-local.ps1` (ou duplo-clique `build-local.cmd`) | idem |

> ⚠️ **Cross-build não rola:** de uma máquina **Linux** só se builda **Linux**;
> macOS e Windows precisam dos **próprios SOs**. Por isso a forma de cobrir os 3
> é a **CI** (runners `macos`/`ubuntu`/`windows`) — ou rodar o `build-local` em
> cada máquina.

## Targets por SO

- **Linux** — `.deb` + `.AppImage` (+ `.rpm`), via `targets: "all"` do `tauri.conf.json`.
- **macOS** — `.dmg` + `.app.tar.gz`.
- **Windows** — `.msi` (WiX) + `-setup.exe` (NSIS).

## Pré-requisitos

- **Comum:** Node 20, Rust (`rustup default stable`).
- **Linux (Debian/Ubuntu):**
  `sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf`
- **macOS:** `xcode-select --install`.
- **Windows:** Rust + MSVC ("Desktop development with C++") e WebView2 (já vem no Win 11).

## Versão

`version.md` é a **fonte única**. O `scripts/sync-version.mjs` propaga para
`package.json`, `tauri.conf.json`, `Cargo.toml` e os lock files — roda no
`prebuild` (npm) e via `npm run version:sync` (a CI e os scripts locais chamam
antes de buildar). Tag de release = `v<version.md>`.

## Rodar o app empacotado numa VM/sem GPU

Em ambiente **remoto/VM/NVIDIA sem acesso a DRM**, o app empacotado pode abrir com
**janela em branco** (quirk do WebKitGTK — ver [decisoes.md](decisoes.md), ADR-006).
Para abrir ali, rode com **`WEBKIT_DISABLE_DMABUF_RENDERER=1`** (render por
software). Não afeta o build, só a execução. Em máquina com GPU real, abre normal.

## Assinatura / notarização (F4 — ainda não)

Hoje os bundles saem **sem assinatura** (avisos de SmartScreen/Gatekeeper).
Quando os certificados forem procurados (long pole de prazo), entram na CI:

- **macOS** — Apple Developer ID + `notarytool` (secrets `APPLE_*`).
- **Windows** — Authenticode **EV** (Azure Trusted Signing preferível).
- **Updater Tauri** — chave `TAURI_SIGNING_PRIVATE_KEY` + `latest.json`.

Os pontos de entrada já estão marcados em `build.yml`. **Hardening:** pinar as
actions por SHA (como o SHVTERM faz).
