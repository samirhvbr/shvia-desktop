# Pacote Arch (.pkg.tar.zst) nos repos de desktop

> **Saneado em 07/08/2026.** Este arquivo descrevia o `fpm` como "a rota
> implementada" nos três repos. Para **este** repo isso deixou de ser verdade na
> 1.1.16, e a parte que virou decisão estável migrou para
> [`../docs/build.md`](../docs/build.md#arch-linux-pkgtarzst--repo-pacman) e o
> **ADR-028** — que é a convenção do `.continue/` (nota madura vira doc e sai
> daqui). O que sobrou aqui é o que continua **em aberto**: os outros dois repos.

## Contexto

Em 04/08/2026 os três repos de desktop ganharam pacote Arch pela mesma rota:
**converter o `.deb` com `fpm`** no próprio host Debian de build, sem precisar de
máquina Arch. Fazia sentido enquanto a máquina de desenvolvimento era macOS e a
de build era Debian.

Desde então o Samir passou a usar **Arch Linux**, e a premissa virou pelo avesso:
num Arch, converter `.deb` é contorno sem motivo — `makepkg` faz o pacote nativo.

## Situação por repo

| Repo | Rota hoje | Pendência |
|---|---|---|
| **SHVIA-DESKTOP** | ✅ `makepkg` + PKGBUILD + repo pacman (1.1.16) | validar numa máquina Arch — ver [`estado-atual.md`](estado-atual.md) §1 |
| **SSHVTERM-DESKTOP** (1.2.46) | `fpm` puro em `build-local.sh` | mesmo retrabalho da 1.1.16 |
| **GITHUB-DESKTOP** | `buildPacman`/fpm em `script/package.ts` | mesmo retrabalho da 1.1.16 |

Caminhos: `/home/samir/x/SSHVTERM/SSHVTERM-DESKTOP/build-local.sh` e
`/home/samir/x/GITHUB-DESKTOP/script/package.ts`.

## O que o retrabalho da 1.1.16 envolveu (o molde para os outros dois)

1. **Detecção de distro** por `/etc/os-release` (`ID` + `ID_LIKE`, para cobrir
   Manjaro/EndeavourOS e Ubuntu/Mint), escolhendo `makepkg` num Arch e mantendo
   `fpm` como fallback num Debian.
2. **PKGBUILD que reempacota o `.deb`** em vez de recompilar — mesmo binário, e
   sem levar a chave privada do updater para dentro do `makepkg`.
3. **`repo-add`** gerando o banco do repositório, para o usuário atualizar com
   `pacman -Syu` em vez de baixar arquivo à mão.
4. **O guard do auto-update** — ver abaixo, é o item que não é óbvio.
5. **(1.1.18)** O guard em **duas camadas** e o **nome único** do pacote — ver
   abaixo. Foi o que faltou na 1.1.16 e chegou quebrado ao usuário na 1.1.17.

## ⚠️ O que descobrimos e vale para os três: pacman não se auto-atualiza

Verificado no fonte do `tauri-plugin-updater` 2.10.1:

- `bundle_type()` lê um marcador que o **bundler grava no binário**; os valores
  possíveis são `Deb`, `Rpm`, `AppImage`, `Msi`, `Nsis`. **Não existe pacman.**
- `install_inner` despacha `Deb → dpkg -i`, `Rpm → rpm -U`, e **todo o resto cai
  no `install_appimage`**, que sobrescreve o executável em execução.

Como o pacote Arch sai do payload do `.deb`, o binário se diz **DEB** — e o app
tentaria `pkexec dpkg -i` **depois de baixar ~80 MB**, num sistema que não tem
dpkg. A solução em SHVIA-DESKTOP foi um marcador
(`/usr/share/shvia-desktop/instalado-por`) que o app lê para avisar
"rode `pacman -Syu`" em vez de tentar instalar.

**Qualquer um dos outros dois que ganhe pacote pacman herda esse problema**, e um
pacote sem o guard entrega ao usuário um update que falha no fim do download.

### ⚠️ E o marcador sozinho não resolve — aprendido na pele (1.1.17 → 1.1.18)

O marcador é instalado pelo PKGBUILD, então ele existe **só quando o build roda num
Arch**. A rota `fpm` num Debian não sabe do PKGBUILD e não injetava nada — e foi
justamente o pacote convertido que subiu na 1.1.17. No Arch, ele fez o que o ADR-028
previa e o build avisava por escrito: baixou o `.deb` e morreu no `dpkg`, com uma
mensagem de erro falando de senha de administrador.

Quem copiar isto para SSHVTERM-DESKTOP ou GITHUB-DESKTOP precisa das **duas** peças,
não só da primeira ([ADR-029](../docs/decisoes.md#adr-029--o-guard-do-auto-update-não-pode-depender-do-empacotamento)):

1. **A rota `fpm` extrai o payload, injeta o marcador e empacota com `-s dir`** —
   converter o `.deb` direto não deixa injetar arquivo nenhum.
2. **Um guard no app que não dependa de arquivo:** bundle se dizendo `deb` sem `dpkg`
   no sistema (ou `rpm` sem `rpm`) já é impedimento — avisa e não baixa. É o que
   segura pacote empacotado errado, hoje e no futuro.

E um detalhe barato de esquecer: **o `pkgname` tem de ser o mesmo nas duas rotas.** O
`fpm` herda o nome do pacote Debian (aqui, `shv-ia`) e o PKGBUILD usa o seu
(`shvia-desktop`) — dois nomes para o mesmo app deixam o `pacman -Syu` de quem
instalou um sem ver o outro, parado e sem erro na tela. Trocar o nome depois exige
`replaces`/`conflicts` nas duas rotas para migrar quem já instalou.

## Bloqueio conhecido no SSHVTERM

O `.pkg.tar.zst` de lá está fora do `--publish` por um motivo diferente do nosso:
o `releases:add` do site **recusa a extensão** (`ALLOWED_EXTENSIONS` no
SSHVTERM-WEB). Enquanto isso não mudar, a distribuição do pacote Arch daquele
repo é manual — mesmo com o build gerando o arquivo.

## Lembrete

O **AppImage** que os três já geram roda no Arch hoje, sem depender de nada
disso — e é o único formato Linux que **se auto-atualiza de verdade**, porque o
arquivo é do usuário.
