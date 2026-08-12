# Changelog

Entradas no formato da mensagem de commit (`versão - comentário`, AGENTS.md),
mais recente primeiro. É daqui que a skill COMMITTER tira a mensagem (AGENTS.md §PS).

## 1.1.18 - Conserta o update no Arch: o pacote convertido sai com o marcador, e o app deixa de depender só dele

O pacote pacman publicado na 1.1.17 (`shv-ia-1.1.17-1-x86_64.pkg.tar.zst`) foi
gerado por `fpm` numa máquina Debian, e portanto **sem** o marcador
`/usr/share/shvia-desktop/instalado-por`. No Arch, o app não sabia que tinha vindo
do pacman: pediu `?bundle=deb`, recebeu o `.deb`, chamou `pkexec dpkg -i` — que não
existe lá — e falhou no fim do download, exibindo um conselho sobre senha de
administrador que não tinha nada a ver com a causa. O ADR-028 previa esse cenário
por escrito e o build avisava no terminal; nada disso impediu o pacote de subir.

- **Guard novo no `updater.rs`, independente do empacotamento:** bundle se dizendo
  `deb` sem `dpkg` no sistema (ou `rpm` sem `rpm`) já é motivo para não oferecer o
  download. Ao contrário da leitura do marcador, aqui **não** há fail-open a
  preservar — não existe sistema onde essa instalação se atualize, então o download
  terminaria em erro de qualquer jeito. O marcador continua e tem precedência,
  porque ele permite a instrução exata (`sudo pacman -Syu`) em vez de um palpite.
  A busca do comando cobre o `PATH` e `/usr/sbin:/usr/bin:/sbin:/bin`, já que o
  plugin chama o instalador por `pkexec`/`sudo`, que montam PATH próprio.
- **A rota `fpm` do `build-local.sh` deixou de converter o `.deb` direto:** agora
  extrai o payload (`dpkg-deb -x`, ou `bsdtar` onde não houver), injeta o marcador e
  empacota com `-s dir`. Converter não deixava injetar arquivo nenhum — era a raiz
  do problema, não um detalhe de implementação.
- **Um nome só para o pacote: `shvia-desktop`.** O `fpm` vinha publicando `shv-ia`
  (nome herdado do pacote Debian) e o PKGBUILD, `shvia-desktop` — dois nomes para o
  mesmo app deixariam o `pacman -Syu` de quem instalou um cego para o outro, parado
  e sem erro na tela. `replaces`/`conflicts` nas duas rotas migram quem já instalou
  o `shv-ia`.
- **O `.pkg` de nome legado é removido do bundle dir antes de empacotar.** Ele tem a
  versão corrente no nome, então o `release-manifest.mjs` o aceitaria e o
  `find … | head -1` do `repo-add` poderia publicá-lo em vez do pacote novo.
- **ADR-029** conta o caso e revoga a consequência do ADR-028 que aceitava pacote sem
  marcador. `docs/build.md` perde o "conversão best-effort", e o `.continue/arch.md`
  — o molde para SSHVTERM-DESKTOP e GITHUB-DESKTOP — passa a exigir as duas camadas:
  quem copiar só o marcador herda esta falha.
- **Validado:** `cargo test` 48/48 (6 testes novos, incluindo o caso real desta versão
  e o contra-teste de que Debian/Fedora seguem atualizando), o novo conferido por
  reversão; `cargo clippy --all-targets` limpo; `bash -n`; e o bloco de empacotamento
  **executado** num sandbox com um `shv-ia-1.1.17` plantado (o `.deb` no disco ainda
  é o da 1.1.17) — saiu `shvia-desktop-1.1.17-1-x86_64.pkg.tar.zst` com o marcador dentro,
  `%REPLACES%`/`%CONFLICTS%` no `shvia.db` e o pacote legado apagado. Segue **não
  validado** o caminho `makepkg` (esta máquina é Debian 13) — pendência §1 do
  `.continue/estado-atual.md`, aberta desde a 1.1.16.

## 1.1.17 - Saneia o .continue: estado-atual descrevia a 0.8.0 com o repo na 1.1.16

- `estado-atual.md` reconstruído a partir do `git log` real. Ele listava como
  pendência coisa entregue há semanas — offline v2 (1.1.13), updater (1.0.0),
  tray (1.1.0), diagnóstico (1.1.4) e o `anna` no instalador (0.18.0) — e quem
  lesse ia refazer trabalho pronto. O que não deu para confirmar (microfone e
  Ctrl+V de imagem no WebKitGTK) ficou marcado como **não reavaliado**, não como
  pendente: afirmar que continua quebrado sem testar seria inventar status.
- Entram as duas pendências reais que a 1.1.16 criou: o caminho `makepkg` nunca
  rodou (foi escrito numa máquina Debian, sem makepkg/bsdtar/repo-add), e o
  `release-manifest.mjs` derruba a entrada do `.pkg` do manifesto quando se
  publica de uma máquina Linux que não gera pacote Arch.
- Dois links mortos consertados: `SAMIR-v1.md` (removido na 1.1.6) e a âncora
  `#decisões-em-aberto`, que no escopo é `#7-decisões-em-aberto`.
- `arch.md` encolhido para o que segue em aberto — SSHVTERM-DESKTOP e
  GITHUB-DESKTOP ainda na rota `fpm`. A parte que virou decisão estável está em
  docs/build.md e no ADR-028, que é a convenção da pasta (nota madura vira doc e
  sai daqui). Fica registrado ali que qualquer repo que ganhe pacote pacman
  herda o problema do auto-update, e sem o marcador entrega um update que falha
  no fim do download.
- `README.md` do `.continue` passa a listar o `arch.md`, que existia desde 04/08
  sem estar na tabela.

## 1.1.16 - Build nativo no Arch (makepkg + repo pacman) e o app para de tentar auto-update lá

- `build-local.sh` detecta a distro por `/etc/os-release` (`ID` + `ID_LIKE`, cobrindo
  Manjaro/EndeavourOS e Ubuntu/Mint). O preflight passa a sugerir `pacman -S` no Arch —
  antes mandava `apt-get install`, comando que não existe lá.
- Pacote Arch agora sai por `makepkg` com o novo `packaging/arch/PKGBUILD` quando o build
  roda num Arch; o `fpm` convertendo o `.deb` fica como fallback para máquina Debian.
  O PKGBUILD reempacota o `.deb` em vez de recompilar — mesmo binário, e sem levar a
  chave privada do updater para dentro do makepkg.
- `--publish` sobe também o banco do repositório (`repo-add`), então o usuário recebe
  versão nova com `pacman -Syu` em vez de baixar arquivo à mão.
- O app **não tenta mais se auto-atualizar quando foi instalado por pacman** (ADR-028):
  o `tauri-plugin-updater` não tem instalador de pacman e rodaria `pkexec dpkg -i` depois
  de baixar ~80 MB. O pacote instala `/usr/share/shvia-desktop/instalado-por` e o
  `updater.rs` lê o arquivo: havendo versão nova, avisa e manda rodar `pacman -Syu`.
- Revoga a nota da 1.1.15: o `.pkg.tar.zst` passa a entrar no `release.json` como artefato
  de download (sem `.sig`, e o endpoint ignora artefato sem assinatura — logo, inerte para
  o auto-update).
- **Não validado:** o caminho `makepkg` de ponta a ponta. A máquina onde isto foi escrito é
  Debian 13, sem `makepkg`/`bsdtar`/`repo-add` — precisa de uma passada num Arch.

## 1.1.15 - build-local.sh (Linux) ganha o pacote Arch (.pkg.tar.zst) via fpm

- O bundler do Tauri 2.x só gera deb/rpm/AppImage no Linux; o pacote pacman agora
  sai convertendo o .deb com o fpm na própria máquina Debian (sem host Arch).
  Deps mapeadas para os nomes do Arch (webkit2gtk-4.1, gtk3, libayatana-appindicator
  — o app usa tray-icon). Best-effort: sem fpm no PATH, avisa como instalar e segue.
- Fora do release.json/publish por enquanto: o fpm não gera o .sig do updater e o
  endpoint do ShvIA trata artefato sem assinatura como inexistente — entrar no
  manifesto quebraria a verificação da publicação. Distribuição manual por ora.
