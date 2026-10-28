# Build & Empacotamento — ShvIA Desktop

Release installers are built **locally on each target OS**, through the versioned
`build-local` wrappers. The shared application interface comes from ShvIA Web; the
package also contains native Rust capabilities, the optional `anna` sidecar and the
Claude/Codex runner resources. A direct `tauri build` does not perform all the wrapper's
staging, credential preflight, manifest and publication steps.

> **Source review — 2026-10-15, baseline `983f2aa` (1.29.0).** This guide was checked against
> build scripts, manifests and workflows. No installer was built, signed, notarized,
> uploaded or accepted on a real device during this review. Credential files were not
> inspected. External credential-setup UI steps below retain their original context;
> this review checked the scripts' selection logic, not Apple's current account screens.

**Historical release policy:** the CI packaging matrix was removed in 0.4.6 for cost.
The later Enterprise decision recorded 50,000 minutes/month and a 10x macOS multiplier;
those figures are historical account/pricing notes, not a current billing audit. The
current workflows run verification and create the GitHub version record; they do not
package or sign release installers. Restoring that pipeline is a separate architecture
decision.

## TL;DR

| SO | Como (na raiz do repo) | Saída |
|----|------------------------|-------|
| **Linux** | `./build-local.sh` | `.deb` + `.AppImage` + `.rpm` |
| **macOS** | `./build-local.sh` | `.dmg` + `.app.tar.gz` |
| **Windows** | `.\build-local.ps1` (ou duplo-clique `build-local.cmd`) | `.msi` + `-setup.exe` |

Saída em `src-tauri/target/release/bundle/`. Opções (Linux/macOS): `--skip-npm-ci`,
`--bundles <deb|appimage|rpm|dmg|app>`, `--anna <path>`, `--no-anna`, `--pentest <path>`,
`--no-pentest`, `--no-sign`, `--skip-git-pull`, `--force` and `--publish`. Windows uses PowerShell parameters:
`-SkipNpmCi`, `-SkipGitPull`, `-Anna <path>`, `-NoAnna`, `-NoSign`; it has no publish/reuse
switch. These are different scripts, not interchangeable command-line syntaxes.

> ⚠️ **Cross-build não rola:** cada SO se builda **no próprio SO**. Para cobrir os
> 3, rode o `build-local` em cada máquina (1 Linux, 1 macOS, 1 Windows).

The platform CI can cross-check Windows GNU Rust from Linux, but that is not the Windows
MSVC installer build. The native WebView, packaging tools and release acceptance still
belong to the target OS. See [the Windows acceptance runbook](roteiro-validacao-windows.md).

## What is packaged and what must exist at runtime

| Component | Build input | Runtime boundary |
|---|---|---|
| Local shell | Root `npm ci`, TypeScript and Vite (`npm run build`) | Offline/loading/settings shell; the main application page is remote |
| Native app | `src-tauri/Cargo.toml`, `Cargo.lock` and OS-specific Tauri dependencies | WebView and OS permissions; capabilities differ by OS |
| SHVIA engine | `scripts/stage-anna.mjs`, then `bundle.externalBin` | Optional `anna` executable matching the build host's target triple |
| Pentest engine | `scripts/stage-pentest.mjs` (release pinned in `pentest-engine.json`), then `bundle.externalBin` in `tauri.linux.conf.json` / `tauri.macos.conf.json` | Linux and macOS only. The engine ships in the installer; Docker and the sandbox image it pulls on first use are separate requirements |
| Packet-capture sidecar | `scripts/stage-traffic-capture.mjs` (builds `src-tauri/traffic-capture` with `cargo build --release` and runs it once before staging), then `bundle.externalBin` in `tauri.linux.conf.json` / `tauri.macos.conf.json` | Linux and macOS only; `build-local.sh --no-traffic` skips it. The Linux build machine needs `libpcap-dev`, the `.deb`/`.rpm` depend on `libpcap0.8`/`libpcap`, and capture needs a permission grant at run time ([TRAFFIC/README.md](TRAFFIC/README.md)) |
| Claude runner | Bundled `.mjs`/`.json` and installers under `claude-runner/` | Node, runner installation, SDK and authorized Claude credentials; see [runner README](../claude-runner/README.md) |
| Codex runner | Bundled modules/installers/schema under `codex-runner/` | Node, installed runner and Codex CLI/app-server/login; see [runner README](../codex-runner/README.md) |

`stage-anna.mjs` selects `--from`, then `SHVIA_ANNA_BIN`, then `anna` on PATH. It probes
`--version`, checks the minimum supported version and stages the target-named executable.
The minimum is defined in the script; a version merely printed in an old build log is not
a compatibility check. Absence removes a stale staged binary, and the wrappers override
`externalBin` with an empty list. `--no-anna`/`-NoAnna` therefore means **no bundled SHVIA
engine**, not that every other local/remote Code engine disappears. An independently
installed engine can still be discovered at runtime.

## Pentest engine

The controlled assessment profile runs a local engine. It used to be something the operator
installed and updated by hand, and a Desktop with the right version (1.30.5) still said
"Pentest not detected" because the engine was a separate thing. It now ships inside the
installer, so updating the Desktop updates it.

**What it is.** The SHVIA fork of Strix, frozen with PyInstaller (one file, about 94 to 118
MB per platform) by that repository's `pentest-binaries.yml`, and published on its GitHub
Release as `pentest-<target triple>` plus a `SHA256SUMS`. It is built there, not here: this
repository has no Python source and this build has no `pip` step.

**Where it lands.** `externalBin` puts it beside the executable: `Contents/MacOS/pentest` on
macOS, and `/usr/bin/pentest` in the `.deb` (measured: `ShvIA_1.32.0_amd64.deb` is 117.7 MB, the engine 111.8 MB of it, and the packaged
`pentest --fork-version` runs). The `.rpm` and the AppImage were not built here. The app looks for it there and nowhere
else (`motores::bundled_bin`): no lookup on PATH, so what runs is always the build the
installer carried. A build that carries none answers `E_PENTEST_NOT_BUNDLED`, which the page
translates, instead of "update Pentest".

**Staging.** `scripts/stage-pentest.mjs` takes `--from`, then `SHVIA_PENTEST_BIN`, then the
release pinned in `pentest-engine.json`: a version, the release URL and one SHA-256 per
target triple. A downloaded file is cached under `~/.cache/shvia/pentest-engine/` and is
checked against that hash on every build. What fails the build is a file that is there and
wrong: a hash that differs from the pinned one, an engine that does not run or does not print
a plain `X.Y.Z` for `--fork-version`, one below the floor, or one that prints a different
version than the manifest pins. What does not fail it is an absence (no network and no cache,
no asset for the platform): the build goes on without the engine and removes any engine an
earlier build staged. `npm run prova:pentest` runs each of those cases against a local HTTP
server and keeps the floor in the script equal to the one in `compatibility.rs`.

**Windows.** The controlled profile refuses to run on Windows, so the engine is declared per
platform and the Windows build never asks for one.

**Raising the engine.** Publish the fork's Release (`pentest-binaries.yml` with `release_tag`),
copy the four SHA-256 values from its `SHA256SUMS` into `pentest-engine.json` with the new
version, and raise the floor in `stage-pentest.mjs` and `compatibility.rs` together only if
the Desktop needs something the older engine lacks.

**Probe time.** A one-file build unpacks itself on every start: `--fork-version` takes about
2.6 s on Linux, cold or warm (measured), and a first launch on macOS also waits for Gatekeeper.
The probe deadline is 20 s for that reason.

**macOS signing: not verified here.** The engine is signed with the app by `tauri build`
(hardened runtime). A PyInstaller one-file engine unpacks libraries that are signed ad hoc, and
library validation refuses libraries that do not carry the app's Team ID, so a signed build
can fail to start. This was not tested: no Mac was available when the engine was bundled. So
`build-local.sh` runs `Contents/MacOS/pentest --fork-version` from inside the signed `.app`
and stops the build, with the explanation, if it does not print a version. The two ways out
each cost something: build the engine on the Mac with `--codesign-identity` (its libraries then
carry the Team ID) and pass it with `--pentest`, or add
`com.apple.security.cs.disable-library-validation` to the app's entitlements, which weakens
library validation for the whole app, not only the engine. `--no-pentest` ships the app without
it.

## Targets por SO

- **Linux** — `.deb` + `.AppImage` (+ `.rpm`), via `targets: "all"` do `tauri.conf.json`,
  **+ `.pkg.tar.zst`** (Arch), que não vem do bundler do Tauri — ver
  [Arch Linux](#arch-linux-pkgtarzst--repo-pacman).
- **macOS** — `.dmg` + `.app.tar.gz`.
- **Windows** — `.msi` (WiX) + `-setup.exe` (NSIS).

## Pré-requisitos

- **Comum:** Node 20.19+ or 22.12+ (the locked Vite requirement; CI uses Node 22), npm and stable Rust. Local runner requirements are additional to the Tauri build toolchain.
- **Linux (Debian/Ubuntu):**
  `sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf`
- **Linux (Arch):**
  `sudo pacman -S --needed base-devel webkit2gtk-4.1 curl wget file openssl libayatana-appindicator librsvg xdotool patchelf`
  (`base-devel` traz `makepkg` + `fakeroot`, que empacotam o `.pkg.tar.zst`)
- **macOS:** `xcode-select --install`.
- **Windows:** Rust + MSVC ("Desktop development with C++") e WebView2 (já vem no Win 11).

Check the WebView2 runtime on the actual Windows machine; the documented MSVC toolchain
and a compiled crate do not prove the WebView/clipboard/microphone or installer works there.
On Linux, install the WebKitGTK development packages for compilation and provide the
runtime packages required by the produced package. The VM workaround below is a runtime
workaround, not evidence of desktop/GPU acceptance.

## Local verification before packaging

Run Node scripts from the repository root. Rust's manifest is under **`src-tauri/`**:
use `--manifest-path src-tauri/Cargo.toml` from the root, or change into that directory
before running Cargo. A bare root-level `cargo clippy` has no manifest to use.

```bash
npm ci
npm run prova:bump
npm run prova:doc
npm run prova:politica
npm run prova:runner-version
```

For Rust checks, first stage a usable `anna` with `npm run anna:stage` (and, on Linux or macOS, the
engine with `npm run pentest:stage`), and install the
Codex runner as described by its [README](../codex-runner/README.md). The suite contains an
installed-runner `--version` smoke check. Then:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
```

A clean checkout has no `src-tauri/binaries/` because those files are ignored. CI prepares
an inert `anna-<triple>` stand-in before Rust tests; it never packages that file. Its separate
no-sidecar compile check uses `TAURI_CONFIG='{"bundle":{"externalBin":[]}}'`. Do not ship a
stand-in or mistake that compile override for a working Code engine. The full preparation,
PowerShell prerequisites and additional packaging/signing contract proofs live in
[ci.yml](../.github/workflows/ci.yml) and [plataformas.yml](../.github/workflows/plataformas.yml).

## Versão

`version.md` é a **fonte única**. O `scripts/sync-version.mjs` propaga para
`package.json`, `tauri.conf.json`, `Cargo.toml` e os lock files — roda no `prebuild`
(npm) e via `npm run version:sync` (o `build-local` chama antes de buildar).

## Rodar o app empacotado numa VM/sem GPU

Em ambiente **remoto/VM/NVIDIA sem acesso a DRM**, o app empacotado pode abrir com
**janela em branco** (quirk do WebKitGTK — ver [decisoes.md](decisoes.md), ADR-006).
Para abrir ali, rode com **`WEBKIT_DISABLE_DMABUF_RENDERER=1`** (render por
software). Não afeta o build, só a execução. Em máquina com GPU real, abre normal.

## Arch Linux (`.pkg.tar.zst` + repo pacman)

O bundler do Tauri 2.x não gera pacote pacman, então ele sai do
[`packaging/arch/PKGBUILD`](../packaging/arch/PKGBUILD), que **reempacota o `.deb`**
recém-gerado em vez de recompilar. Dois caminhos, escolhidos pela distro que roda
o build (o `build-local.sh` detecta por `/etc/os-release`, `ID` + `ID_LIKE`):

| Build roda em | Ferramenta | Resultado |
|---|---|---|
| Arch (ou derivada) | `makepkg` com o PKGBUILD | pacote nativo, com hook de pós-instalação |
| Debian/Ubuntu | `fpm -s dir` sobre o payload extraído do `.deb` | mesmo `pkgname`, mesmas deps, mesmo marcador |

As duas rotas têm de entregar pacote **equivalente** — mesmo nome (`shvia-desktop`),
mesmas dependências e o marcador de origem dentro. Não é preciosismo: até a 1.1.17 a
rota `fpm` saía sem o marcador e com outro nome (`shv-ia`), e o pacote publicado dessa
forma entregou ao usuário do Arch um update que falha no fim do download
([ADR-029](decisoes.md#adr-029--o-guard-do-auto-update-não-pode-depender-do-empacotamento)).
A rota `makepkg` segue sendo a preferida — ela é nativa e tem uma lista de deps só —,
mas nenhuma das duas é "best-effort".

Quem instalou o `shv-ia` antigo migra sozinho: o PKGBUILD e o `fpm` declaram
`replaces`/`conflicts` para aquele nome, e o `pacman -Syu` troca o pacote.

> ⚠️ **`-Syu` migra; `-U` NÃO.** Medido em 16/08/2026 num Arch de verdade (pacman
> 7.1.0): `replaces` só é consultado em transação de **sync**. Instalando o arquivo
> à mão com `pacman -U`, o pacman vê apenas o `conflicts`, pergunta *"Remove
> shv-ia?"* e, sem resposta, aborta com **`unresolvable package conflicts
> detected`**. Não é defeito do PKGBUILD — é semântica do pacman —, mas é um beco
> sem saída para quem instala pelo arquivo, que é justamente o caminho de quem não
> tem o repo configurado (ver o fim desta seção). Saída: `pacman -Rdd shv-ia` antes,
> ou responder `y` ao prompt.
>
> Pelo `-Syu`, o mesmo cenário sai limpo: *"Replace shv-ia with
> shvia/shvia-desktop? [Y/n]"* com default **Y**, remove o antigo, instala o novo e
> o marcador `instalado-por` sobrevive à troca.

Avulso, a partir de um `.deb` que já está no disco:

```bash
cd packaging/arch && makepkg -f
SHVIA_DEB=/caminho/x.deb makepkg -f     # apontando outro .deb
```

### No Arch, quem atualiza é o pacman — não o app (ADR-028)

O `tauri-plugin-updater` não tem instalador de pacman: `bundle_type()` só devolve
`Deb`/`Rpm`/`AppImage`/`Msi`/`Nsis`. Como o pacote sai do payload do `.deb`, o
binário chega marcado como **DEB** — e o plugin rodaria `pkexec dpkg -i`, que não
existe no Arch, **depois** de baixar ~80 MB.

Por isso o pacote instala `/usr/share/shvia-desktop/instalado-por` com o conteúdo
`pacman`, e o [`src-tauri/src/updater.rs`](../src-tauri/src/updater.rs) lê esse
arquivo: havendo versão nova, ele **avisa e manda rodar `pacman -Syu`** em vez de
oferecer o download. É um contrato entre os dois arquivos — mudou um, mude o outro.

E o app **não depende só desse contrato** (ADR-029): mesmo sem o marcador, um install
que se diz `deb` num sistema sem `dpkg` é impedimento suficiente para não oferecer o
download. É o que segura o caso de um pacote pacman montado por fora do PKGBUILD —
inclusive um empacotado errado por nós.

O `--publish` sobe, junto dos artefatos, o banco do repositório
(`shvia.db`, `shvia.db.tar.gz`, `shvia.files`, `shvia.files.tar.gz`, gerados por
`repo-add`). Do lado do usuário, em `/etc/pacman.conf`:

```ini
[shvia]
SigLevel = Optional TrustAll
Server = https://ai.shvia.org/storage/desktop
```

`SigLevel = Optional TrustAll` porque o pacote **não é assinado por GPG** — a
assinatura minisign do pipeline cobre os artefatos do updater, e o pacman não a
entende. Assinar o repo com GPG é o passo que falta para tirar o `TrustAll`.

O banco é regerado a cada build e lista **só a versão corrente**: a máquina de build
só tem o pacote que ela acabou de gerar, e o repo existe para servir a última versão.

Sem `repo-add` no PATH (no Debian ele vem em `pacman-package-manager`), o `.pkg` ainda
é publicado — só não há `pacman -Syu`, e a instalação vira `pacman -U <arquivo>`.

## Distribuição

Delivery is **local-first**, as in SHVTERM: users get the installers from the server
(`ai.shvia.org/storage/desktop/`, through `--publish` and the updater), never from GitHub.

**Since 1.6.6 the GitHub Release is the record of what went out.** `--publish` attaches the
files it just sent to the server — installers, their `.sha256`, and `release.json` — to the
Release of the same version (`gh release upload <version> … --clobber`). Before that, every
Release had 0 assets (finding f121), and nothing recorded which installers were published for
which OS: the server's `release.json` is overwritten by each publish. The repository was
confirmed **public** through GitHub metadata on 2026-10-15; do not treat Release assets as
private storage. The earlier storage-cost decision was recorded on 23/09/2026.

The [Release workflow](../.github/workflows/release.yml) creates the version's GitHub record
when `version.md` changes on the default branch. That is separate from producing installers,
publishing them to the download server and accepting an installed update. A Release entry
alone does not prove that every OS has an artifact or that the server serves it.

## Assinatura / notarização

### macOS — ✅ in `build-local.sh`

**Why it matters:** an unsigned app is *"damaged"* to macOS, which offers to **move it to the
trash** when it is opened after a **download or transfer** (the app carries the *quarantine*
attribute). The fix is **sign (Developer ID) + notarize + staple**.

On macOS, `build-local.sh` does it by itself:

1. **Signs.** It finds the `Developer ID Application` cert in the keychain and exports
   `APPLE_SIGNING_IDENTITY`; `tauri build` signs the `.app` with the *hardened runtime*.
2. **Notarizes + staples.** It picks the notarization credential (below) and exports it;
   `tauri build` uploads the app to Apple, waits for the verdict and staples the `.app`, and
   the script does the same for the `.dmg`.
3. **Reports verification.** At the end it runs `codesign --verify`, `spctl` (Gatekeeper)
   and `stapler validate`. The publish gate is narrower: it requires strict `codesign`
   verification for an available `.app` and a stapled ticket for an available `.dmg`.
   `spctl` and the `.app` ticket check in the earlier reporting function do not themselves
   abort publication. See the gate boundary below; this source review did not certify
   any artifact's Developer ID, notarization or Gatekeeper acceptance.

**The credential: an App Store Connect API key (since 1.6.39).** Until 1.6.38 it was the
app-specific password of the Apple ID. That password went to `notarytool` as `--password`, on
the command line of a process that waits minutes for Apple, and anything on the Mac could read
it in `ps` for that long. Tauri's bundler notarizes the `.app` the same way, so changing only
the script's own call would not have closed it. With the API key, `notarytool` gets
`--key <path to .p8> --key-id --issuer`, and nothing secret goes on the command line.

The order `build-local.sh` follows (`escolhe_credencial_de_notarizacao`):

1. `APPLE_API_KEY` + `APPLE_API_ISSUER` + `APPLE_API_KEY_PATH` from the environment.
2. Otherwise, the one `~/.shvia/AuthKey_<KEYID>.p8` (the Key ID comes from the file name)
   with the issuer ID in `~/.shvia/apple-api-issuer`. If there are two keys, it refuses to
   guess: set `APPLE_API_KEY`.
3. Otherwise, the old app-specific password in the keychain (service `shvia-notarize`),
   with a warning each build.
4. Otherwise, an available signing identity can sign the build without notarization.
   The normal DMG publish path then fails its ticket check; an app-only publish has the
   narrower gate described above and must not be called notarization-verified.

When the key is used, `APPLE_ID`/`APPLE_PASSWORD` are **unset**, even if `signing.env`
exported them, so Tauri is never left to choose between two credentials. A half-configured
key (the file without the issuer, or a Key ID naming a file that is not there) is reported by
name before the build goes back to the password. `npm run prova:notarizacao` runs the real
functions against fakes and reads the argv `notarytool` would have received.

#### Creating the key: once, about 5 minutes (the owner's step)

You need the **Account Holder** or **Admin** role in the Apple Developer team (S65UBCTPN5).

1. Open App Store Connect › Users and Access › Integrations › App Store Connect API
   (<https://appstoreconnect.apple.com/access/integrations/api>), tab **Team Keys**. If the page
   first asks to request access to the API, the Account Holder does that once.
2. Click **+** (Generate API Key). Name: `shvia-notarize`. Access: **Developer**, which is
   what Tauri's guide asks for and is enough for notarization. Click **Generate**.
3. Copy the **Issuer ID** shown above the table (a UUID) and save it:
   ```bash
   mkdir -p ~/.shvia && printf '%s\n' 'PASTE-THE-ISSUER-ID' > ~/.shvia/apple-api-issuer
   ```
4. In the new key's row, click **Download**. **Apple lets you download it only once.** It
   saves `AuthKey_<KEYID>.p8`, where `<KEYID>` is the Key ID column. Move it into place:
   ```bash
   mv ~/Downloads/AuthKey_*.p8 ~/.shvia/ && chmod 600 ~/.shvia/AuthKey_*.p8
   ```
   Keep a copy in the password manager, as you do with `updater.key`. If it is lost, revoke
   it on the same page and generate another; nothing already notarized is affected.
5. Check that Apple accepts it (it lists the past submissions, or an empty list):
   ```bash
   xcrun notarytool history --key ~/.shvia/AuthKey_<KEYID>.p8 --key-id <KEYID> \
     --issuer "$(cat ~/.shvia/apple-api-issuer)"
   ```
6. `./build-local.sh` should now print
   `✔ notarização: chave de API <KEYID> (App Store Connect) — nada secreto na linha de comando`.
7. After one notarized build with the key, retire the password. Remove it from the keychain
   (`security delete-generic-password -s shvia-notarize`) and revoke it at
   [account.apple.com](https://account.apple.com) › Sign-In and Security › App-Specific
   Passwords.

On another Mac that builds releases, repeat steps 3–4 with the same `.p8` from the password
manager: one key serves every machine. It never goes in the repo, in `signing.env`, or in chat.
Tauri's own reference: <https://v2.tauri.app/distribute/sign/macos/>.

### Assinatura do updater (obrigatória a partir da 1.0.0)

O par minisign existe desde 28/07/2026 e a **pública** está no `tauri.conf.json`
(ADR-022). A **privada** vive na máquina de release + no cofre, e precisa estar
disponível **antes** do build — sem ela o Tauri não grava os `.sig`, o `release.json`
sai sem o campo `signature` e o servidor passa a responder `204` para todo mundo (o
app nunca oferece o update, em silêncio).

**Configuração da máquina de release: dois arquivos, e nada de `export`.**

```
~/.shvia/updater.key     a chave privada (copiada da outra máquina de release)
~/.shvia/updater.pass    A SENHA e mais nada — sem export, sem aspas
```

```bash
chmod 600 ~/.shvia/updater.key ~/.shvia/updater.pass
./build-local.sh --publish     # build + sobe pro servidor
```

O `build-local.sh` (1.1.9+) lê os dois **sozinho**, sem `signing.env`, sem variável de
ambiente e sem depender do terminal aberto — que é o ponto: `export` morre quando a
janela fecha, e o sintoma disso é "ontem assinava, hoje não", sem nada ter mudado no
repo.

A senha é resolvida nesta ordem, e a primeira que existir vence:

| ordem | de onde | serve para |
|---|---|---|
| 1 | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` no ambiente | teste pontual |
| 2 | `~/.shvia/updater.pass` (ou `$SHVIA_UPDATER_PASS_FILE`) | os três SOs |
| 3 | keychain, item `shvia-updater` | só macOS |

O keychain (item 3, entrou na 1.1.8) é melhor que arquivo — mas é **só macOS**:
`security` não existe no Linux nem no Windows, e um `signing.env` que o chama sem
guarda devolve senha **vazia em silêncio** ali. Por isso a resolução mora no script e
não no arquivo de credenciais.

O `release-manifest.mjs` avisa em amarelo quando **nada** foi assinado — se esse aviso
aparecer, a chave não chegou ao bundler e o release não serve para auto-update.

### Rotating the updater key (prepared in 1.6.40; publishing it is the owner's act)

The pubkey is **compiled into every installed client**, so a rotation cannot be a swap. It
takes **one transition release** that installed clients still accept (**signed with the OLD
key**) and that already carries the **NEW pubkey**, which every client it installs checks from
then on. Every release after it is signed with the new key.

⚠️ **The cost, chosen with the rotation.** A client that skips the transition release (still on
an older version when the next one is published) can no longer update itself: it rejects the
new key's signature. It has to be reinstalled by hand, from the site or the GitHub Release.
That is why the transition release stays the latest for a while before the next one goes out.

The steps:

1. **Generate the new pair** on the release Mac. `-w` writes `…key` and `…key.pub`, and the
   command asks for the new password:
   ```bash
   npx tauri signer generate -w ~/.shvia/updater-next.key
   printf '%s\n' 'THE-NEW-PASSWORD' > ~/.shvia/updater-next.pass && chmod 600 ~/.shvia/updater-next.*
   ```
   Put both (the key and the password) in the password manager now, next to the current ones.
2. **The transition commit.** Put the content of `~/.shvia/updater-next.key.pub` in
   `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`, bump the version, and write the
   CHANGELOG entry ("…carries the new updater pubkey (key rotation, transition release)").
   Land it.
3. **Build and publish the transition release with the OLD key**, which is still
   `~/.shvia/updater.key`:
   ```bash
   ./build-local.sh --publish --transicao-de-chave     # macOS, then Linux
   ```
   `--transicao-de-chave` declares it. The build reads the previous release's pubkey from
   the last version tag, refuses when it equals the one in `tauri.conf.json` (nothing rotated,
   or the transition already shipped), and checks the signing key and every manifest signature
   against the **previous** pubkey instead of the new one. Without the flag, the old key after a
   pubkey change is refused, and the refusal points to the flag. On Windows, `build-local.ps1`
   signs with whatever `TAURI_SIGNING_PRIVATE_KEY` holds. For this one release, that must be the
   old key.
4. **Hold.** Leave the transition release as the latest long enough for the installed base to
   pass through it. How long is your call. Each client checks 20 s after it starts and every
   6 h (`updater.rs`), so a machine that is used daily passes through within a day. Arch
   installs update through pacman (ADR-028) and never check this key.
5. **Swap the keys** on every release machine (and in the password manager):
   ```bash
   mv ~/.shvia/updater.key ~/.shvia/updater-retired.key && mv ~/.shvia/updater.pass ~/.shvia/updater-retired.pass
   mv ~/.shvia/updater-next.key ~/.shvia/updater.key && mv ~/.shvia/updater-next.pass ~/.shvia/updater.pass
   ```
   Keep the retired key offline. It still signs updates that every pre-transition client
   trusts, which makes it a secret to keep, not to throw away carelessly.
6. **The next release is normal:** `./build-local.sh --publish`. The key proof now requires
   the new key, which matches `tauri.conf.json`.

`npm run prova:transicao` runs the real functions in temp git repositories, with the old
pubkey on a tagged release and the new one on the transition commit.

### Publicar: `--publish` (macOS e Linux)

`./build-local.sh --publish` sobe os artefatos desta plataforma + o `release.json` por
`scp` e **verifica pela URL pública**. Destino padrão em `SHVIA_PUBLISH_DEST`
(sobrescrevível com `--dest`), base pública em `SHVIA_PUBLIC_BASE` (`--base-url`).
Uma senha só — é um `scp` com todos os arquivos; `ssh-copy-id root@HOST` elimina o
prompt de vez.

Três coisas que o passo manual não fazia, e cada uma corresponde a um erro real:

1. **A lista sai do próprio `release.json`.** No macOS o artefato do updater
   (`ShvIA.app.tar.gz`) mora em `bundle/macos/` e o `.dmg` em `bundle/dmg/` — um
   `scp` de um diretório só perde exatamente o arquivo que o updater baixa
   (aconteceu em 28/07/2026). Artefato declarado no manifesto e ausente no disco
   **aborta** a publicação em vez de subir um manifesto quebrado.
2. **O manifesto publicado é baixado ANTES de gerar o novo**, para o merge de
   plataformas acontecer sozinho. Sem isso, publicar do macOS apaga a entrada do
   Windows que estava no servidor, e o sintoma é nenhum: build passa, endpoint
   responde, e só os usuários de Windows param de receber update. Isso substitui o
   passo manual de "copiar o `release.json` de uma máquina para a outra".

   **Since 1.6.22 an unsigned Mac build cannot be published.** `--publish` with `--no-sign` is
   refused before anything runs, and at publish time on macOS the `.app` must pass
   `codesign --verify --deep --strict` and the `.dmg` `stapler validate` — the reuse path, which
   never re-signs, passes through this check too. `npm run prova:apple`.

   **Since 1.6.21 the key of every signature is checked before the upload.** The keyid of each
   `signature` in `release.json` must match the pubkey compiled into the clients; a mismatch
   or an unreadable one aborts, and nothing is sent. The build-time key proof never ran on the
   reuse path, and on a fresh clone it was "deferred" before `npm ci` and never ran again —
   it now runs right after `npm ci`. `npm run prova:chaves`.

   **Since 1.6.20 a failed download aborts the publish.** Before, a timeout, a 5xx, a refused
   connection or unreadable JSON all counted as "nothing published", and the upload replaced
   the server's manifest with one holding only this platform — the loss above, triggered by
   a flaky network. Only HTTP 200 (merge) and 404 (first publish ever) are trusted now. To
   discard the published manifest on purpose: `SHVIA_PUBLISH_SEM_MESCLAR=1 ./build-local.sh
   --publish`. Proven by `npm run prova:manifesto` (a local HTTP server, the real function).

   **E o merge é por ARTEFATO, não por plataforma (1.1.19).** "Uma máquina por
   plataforma" deixou de valer no Linux: a Arch gera o `.pkg.tar.zst` (`makepkg`,
   ADR-028) e a Debian não. Enquanto a plataforma inteira era substituída, publicar
   da Debian apagava do manifesto o pacote pacman que a Arch tinha publicado — com
   o mesmo sintoma nenhum do caso acima, só que um nível abaixo e mais difícil de
   ver, porque as duas máquinas escrevem em `linux`. Agora as entradas se acumulam
   por nome de arquivo dentro da versão, o build atual sempre vence no rehash, e o
   log diz quais artefatos foram **preservados** de outra máquina. Versão nova
   continua descartando o manifesto inteiro — nada de outra release sobrevive.
3. **Verificação pela URL pública, não pelo diretório.** Confere o `sha256` do
   artefato assinado baixando-o de verdade — é o que pega upload truncado, cujo
   sintoma seria falha de assinatura sem explicação.

O `.sig` **não** sobe: o conteúdo dele já está embutido no `release.json`.

4. **Then the same files go to the GitHub Release** of the version (see *Distribution*). This
   step **never fails the build**: the server copy is what users download, the Release copy is
   the record. With no `gh`, no Release yet (it is created by `release.yml` when `version.md`
   reaches `master`), or a failed upload, it warns and prints the `gh release upload` command
   to finish by hand. Proven by `npm run prova:anexa`, also in CI.

### `signing.env`: opcional (as credenciais já são resolvidas sozinhas)

**This file loader belongs to `build-local.sh` (macOS/Linux).** The Windows PowerShell
script resolves the updater key from its environment or `~/.shvia/updater.key` and
`updater.pass`; it does not source a POSIX `signing.env`/`build.env`. Signing credentials
are release-machine inputs, never files to inspect or include in a documentation review.

Com `~/.shvia/updater.key` + `~/.shvia/updater.pass` no lugar, **não é preciso arquivo
de credenciais nenhum** — esta seção existe para quem precisa apontar outro destino de
publicação, outra chave, ou guardar a senha no keychain do Mac.

No macOS, para não ter arquivo de senha:

```bash
security add-generic-password -U -s shvia-updater -a "$USER" -w
```

O `security` pede a senha escondida e a guarda no **keychain**, o mesmo lugar onde a
senha de notarização já mora (`shvia-notarize`). O `build-local.sh` consulta esse item
sozinho quando não há `~/.shvia/updater.pass`.

Se ainda quiser o arquivo de credenciais, ele pode ficar no repo
(`cp signing.env.example signing.env && chmod 600 signing.env`) ou **fora dele** —
sobrevive a clone novo, a `git clean -xdf` e a apagar a árvore inteira:

```bash
mkdir -p ~/.config/shvia
cp signing.env.example ~/.config/shvia/build.env && chmod 600 ~/.config/shvia/build.env
```

Mesmo modelo de arquivo, mesmo endereço que o SSHVTERM-DESKTOP já usa
(`~/.config/sshvterm/build.env`): a máquina de release é a mesma, e um hábito só para
os dois repos é menos coisa para lembrar. A ordem de procura é
`$SHVIA_BUILD_ENV` → `./signing.env` → `~/.config/shvia/build.env`, e vence o primeiro
que existir.

O `build-local.sh` carrega o arquivo **sozinho** e **anuncia na primeira linha** qual
deles carregou — "o build saiu assinado ou não" não pode depender de um arquivo
invisível, e com dois endereços possíveis o caminho na tela é o que evita editar um
arquivo enquanto o build lê o outro.

Duas escolhas do formato:

- **Guarda o caminho da chave, não a chave** — via substituição de comando:
  `TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.shvia/updater.key)"`. O arquivo fica com o
  caminho (um `signing.env` vazado sem a chave não assina nada) e a variável recebe o
  conteúdo, que é o que o `tauri build` exige.

  > ⚠️ **Não use `TAURI_SIGNING_PRIVATE_KEY_PATH`.** Ela funciona no
  > `tauri signer sign`, mas o **bundler a ignora** e só reclama no FIM do
  > empacotamento. Descoberto do jeito caro em 28/07/2026: um build do SSHVTERM
  > assinou e notarizou o `.app`, gerou o `.dmg`, e só então morreu com "A public key
  > has been found, but no private key". O preflight recusa esse caso agora.
- **Não é `.env`.** O Vite lê `.env` neste projeto (só expõe `VITE_*` ao bundle, então
  nada daqui vazaria para o JavaScript) — mas `.env` é o nome que toda ferramenta
  procura, e já houve incidente de `.env` sobrescrito nesta máquina. É também o padrão
  já usado no SSHVTERM-DESKTOP.

`signing.env` está no `.gitignore`; o `.example` é versionado. O `.example` usa
`${VAR:-...}`, então um `export` feito no shell **vence** o arquivo — um teste pontual
não é sobrescrito por ele.

### A chave é exigida ANTES de compilar (e é UMA para as três máquinas)

Como o bundle gera artefato de updater, o build **exige**
`TAURI_SIGNING_PRIVATE_KEY`. O script verifica isso no primeiro segundo — o Tauri só
reclamaria no fim do empacotamento (na máquina Linux, em 28/07/2026, custou **2m01s**
de compilação antes de abortar com `A public key has been found, but no private key`).

A variável estar preenchida **não é prova**, então o preflight assina um arquivo
descartável (~1s) e checa as duas coisas que sobram:

1. **A senha abre a chave.** Senha errada aborta o `tauri build` no último passo, e
   leva os minutos de compilação junto.
2. **É a chave do par publicado.** O keyid da assinatura é comparado com o da `pubkey`
   do `tauri.conf.json`. Este é o caro: com a chave de outro par o build termina, o
   release sai, e **todo cliente já instalado recusa o update** — quem está na versão
   antiga fica preso nela, e o próprio updater não conserta isso depois. Não é
   hipótese: esta máquina tem mais de uma chave minisign no disco
   (`~/.shvia/updater.key`, `~/.tauri/*.key`, a do SSHVTERM) e todas são igualmente
   válidas aos olhos do bundler.

Em árvore recém-clonada (sem `node_modules`, que o `npm ci` só instala no passo
seguinte) não há CLI para a prova: o script avisa que adiou e segue — buscar a CLI da
rede dentro de um preflight que se vende como instantâneo seria pior.

O par é **único** (ADR-022): a mesma chave que assina o release do macOS assina o do
Linux e o do Windows. Cada máquina de build precisa dela no ambiente — copiada pelo
gerenciador de senhas, nunca por chat.

Para empacotar sem chave (teste, não publicável): `--no-sign`. O build sai **sem**
artefato de updater em vez de abortar.

### Reuso: não recompila o que já está pronto

Se já existe build **desta versão** no disco e nenhuma fonte mudou, `build-local.sh`
pula `npm ci`/`version:sync`/`tauri build` e vai direto ao manifesto (e à publicação,
com `--publish`). Existe para o caso de esquecer o `--publish` e não pagar um rebuild
inteiro só para subir arquivo que já existe — na prática, 2s em vez de minutos.

"O arquivo existe" **não** é o teste, e o macOS mostra por quê: o artefato do updater
é `ShvIA.app.tar.gz`, sem versão no nome. O script confere o **`sha256` do
`release.json`** (é o que dá identidade ao arquivo) e, além disso, **recompila se
qualquer fonte for mais nova que o artefato** — senão editar código sem bumpar a
versão publicaria binário velho, assinado, como se fosse a versão nova.

Para forçar: `--force`, ou apague `src-tauri/target/release/bundle`.

> **Windows:** o `build-local.ps1` ainda **não** tem o `--publish` — publique à mão
> lá, mandando o `-setup.exe`/`.msi` **e** o `release.json`, e confira o `sha256`
> pela URL pública antes de confiar.

### Windows — no code-signing certificate, by decision

The owner answered "only Apple" on 23/09/2026: Apple Developer stays, and there is **no
Windows certificate**. Windows installers go out unsigned, and SmartScreen shows its
"unknown publisher" warning. That warning is the known cost, not a defect to chase.
`build-local.ps1` still signs when `SHVIA_WIN_CERT_THUMBPRINT` or `SHVIA_WIN_PFX` is set,
and without either it builds unsigned and says so.

⚠️ **E14: the trap waiting for the day a certificate exists.** Found in the 23/09 review and
not fixable blind. In `build-local.ps1`, `tauri build` writes the updater's `.sig` first, and
**then** `Invoke-Signing` runs `signtool`, which rewrites the installer's bytes. The manifest
then carries the new sha256 with the **old** `.sig`, and every installed Windows client would
reject that update. The fix is to let Tauri sign during the bundle
(`bundle.windows.signCommand`), so the updater signature is made over the signed bytes. It is
only verifiable with a certificate and a Windows machine building (E1, 1.6.8), so it waits for
both. Until then the ordering is harmless, because nothing is signed.

## CI (GitHub Actions) — testes sim, matriz de release ainda não

**Checks have run through CI since 01–02/09/2026** (finding G-24). The current
[ci.yml](../.github/workflows/ci.yml) runs on pushes to the default branches, pull requests
and manual dispatch. On Ubuntu it prepares Node 22, stable Rust/Clippy and Tauri system
dependencies, then runs:

- Runner policy/protocol tests, the runner module-load probe, a real SDK shell-directory
  regression and the PowerShell installer checks.
- Version carriers, the pre-commit guard, document-version freshness and the installer,
  manifest, updater-key, Apple-signing, notarization, reuse, no-sidecar and publication
  contract proofs named by `package.json`.
- An installed Codex-runner smoke check through `cargo test --locked`, Clippy with
  `--locked --all-targets -- -D warnings`, a no-sidecar `cargo check --locked`, and
  `cargo-deny check` for advisories, bans, licenses and sources.

The workflow file is the complete gate list; the description above is not a claim that
the latest remote run passed. Several packaging proofs use temporary files, fake tools or
local HTTP servers. They exercise contracts without producing a signed release.

**The release matrix remains absent.** Historically, mac/win/linux `tauri-action`
packaging was removed in 0.4.6 for cost; the Enterprise/10x discussion below records the
owner's September decision rather than current account billing. Restoring CI packaging,
signing and updater publication requires a separate architecture decision. GitHub's
version-record workflow already exists and should not be confused with that missing
installer pipeline.

### Platforms (1.6.37)

**Historical gap closed by the platform workflow:** `ci.yml` runs on Ubuntu, so the code behind `cfg(target_os = "windows")` and
`cfg(target_os = "macos")` — `windows_ipc.rs`, `macos_ipc.rs`, and the gates in `lib.rs`,
`tray.rs`, `updater.rs` and others — was never compiled in CI. The Windows build did not compile
from 0.9.0 to 1.6.8, two months, and nothing said so.
[plataformas.yml](../.github/workflows/plataformas.yml) now runs on matching pushes/PRs
when `src-tauri/**` or that workflow changes, and by manual dispatch:

- **Windows GNU** — `cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings` on Ubuntu, with
  MinGW-w64: `tauri-build` compiles the icon and manifest resource with
  `x86_64-w64-mingw32-windres` (through `embed-resource`) even for a check.
- **macOS** — `cargo clippy --locked --all-targets -- -D warnings` and `cargo test --locked` on `macos-latest`, the platform the
  releases are built on.
- **Windows MSVC** (since 1.12.4) — Clippy with the same locked/all-targets/warnings gate
  and `cargo test --locked` on `windows-latest`, matching the installer toolchain.

All three Cargo jobs run from `src-tauri`. The macOS/MSVC jobs install the Codex runner
before its smoke test, and each platform prepares its own sidecar stand-in. None opens
the GUI or runs an installed update. [Windows acceptance](roteiro-validacao-windows.md)
and [microphone/clipboard acceptance](roteiro-microfone-e-colar.md) remain separate.

The owner's answer on 23/09/2026 was Enterprise, macOS included at the then-recorded 10x
multiplier; the path filter was chosen to scope checks to changes that can affect native
code. This review did not recheck plan, pricing or quota. The platform jobs compile, lint
and test; they do not package or sign, and release builds stay local.

### Dependabot (1.6.35)

`.github/dependabot.yml` watches every manifest: `cargo` in `src-tauri/`, `npm` at the root and
in `claude-runner/` and `codex-runner/`, and the workflows' `github-actions`, weekly on Monday
morning (São Paulo time). Patch and minor updates come grouped, one PR per directory; each major comes
alone. Vulnerability alerts and security updates were recorded enabled in the repository
settings on 23/09/2026; this review checked the versioned configuration, not the current
settings. The owner chose "complete" over "security + patch/minor only".

**A Dependabot PR is a proposal, never a merge.** Its commit is `Bump x from a to b`, with no
version and no CHANGELOG entry. Merging it as it is breaks the commit rule and publishes no
Release. To land one, take its change into one `X.Y.Z - ...` commit with the CHANGELOG entry
and the bump (`echo X.Y.Z > version.md && node scripts/sync-version.mjs`). **Then leave the PR
alone.** Closing it by hand works like `@dependabot close`: Dependabot does not propose that
update again. That is the wrong message when the commit took a different version, as 1.6.36
did (3.21.0, the first patched `serde_with`, instead of the proposed 3.23.0). Dependabot closes
a PR that `master` no longer needs by itself.

`dtolnay/rust-toolchain` is ignored on purpose. It is pinned to a commit of its `stable`
branch, and Dependabot would move it to the default branch, whose action needs a `toolchain`
input that `ci.yml` does not pass. Update it by hand: the SHA of `stable` and the date in the
comment.

`npm run prova:dependabot` (in CI) fails when a tracked `package.json` or `Cargo.toml` has no
entry, or when an entry points at a directory without one. A new runner directory cannot fall
out of the updates without anyone noticing.
