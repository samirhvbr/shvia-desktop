# Changelog

Entradas no formato da mensagem de commit (`versão - comentário`, AGENTS.md),
mais recente primeiro. É daqui que a skill COMMITTER tira a mensagem (AGENTS.md §PS).

## 1.1.23 - Reempacota o anna 0.11.0: o teto de voltas do Modo Code deixa de matar turno legítimo na volta 25

- Rebuild sem mudança de código próprio: o estágio D5 empacota o `anna` do
  PATH na hora do build, e o `resolve_bin` do app prefere o binário bundlado —
  então a única forma de o Modo Code do desktop ganhar o teto novo é uma
  versão nova do app. A 1.1.22 (nunca publicada) carregava o anna 0.10.1, cujo
  teto fixo de 25 voltas matou um turno real de 26 ferramentas no projeto KIDS
  em 19/08 ("teto de voltas atingido — refine a pergunta").
- O anna 0.11.0 (SHVIA-CODE 1db563b) traz `max_voltas` configurável (env
  `SHVIA_MAX_VOLTAS` > config.toml > default 100) e, no host NDJSON, encerra o
  turno com a sessão viva — responder "continua" retoma de onde parou. No
  macOS o config.toml do anna fica em `~/Library/Application Support/shvia-code/`.

## 1.1.22 - O bump ganha prova: os seis portadores concordam, ou o build cai

- **A terceira ocorrência do bump pela metade.** `scripts/sync-version.mjs`
  sincronizava os portadores de versão a partir do `version.md` e **nunca
  falhava**: arquivo ausente e padrão de versão não encontrado eram `console.warn`
  \+ `continue`. Um portador que parasse de casar com o regex sairia da sincronia
  **para sempre**, com o build verde e ninguém sabendo.
- **O histórico que justifica:** 1.1.18→1.1.19 deixou os manifestos atrás; a
  1.1.19 existia justamente para consertar isso; e na 1.1.20 o `version.md` e o
  CHANGELOG foram na frente dos manifestos e **travaram a validação do `makepkg`**.
  Três vezes é padrão, não descuido.
- **O que muda:** o script agora **prova** o que fez — ao fim reconfere os cinco
  portadores contra o `version.md` e sai `exit 1` se algum discordar, nomeando
  quais. `npm run prova:bump` faz a mesma conferência **sem escrever**, para rodar
  antes de commitar: o build já sincronizava, e o que passava era a árvore
  **commitada**.
- **Reusa o mesmo par (regex, substituição) da sincronia** em vez de um segundo
  padrão para ler versão — segunda implementação é a que diverge calada.
- **Por que falhar e não avisar:** é a mesma regra do piso de versão do `anna`
  (1.1.21) e do `STANDING_ATIVO` nascer em `0`. Aviso em log de build é o que
  ninguém lê. Conferido por reversão nas duas classes de falha — portador em
  versão diferente e padrão que deixou de casar —, as duas saem `exit 1`.

## 1.1.21 - O empacotamento do `anna` ganha piso de versão: motor velho derruba o build em vez de virar release

- **O caso (19/08):** sessão longa do Modo Code travava no 422 do gateway
  (`messages` tem `max:200`). O `anna` 0.10.0 (18/08) compacta o histórico antes
  de enviar e resolve — mas o app saía com o `anna` que estivesse no PATH da
  máquina de build, e ali havia um **0.8.4 de julho**. Resultado: app na última
  versão, mensagem de erro mandando "atualize o app", e o app já atualizado. O
  que estava velho era o sidecar **dentro** dele.
- **O que muda:** `scripts/stage-anna.mjs` passa a exigir `ANNA_MINIMO` (0.10.0)
  e **derruba o build** abaixo disso, nomeando a versão achada e o comando de
  conserto. Ausência do `anna` continua não sendo erro — lá o Modo Code fica
  declaradamente indisponível; aqui ficaria disponível e quebrado, que é pior.
  É a mesma regra que o script já aplicava a binário que não responde
  `--version`, estendida a um caso a mais.
- **Por que não bastava o aviso:** o risco estava previsto no cabeçalho do
  próprio script desde o começo, com a mitigação *"imprime a versão para alguém
  conferir depois"*. O número estava no log do build; o log é que não tem leitor.
  Aviso perde para gesto — o mesmo argumento do `STANDING_ATIVO` nascer em `0`.

## 1.1.20 - Ícone do app entra na marca atual do ShvIA (o "S" azure) — o update parava de "trocar o ícone" porque o repo nunca trocou

- O usuário via o ícone antigo ("AI" + seta Blue3 sobre navy, design 0.3.1 de
  08/07) voltar a cada atualização e parecia bug do updater. NÃO era: o updater
  entrega exatamente o que o repo builda, e `src-tauri/icons/` + `brand/` nunca
  receberam a marca nova — o "S" azure existia só no site (favicon.svg do
  SHVIA-WEB, marca oficial de 25/07, quando o ShvIA ganhou identidade própria).
- Fonte nova `brand/shvia-desktop-icon-1024.png`: o favicon.svg oficial (bloco
  azure #34B3EC, rx≈23%, S em #0B0F17) embrulhado na grade de ícone do macOS
  (conteúdo 824×824 centrado em canvas 1024 transparente, margens 100px) e
  renderizado via qlmanage. Conjunto inteiro regenerado com `tauri icon`
  (icns/ico/png/Square*); os android/ e ios/ gerados foram descartados (mobile
  é outro repo). O tray de menu bar segue o template "AI" da 1.1.14 — decisão
  deliberada, não foi tocado.
- O ícone novo chega ao usuário na PRÓXIMA release publicada (o build embute o
  .icns). No Dock/Finder o macOS pode segurar cache de ícone da versão antiga;
  o app aberto e o alternador ⌘Tab mostram o novo imediatamente.

## 1.1.19 - O manifesto passa a mesclar por artefato, e o pacote do Arch para de sumir quando a Debian publica

O merge do `release.json` era **por plataforma**: `platforms[PLATAFORMA]` trocava
inteiro a cada build. Isso assumia "uma máquina por plataforma", e o Linux deixou de
caber nessa suposição na 1.1.16 — a máquina Arch gera o `.pkg.tar.zst` por `makepkg`
(ADR-028) e a Debian não gera nenhum. Publicar da Debian **apagava do manifesto** o
pacote pacman que a Arch tinha publicado.

É a mesma classe do bug que o download-antes-de-gerar do `--publish` já resolve entre
macOS/Windows/Linux, um nível abaixo — e pior de enxergar, porque as duas máquinas
escrevem na mesma chave `linux` e o manifesto resultante parece íntegro. O repositório
pacman em si sobrevivia (o `shvia.db` é arquivo separado); o que se perdia era a
entrada no manifesto, e com ela o `sha256` de quem baixa o pacote direto.

- **Mescla dentro da plataforma, chaveada pelo nome do arquivo.** O build atual sempre
  vence — artefato regerado substitui o hash antigo em vez de conviver com ele. Versão
  nova continua descartando o manifesto inteiro, então nada de outra release se acumula.
- **O log diz o que foi PRESERVADO de outra máquina.** Sem isso o operador veria "3
  artefatos" e um `release.json` com quatro, sem saber de onde veio o quarto — silêncio
  é o que fez este bug durar.
- **O aviso de "nenhum artefato assinado" ficou honesto.** Ele afirmava que o
  auto-update não ofereceria a versão; com a mescla isso pode ser falso, porque outra
  máquina já publicou artefato assinado da mesma release. A frase forte agora só sai
  quando o manifesto inteiro está sem assinatura.

Conferido com fixture das duas máquinas (Arch publica os 4 → Debian publica 3 → o
`.pkg` continua lá), com rehash e com troca de versão; e o comportamento antigo foi
reproduzido no script anterior para provar que o teste tem régua. Fecha a §2 das
pendências ativas do `.continue/estado-atual.md`.

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
