# Build & Empacotamento — ShvIA Desktop

Como gerar os instaladores do app. **O build é LOCAL** (na sua máquina) — sem
GitHub Actions (Actions é caro em repo privado; build local incremental, com o
cache do cargo, é mais rápido). O ShvIA Desktop é **shell fino** (Tauri 2, sem
sidecar), então é direto: `tauri build`.

## TL;DR

| SO | Como (na raiz do repo) | Saída |
|----|------------------------|-------|
| **Linux** | `./build-local.sh` | `.deb` + `.AppImage` + `.rpm` |
| **macOS** | `./build-local.sh` | `.dmg` + `.app.tar.gz` |
| **Windows** | `.\build-local.ps1` (ou duplo-clique `build-local.cmd`) | `.msi` + `-setup.exe` |

Saída em `src-tauri/target/release/bundle/`. Opções (Linux/macOS): `--skip-npm-ci`,
`--bundles <deb|appimage|rpm|dmg|app>`. Windows: `-SkipNpmCi`.

> ⚠️ **Cross-build não rola:** cada SO se builda **no próprio SO**. Para cobrir os
> 3, rode o `build-local` em cada máquina (1 Linux, 1 macOS, 1 Windows).

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
`package.json`, `tauri.conf.json`, `Cargo.toml` e os lock files — roda no `prebuild`
(npm) e via `npm run version:sync` (o `build-local` chama antes de buildar).

## Rodar o app empacotado numa VM/sem GPU

Em ambiente **remoto/VM/NVIDIA sem acesso a DRM**, o app empacotado pode abrir com
**janela em branco** (quirk do WebKitGTK — ver [decisoes.md](decisoes.md), ADR-006).
Para abrir ali, rode com **`WEBKIT_DISABLE_DMABUF_RENDERER=1`** (render por
software). Não afeta o build, só a execução. Em máquina com GPU real, abre normal.

## Distribuição

Como no SHVTERM, a entrega é **local-first**: o repo é privado, então os
instaladores são distribuídos pelo canal do time (upload manual / site), não por
GitHub Releases.

## Assinatura / notarização (F4 — ainda não)

Hoje os bundles saem **sem assinatura** (avisos de SmartScreen/Gatekeeper). Quando
os certificados forem procurados (long pole de prazo):

- **macOS** — Apple Developer ID + `notarytool` (no `build-local.sh`/CI).
- **Windows** — Authenticode **EV** (Azure Trusted Signing preferível).
- **Updater Tauri** — chave `TAURI_SIGNING_PRIVATE_KEY` + `latest.json`.

## CI (GitHub Actions) — omitida de propósito

Foi **removida por custo** (Actions caro em repo privado; macOS conta 10x). O
SHVTERM tem a referência pronta (`.github/workflows/main.yml`: matriz mac/win/linux
via `tauri-action` + Release + updater). Se na **F4** fizer sentido (releases
assinados/auto-update centralizados), dá pra trazer aquele workflow e adaptar.
