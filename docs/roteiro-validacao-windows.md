# Windows validation runbook (the owner runs it)

> **Why this exists.** The Windows build did not compile from 0.9.0 to 1.6.8. Since 1.6.8 it
> compiles, and since 1.6.37 CI cross-checks it on every change to `src-tauri/`. It has
> **never run on a Windows machine**. CI checks the `x86_64-pc-windows-gnu` target, while the
> real build is MSVC (`x86_64-pc-windows-msvc`). So this runbook is the first time the MSVC
> build and the Windows-only code run at all: the WebView2 bridge (`windows_ipc.rs`), the tray,
> engine lookup (`where`, `%LOCALAPPDATA%\Programs`), and the updater's installer hand-off.
> The owner answered "I'll do it — send the runbook" on 23/09/2026.

About 45 minutes on Windows 11 x64 (Windows 10 works if WebView2 is installed). Mark each step
✅ or ❌. For a ❌, copy the first error line and what the screen said. Paste the filled table at
the end into the panel or an issue.

## 0. Once per machine

```powershell
winget install OpenJS.NodeJS.LTS
winget install Rustlang.Rustup ; rustup default stable
# Visual Studio Build Tools with "Desktop development with C++" (the MSVC linker)
git clone https://github.com/samirhvbr/shvia-desktop ; cd shvia-desktop
```

## 1. Build

```powershell
git pull
# This validation build needs neither the updater key nor the anna engine. This line tells
# Tauri so (see "Known gaps" below):
$env:TAURI_CONFIG = '{"bundle":{"externalBin":[],"createUpdaterArtifacts":false}}'
.\build-local.ps1 -NoAnna
```

If you have `anna.exe` (SHVIA-CODE's Windows build), use `-Anna C:\path\anna.exe` instead of
`-NoAnna`, and set `$env:TAURI_CONFIG = '{"bundle":{"createUpdaterArtifacts":false}}'`. That
covers step 7's gateway engine.

**Expected:** `src-tauri\target\release\bundle\nsis\ShvIA_<version>_x64-setup.exe` and
`...\msi\ShvIA_<version>_x64_en-US.msi`. Note the build time. It is unsigned by decision
("only Apple", `docs/build.md`).

## 2. Install and first launch

| # | Do | Expected |
|---|---|---|
| 2.1 | Run the `-setup.exe` | SmartScreen "Windows protected your PC": **expected** (unsigned). *More info* → *Run anyway* |
| 2.2 | Open ShvIA | Splash with the logo and "ai.shvia.org", then the ShvIA login page |
| 2.3 | Log in, open a chat, send a message | The answer **streams** in token by token (SSE), it does not arrive all at once |
| 2.4 | Resize and move the window, quit (tray → *Sair do ShvIA*), open again | Same size and place, still logged in |

## 3. Window, tray, menus

| # | Do | Expected |
|---|---|---|
| 3.1 | Close the window with the X | The app **keeps running in the tray**: closing collapses on all three OSes |
| 3.2 | Left-click the tray icon | The window comes back |
| 3.3 | Right-click the tray icon | *Servidor: ai.shvia.org*, *Abrir o ShvIA*, *Verificar atualizações…*, *Iniciar com o sistema*, *Fechar mantém rodando aqui*, *Sair do ShvIA* |
| 3.4 | Tick *Iniciar com o sistema*, restart Windows | ShvIA starts with Windows. Untick it afterwards |
| 3.5 | Menu *Arquivo → Nova janela* (Ctrl+N), then *Recarregar* (Ctrl+R) | A second window opens, and a reload does not log you out |
| 3.6 | Click an external link in a chat | It opens in the **default browser**, not inside the app |

## 4. Notifications

| # | Do | Expected |
|---|---|---|
| 4.1 | Trigger any server event that notifies (a price alert, a routine result) | A Windows toast appears |
| 4.2 | Look at the taskbar icon | **No badge**, by design: Windows has no badge count in the plugin (ADR-017) |

## 5. Help

| # | Do | Expected |
|---|---|---|
| 5.1 | *Ajuda → Diagnóstico…* | Versions, OS, server, engine status. **Copy it into the report**: it is the best single piece of evidence |
| 5.2 | *Ajuda → Verificar atualizações…* | "No update" or equivalent: no Windows build is published in the manifest yet |

## 6. Code mode: the WebView2 bridge (`windows_ipc.rs`)

| # | Do | Expected |
|---|---|---|
| 6.1 | Open Code mode, pick a folder with the native dialog | The dialog opens. The panel shows the file tree and `git status` |
| 6.2 | Open a file from the tree, and a diff from *Changes* | Contents show, including a path with accents or spaces |
| 6.3 | Save a generated image (any "save" action) | The native save dialog opens and writes the file |

## 7. Engines

| # | Do | Expected |
|---|---|---|
| 7.1 | Engine **gateway (anna)** — only if you built with `-Anna` | Found (*bundled*). A small task ("liste os arquivos da pasta") answers |
| 7.2 | Engine **Claude (assinatura)** | **Expected to be unavailable**, see "Known gaps". Write down exactly what the screen says |
| 7.3 | Engine **Codex** | **Expected to be unavailable**, same reason. Write down the screen |

## Known gaps (found while writing this runbook, 23/09/2026)

- **`build-local.ps1` lacks the 1.6.24 fix.** Tauri refuses to build when a `bundle.externalBin`
  file is missing. `build-local.sh` passes `externalBin: []` when no `anna` is staged, and the
  PowerShell script does not, so `-NoAnna` (or no `anna` on PATH) fails at the end of the build.
  The `TAURI_CONFIG` line in step 1 works around it. The real fix belongs in the script, measured
  on this machine.
- **`build-local.ps1` has no updater-key check.** Without `TAURI_SIGNING_PRIVATE_KEY` the release
  build fails at the end. `createUpdaterArtifacts: false` in step 1 is fine for validation. A
  release build needs the key, the same one as on the Mac (ADR-022).
- **The Claude and Codex engines have no Windows path.** Their runners install through
  `install.sh`, which is bash, and the app looks for `claude-runner.exe`/`codex-runner.exe` on
  Windows. So step 7.2/7.3 is expected to say the runner is missing. Whether Windows needs these
  engines is a product question for the owner.

## Report

| step | ✅/❌ | note |
|---|---|---|
| 1 build (time: ___ min) | | |
| 2.1–2.4 | | |
| 3.1–3.6 | | |
| 4.1–4.2 | | |
| 5.1–5.2 | | *(paste the Diagnóstico text below)* |
| 6.1–6.3 | | |
| 7.1–7.3 | | |
