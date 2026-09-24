# Build & Empacotamento — ShvIA Desktop

Como gerar os instaladores do app. **O build de release é LOCAL** (na sua máquina) **por
decisão de arquitetura** — não por falta de cota: a conta é GitHub Enterprise (50.000
min/mês), mas continua valendo a ressalva de custo por trás da decisão original: runners
**macOS consomem minutos a 10x** em Actions, então empacotar as 3 plataformas via matriz
`tauri-action` ainda pesa desproporcionalmente no orçamento, mesmo com Enterprise. Build
local incremental, com o cache do cargo, também é mais rápido para iteração. O ShvIA
Desktop é **shell fino** (Tauri 2, sem sidecar), então é direto: `tauri build`. (Testes,
diferente do build de release, já rodam via CI — ver seção abaixo.)

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

- **Linux** — `.deb` + `.AppImage` (+ `.rpm`), via `targets: "all"` do `tauri.conf.json`,
  **+ `.pkg.tar.zst`** (Arch), que não vem do bundler do Tauri — ver
  [Arch Linux](#arch-linux-pkgtarzst--repo-pacman).
- **macOS** — `.dmg` + `.app.tar.gz`.
- **Windows** — `.msi` (WiX) + `-setup.exe` (NSIS).

## Pré-requisitos

- **Comum:** Node 20.19+ or 22.12+ (what `vite` 8 requires, since 1.6.49), Rust (`rustup default stable`).
- **Linux (Debian/Ubuntu):**
  `sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf`
- **Linux (Arch):**
  `sudo pacman -S --needed base-devel webkit2gtk-4.1 curl wget file openssl libayatana-appindicator librsvg xdotool patchelf`
  (`base-devel` traz `makepkg` + `fakeroot`, que empacotam o `.pkg.tar.zst`)
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
which OS: the server's `release.json` is overwritten by each publish. The repository is
private, so the assets are too; the cost is the storage, accepted by the owner on 23/09/2026.

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
3. **Verifies.** At the end it runs `codesign --verify`, `spctl` (Gatekeeper) and
   `stapler validate`. `--publish` refuses a build that fails them (1.6.22).

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
4. Otherwise, the build is signed but not notarized, and `--publish` refuses it.

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

**Testes/lint rodam via CI** desde 01–02/09/2026 (`.github/workflows/ci.yml`, achado G-24).
A **matriz de build/release** (mac/win/linux via `tauri-action` + Release + updater) foi
**removida na 0.4.6 por custo** e segue de fora por decisão, mesmo com a conta em GitHub
Enterprise (50.000 min/mês) — o motivo que resta é o multiplicador: runners **macOS contam
10x** os minutos, o que pesa numa matriz de release rodando a cada tag. O SHVTERM tem a
referência pronta (`.github/workflows/main.yml`: matriz mac/win/linux via `tauri-action` +
Release + updater). Se compensar trazer essa matriz de volta (releases assinados/auto-update
centralizados via CI), é decisão de arquitetura a tomar separadamente — não consequência
automática do upgrade de plano.

### Platforms (1.6.37)

`ci.yml` runs on ubuntu, so the code behind `cfg(target_os = "windows")` and
`cfg(target_os = "macos")` — `windows_ipc.rs`, `macos_ipc.rs`, and the gates in `lib.rs`,
`tray.rs`, `updater.rs` and others — was never compiled in CI. The Windows build did not compile
from 0.9.0 to 1.6.8, two months, and nothing said so. `.github/workflows/plataformas.yml` runs
when `src-tauri/**` changes:

- **Windows** — `cargo clippy --target x86_64-pc-windows-gnu -D warnings` on ubuntu, with
  MinGW-w64: `tauri-build` compiles the icon and manifest resource with
  `x86_64-w64-mingw32-windres` (through `embed-resource`) even for a check.
- **macOS** — `cargo clippy -D warnings` and `cargo test` on `macos-latest`, the platform the
  releases are built on.

Minutes are not the limit (the owner's answer on 23/09/2026: Enterprise), macOS included at 10x;
the path filter is what keeps the cost to changes that can break these builds. It compiles,
lints and tests. It does not package or sign; release builds stay local.

### Dependabot (1.6.35)

`.github/dependabot.yml` watches every manifest: `cargo` in `src-tauri/`, `npm` at the root and
in `claude-runner/` and `codex-runner/`, and the workflows' `github-actions`, weekly on Monday
morning (São Paulo time). Patch and minor updates come grouped, one PR per directory; each major comes
alone. Vulnerability alerts and Dependabot security updates are switched on in the repository
settings (23/09/2026). The owner chose "complete" over "security + patch/minor only".

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
