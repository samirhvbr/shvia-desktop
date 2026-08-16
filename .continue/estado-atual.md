# ShvIA Desktop — Estado e Continuidade

> **Ler primeiro.** Notas de continuidade: o que está **em aberto**. O que já
> está implementado mora em [`../docs/funcionalidades.md`](../docs/funcionalidades.md),
> e o porquê das decisões em [`../docs/decisoes.md`](../docs/decisoes.md).
> Última atualização: **15/08/2026** (versão 1.1.19).

> ⚠️ **Saneado em 07/08/2026.** Este arquivo estava descrevendo a **0.8.0** —
> catorze versões atrás — e listava como pendência coisa entregue há semanas
> (offline v2, updater, tray, anna no instalador). Quem lesse ia refazer trabalho
> pronto. As seções abaixo foram reconstruídas a partir do `git log` real, e o
> que não deu para confirmar está marcado como **não reavaliado**, não como
> pendente.

## Onde estamos

**1.1.19** — o app se auto-atualiza (ADR-022), tem bandeja nos 3 SOs (ADR-024),
diagnóstico próprio (ADR-025), diálogo de arquivo nativo (ADR-027) e empacota
para Linux (deb/rpm/AppImage/**pacman**), macOS (dmg) e Windows (msi/nsis). O
`anna` viaja dentro do instalador desde a 0.18.0 — o Modo Code não tem mais
pré-requisito externo.

As fases F1 e F2 estão entregues. O detalhe do que funciona está em
[`../docs/funcionalidades.md`](../docs/funcionalidades.md); como buildar, em
[`../docs/build.md`](../docs/build.md).

## Pendências ativas

### 1. O caminho `makepkg` (Arch) nunca rodou de verdade — 1.1.16

A 1.1.16 trocou a geração do pacote Arch de `fpm` (conversão do `.deb`) para
`makepkg` nativo com [`../packaging/arch/PKGBUILD`](../packaging/arch/PKGBUILD),
e ligou o repositório pacman no `--publish`. **O `makepkg` nunca foi executado:**
foi escrito numa máquina Debian.

> **Atualizado em 12/08/2026 (1.1.18).** Duas correções ao texto original: esta
> máquina **tem** `bsdtar` e `repo-add` (o segundo vem em
> `pacman-package-manager`), e os dois já rodaram — o `shvia.db` do repo sai
> daqui. O que continua sem rodar é só o `makepkg` + PKGBUILD.
>
> E a parte "sem o marcador, o app tenta o update e falha" **saiu do papel**: o
> pacote publicado na 1.1.17 foi o convertido por `fpm`, e no Arch do Samir o
> update falhou no fim do download. Consertado na 1.1.18 — a rota `fpm` passou a
> injetar o marcador, e o app ganhou um guard que não depende dele
> ([ADR-029](../docs/decisoes.md#adr-029--o-guard-do-auto-update-não-pode-depender-do-empacotamento)).

> **Atualizado em 15/08/2026, 22:20 — a árvore agora está pronta para essa passada.**
> O commit da 1.1.19 (`437357e`) subiu `version.md` e `CHANGELOG.md` mas **deixou os
> cinco portadores de versão em 1.1.18** (`package.json`, `package-lock.json`,
> `Cargo.toml`, `Cargo.lock`, `tauri.conf.json`) — o bump da 1.1.18 (`dace855`) tinha
> tocado os cinco, é o padrão do repo. Quem rodasse o `makepkg` antes disso ia validar
> um pacote carimbado **1.1.18**, e o teste do auto-update mediria a coisa errada
> justamente na versão que conserta o manifesto. Completado em `964b6e9`.

> **RODOU em 16/08/2026 — num Arch de verdade, e a mecânica passou.** Não era
> preciso máquina Arch: um container `archlinux` descartável (pacman 7.1.0, makepkg
> 7.1.0) serve, com o repo montado **read-only** e tudo acontecendo fora dele.
> O payload usado foi o `.deb` **1.1.18** que está no disco, com o `version.md` da
> cópia ajustado junto — pacote internamente honesto, e o caminho padrão do PKGBUILD
> exercitado inteiro, inclusive a derivação do nome do `.deb` a partir do `pkgver`.
> **O que a mecânica NÃO cobre é a versão:** validar o artefato 1.1.19 exige um
> `tauri build` 1.1.19 de verdade.

| # | Ponto do checklist | Resultado |
|---|---|---|
| 1 | `makepkg` produz o `.pkg.tar.zst`? | ✅ `shvia-desktop-1.1.18-1-x86_64.pkg.tar.zst`, 6,48 MB. ⚠️ o `makepkg` confere as **deps de runtime** antes de empacotar e aborta sem elas — num Arch de verdade estão instaladas (o `build-local.sh` as lista nos pré-requisitos); no container foi preciso `--nodeps` |
| 2 | `pacman -U` instala e o marcador aparece? | ✅ instala; `/usr/share/shvia-desktop/instalado-por` = `pacman`; binário de 7,99 MB e o sidecar `usr/bin/anna` vieram no payload |
| 3 | O app avisa para rodar `pacman -Syu`? | 🟡 **metade provada**: o contrato bate — [`updater.rs:76`](../src-tauri/src/updater.rs) lê exatamente `/usr/share/shvia-desktop/instalado-por` e compara com `"pacman"`, que é o que o PKGBUILD escreve. A outra metade (o aviso na tela) é comportamento de app rodando e continua sem medição |
| 4 | `replaces=('shv-ia')` migra? | ✅ **pelo `-Syu`**, que é o caminho real: *"Replace shv-ia with shvia/shvia-desktop? [Y/n]"*, default Y, o antigo sai, o marcador sobrevive. ⚠️ **pelo `-U` não migra** — `replaces` só vale em transação de sync; ali o pacman vê só o `conflicts` e aborta com `unresolvable package conflicts detected`. Semântica do pacman, não defeito nosso, mas é beco sem saída para quem instala pelo arquivo → registrado em [`build.md`](../docs/build.md#arch-linux-pkgtarzst--repo-pacman) |

**O que sobrou de verdade:** um build 1.1.19 (para carimbar o artefato publicável) e
o aviso na tela do ponto 3. A pergunta *"o `makepkg` funciona?"*, que era a razão de
este item existir, está respondida.

Contexto e o porquê: [ADR-028](../docs/decisoes.md), ADR-029 e
[`../docs/build.md`](../docs/build.md#arch-linux-pkgtarzst--repo-pacman).

### ~~2. `release-manifest.mjs` derruba o `.pkg` do manifesto~~ — ENTREGUE na 1.1.19

O merge passou a ser **por artefato** dentro da plataforma, chaveado pelo nome do
arquivo: o `.pkg` da Arch sobrevive a um publish da Debian, o build atual vence no
rehash, e o log diz o que foi preservado de outra máquina. Versão nova continua
descartando o manifesto inteiro. O porquê e o comportamento estão em
[`../docs/build.md`](../docs/build.md) (§`--publish`, item 2); o detalhe de
implementação, no `CHANGELOG.md` da 1.1.19.

### 3. WebKitGTK no Linux — **não reavaliado desde julho/2026**

Os dois vieram da 0.8.0 e **não há commit indicando conserto**, mas também não
foram testados de novo. São limitações do WebKitGTK (ADR-008), não do nosso
código:

1. **Microfone não captura.** O shell habilita `getUserMedia` e o WebKitGTK
   enumera o device, mas a captura efetiva não vai.
2. **Ctrl+V de imagem não cola.** Texto funciona; o WebKitGTK não expõe a imagem
   do clipboard à página.

**Caminhos:** validar em macOS (WKWebView) e Windows (WebView2), que tendem a
suportar; se virarem must-have no Linux, o fallback é Electron (ADR-003/006/008).
Decisão de produto, não de engenharia.

### 4. Windows — validação ao vivo

A ponte do Modo Code no Windows (WebView2) entrou na ADR-010 e o `cargo check`
cruzado passa, mas **o loop nunca foi validado numa máquina Windows real**. O
sub-item "colocar o `anna.exe` no PATH" **caiu**: desde a 0.18.0 o `anna` vai
dentro do instalador.

Falta também o **Authenticode EV** (o macOS já tem Developer ID + notarização +
staple no `build-local.sh`).

## Decisões em aberto (confirmar com o time)

- [ ] **Online-only é aceitável** como propriedade de produto? Toda a arquitetura
      de casca fina depende disso. Ver [escopo](escopo-projeto.md#7-decisões-em-aberto).
- [ ] **Verba + dono** do cert EV Windows (~US$300–600/ano) e Apple Developer
      (US$99/ano). A **rotação da chave do updater** ganhou custo real na 1.0.0:
      a pubkey fica compilada no binário, então rotacionar exige que todo install
      existente seja reinstalado à mão. Definir dono e periodicidade.
- [ ] **Funções idênticas ao web** ou haverá telas desktop-only?
- [ ] **URL de DEV** do ShvIA, além da produção `ai.shvia.org`.

## Ponteiros

- O que já funciona: [`../docs/funcionalidades.md`](../docs/funcionalidades.md)
- Build/empacotamento: [`../docs/build.md`](../docs/build.md)
- Arquitetura: [`../docs/arquitetura.md`](../docs/arquitetura.md) · ADRs:
  [`../docs/decisoes.md`](../docs/decisoes.md)
- Escopo/fases: [`escopo-projeto.md`](escopo-projeto.md)
- Pacote Arch nos repos irmãos: [`arch.md`](arch.md)
