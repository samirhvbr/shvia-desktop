# ShvIA Desktop — Estado e Continuidade

> **Ler primeiro.** Notas de continuidade: o que está **em aberto**. O que já
> está implementado mora em [`../docs/funcionalidades.md`](../docs/funcionalidades.md),
> e o porquê das decisões em [`../docs/decisoes.md`](../docs/decisoes.md).
> Última atualização: **07/08/2026** (versão 1.1.16).

> ⚠️ **Saneado em 07/08/2026.** Este arquivo estava descrevendo a **0.8.0** —
> catorze versões atrás — e listava como pendência coisa entregue há semanas
> (offline v2, updater, tray, anna no instalador). Quem lesse ia refazer trabalho
> pronto. As seções abaixo foram reconstruídas a partir do `git log` real, e o
> que não deu para confirmar está marcado como **não reavaliado**, não como
> pendente.

## Onde estamos

**1.1.16** — o app se auto-atualiza (ADR-022), tem bandeja nos 3 SOs (ADR-024),
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
e ligou o repositório pacman no `--publish`. **Nada disso foi executado:** foi
escrito numa máquina Debian, que não tem `makepkg`, `bsdtar` nem `repo-add`.

O que precisa de uma passada **numa máquina Arch**:

- `./build-local.sh` até o fim — o `.pkg.tar.zst` sai em `bundle/pacman/`?
- `repo-add` gera `shvia.db`/`shvia.files` e os links viram arquivo de verdade?
- `sudo pacman -U` instala, e o marcador
  `/usr/share/shvia-desktop/instalado-por` aparece com o conteúdo `pacman`?
- Com o marcador presente, o app **avisa para rodar `pacman -Syu`** em vez de
  tentar baixar o update (é o ponto do ADR-028).

Contexto e o porquê: [ADR-028](../docs/decisoes.md) e
[`../docs/build.md`](../docs/build.md#arch-linux-pkgtarzst--repo-pacman).

### 2. `release-manifest.mjs` derruba o `.pkg` do manifesto

O merge do `release.json` é **por plataforma** — `manifesto.platforms[PLATAFORMA]`
é substituído inteiro. Com duas máquinas Linux (uma Arch que gera `.pkg`, uma
Debian que não gera), publicar da Debian **apaga do manifesto** a entrada que a
Arch publicou.

É a mesma classe do bug que o `fetch_remote_manifest` já resolve entre
macOS/Windows/Linux, só que agora **dentro** do linux. O repositório pacman em si
sobrevive (o `shvia.db` é arquivo separado); o que se perde é a entrada no
manifesto. Correção estimada em ~10 linhas, ainda não feita.

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
