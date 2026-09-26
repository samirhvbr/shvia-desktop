# Windows validation runbook (the owner runs it)

> **Why this exists.** The Windows build did not compile from 0.9.0 to 1.6.8. Since 1.6.8 it
> compiles, and since 1.6.37 CI cross-checks it on every change to `src-tauri/`. It has
> **never run on a Windows machine**. CI checks the `x86_64-pc-windows-gnu` target, while the
> real build is MSVC (`x86_64-pc-windows-msvc`). So this runbook is the first time the MSVC
> build and the Windows-only code run at all: the WebView2 bridge (`windows_ipc.rs`), the tray,
> engine lookup (`where`, `%LOCALAPPDATA%\Programs`), the runners installed by `install.ps1`
> (1.6.56–1.6.58), and the updater's installer hand-off.
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
# For step 7 (the Claude and Codex engines). Each login opens the browser once.
npm install -g @anthropic-ai/claude-code ; claude login
npm install -g @openai/codex ; codex login
```

## 1. Build

```powershell
git pull
.\build-local.ps1 -NoAnna -NoSign
```

Since 1.6.50 the script decides this by itself. `-NoAnna` builds without the engine, and the
script tells Tauri there is no sidecar. `-NoSign` skips Authenticode and, when this machine has no
updater key, builds without updater artifacts, which is right for validation. If you have
`anna.exe` (SHVIA-CODE's Windows build), use `-Anna C:\path\anna.exe` instead of `-NoAnna`: that
covers step 7's gateway engine. If `~\.shvia\updater.key` and `updater.pass` are on this machine,
the script reads them, as `build-local.sh` does.

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
| 7.2 | Pick engine **Claude (assinatura)** with no runner installed | `claude-runner não encontrado — use o botão Instalar runner ou rode claude-runner\install.ps1 …` |
| 7.3 | Press **Instalar runner** (or run `.\claude-runner\install.ps1` in PowerShell) | The output ends with `✓ claude-runner instalado em …\shvia-claude-runner (Agent SDK …)`. **No black console window** opens at any point |
| 7.4 | Engine Claude: a small task ("liste os arquivos da pasta"), then a longer one, and press **Parar** mid-turn | The small one answers. *Parar* stops the turn, and Task Manager shows **no `node.exe` left behind** for it (1.6.57 runs `node` directly, not a `.cmd`) |
| 7.5 | Run `.\codex-runner\install.ps1`, then pick engine **Codex** | Install ends with `✓ codex-runner … instalado`. Then **one of three, all informative** — write down which: **(a)** it starts and a small task answers; **(b)** "o sandbox do Codex NÃO segurou…": the sandbox does not hold on this Windows, and the engine refuses by design; **(c)** "o comando de controle … não rodou": the proof could not run even inside the project. (b) and (c) are the next item, not a failure of this runbook |
| 7.6 | Keep the *Changes* tab open while an engine works | **No console window** flashes for `git status` or for the engine (1.6.57) |

## Known gaps (found while writing this runbook, 23/09/2026)

- ✅ **Fixed in 1.6.50: `build-local.ps1` now does what `build-local.sh` does.** With no `anna`
  staged it tells Tauri there is no sidecar, instead of failing at the end of the build. It reads
  or requires the updater key **before** `npm ci`, instead of failing at the end of the bundle.
  Proven with PowerShell in CI (`npm run prova:ps1`); this machine is the first real run. A
  release build still needs the key, the same one as on the Mac (ADR-022).
- ✅ **Fixed in 1.7.2: the first real run failed where the proof could not look.** The override
  went only into `$env:TAURI_CONFIG`, which the Rust build reads and the Tauri CLI's bundler does
  not, so the build compiled for 13 minutes and then asked for `binaries\anna-<triple>.exe`. It
  now goes as `tauri build --config <file>`, and `prova:ps1` checks the call itself.
- ✅ **Since 1.6.56–1.6.58 the Claude and Codex engines have a Windows path** (the owner:
  "Sim, precisa dos dois no Windows"). `install.ps1` installs each runner (proved under pwsh in
  CI). The app runs them as `node <runner>.mjs` and never through the terminal `.cmd`, the
  install button runs the `.ps1`, and no process opens a console window. On the Codex side, the
  npm `codex.cmd` is resolved to `node codex.js`, and the sandbox proof now has a control, so a
  command that cannot run is no longer read as a sandbox that holds. Before 1.6.58 it was, and
  `/bin/sh` cannot run on Windows.
- **Unknown until this machine: does Codex's own sandbox hold on Windows?** It is experimental
  there. The app-server protocol has a Windows sandbox check and setup (`windowsSandbox/readiness`,
  `windowsSandbox/setupStart`, read from its schema) that the runner does not call yet. Step 7.5 says which case this
  machine is, and (b) or (c) decides whether calling that setup is the next item.

## Report

| step | ✅/❌ | note |
|---|---|---|
| 1 build (time: ___ min) | | |
| 2.1–2.4 | | |
| 3.1–3.6 | | |
| 4.1–4.2 | | |
| 5.1–5.2 | | *(paste the Diagnóstico text below)* |
| 6.1–6.3 | | |
| 7.1–7.6 (7.5: a, b or c) | | |
