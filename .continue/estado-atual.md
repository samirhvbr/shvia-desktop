# ShvIA Desktop — Estado e Continuidade

> **Ler primeiro.** Notas de continuidade: o que está **em aberto**. O que já
> está implementado mora em [`../docs/funcionalidades.md`](../docs/funcionalidades.md),
> e o porquê das decisões em [`../docs/decisoes.md`](../docs/decisoes.md).
> Last updated: **23/09/2026** (version 1.6.43).

> ⚠️ **Saneado em 02/09/2026** (achado F-22): descrevia a **1.1.34** com o repo em 1.4.3.
> Foi o **terceiro** saneamento manual deste arquivo pelo mesmo motivo — daí a régua
> `scripts/prova-frescor-da-doc.mjs`, que falha quando a distância reabre em vez de esperar
> alguém reparar. Instrução não conserta o que já falhou três vezes.
>
> O que entrou desde a 1.1.34 está em [`../docs/funcionalidades.md`](../docs/funcionalidades.md)
> §"Phase 3": cerca de pastas autorizadas (F-12), política do runner (F-13/ADR-032),
> `gitDiff`/`readFile` na ponte, publish por `b3sys`, CI em todo push, janela de
> `target=_blank` pela função canônica (F-15) e advisories do Rust medidos (F-31).
>
> ⚠️ **Saneado antes em 07/08/2026.** Este arquivo estava descrevendo a **0.8.0** —
> catorze versões atrás — e listava como pendência coisa entregue há semanas
> (offline v2, updater, tray, anna no instalador). Quem lesse ia refazer trabalho
> pronto. As seções abaixo foram reconstruídas a partir do `git log` real, e o
> que não deu para confirmar está marcado como **não reavaliado**, não como
> pendente.

## Onde estamos

**23/09/2026 — 1.6.8 to 1.6.34: the robustness sweep (block E), then the owner's answers
(block F).** 1.6.8 made the Windows build compile again; 1.6.9–1.6.11 gave Linux a working Quit,
fixed the Changes tab for paths with accents and spaces, and ended a window's agent when its page
reloads. 1.6.12–1.6.16 closed the Claude engine's side of the Code-mode approval boundary —
must-ask cards go out as `policy: "always"` and survive the page's Auto mode, the read fence
resolves `~` and symlinks, destructive commands and the shell's network/secret paths always ask;
the page's side landed in SHVIA-WEB 2.110.452. 1.6.17–1.6.19 hardened both runners and moved slow
bridge work off the UI thread with deadlines; 1.6.20–1.6.25 hardened `build-local.sh --publish`
(manifest read, signing keys, the Apple signature, bundle reuse, packaging without `anna`, the
runner installer); 1.6.26–1.6.31 fixed the Claude login generation, stalled update checks, huge
file reads, engine shutdown and the IPC origin checks (on-prem server on Windows, loopback only
by port). **All 19 open owner decisions were answered on the panel on 23/09**, and block F carries
them out: 1.6.32 (the Claude Code profile follows the latest Opus), 1.6.33 (a pre-commit hook
refuses a half-bumped commit — the 1.6.3 guard), 1.6.34 (`--publish` refuses a build input that
is not committed), 1.6.35–1.6.36 (Dependabot on every manifest, the repository's alerts on, and
the first alert closed), 1.6.37 (Windows and macOS compiled in CI), 1.6.38 (a native dialog
before the Claude login), 1.6.39 (notarization by an App Store Connect API key), 1.6.40 (the
updater key rotation prepared), 1.6.41 (`code_bridge.rs` split into seven modules), 1.6.42
(`pre-push` regenerated from repodocs), 1.6.43 (the product answers recorded — see *Decisões em
aberto* below). Each version's detail is in [`../CHANGELOG.md`](../CHANGELOG.md).

**23/09/2026 — 1.6.3 to 1.6.7, and where the open questions live now.** 1.6.3 finally left the
disk (its first push was half a bump — the third time; the guard is an open owner decision).
1.6.4–1.6.5 made `cargo deny` agree with `cargo audit` and check licenses, bans and sources, not
only advisories; 1.6.6 attaches the installers to the GitHub Release on `--publish` (f121).
**The owner's open decisions are no longer kept here:** they are answered on the panel
[Fila aberta do ShvIA-Desktop](https://claude.ai/artifact/GsGYV37Y4YB5sEU8GoNYLB), and the
executable queue is `.loop/QUEUE.md` (local to the working copy, not versioned). The section
*Decisões em aberto* below is kept only as the reason each one exists.

> ⚠️ **Saneado em 09/09/2026, quarta vez.** O cabeçalho dizia 1.4.3 e este parágrafo dizia
> **1.1.19** — dezoito versões atrás do cabeçalho que já estava atrasado. Quem passou a
> acusar foi a régua do F-22, ao cruzar a tolerância de 25 patches no bump da 1.4.29: ela
> mede o cabeçalho, não o corpo, e o corpo estava pior. Vale como medição da régua também —
> ela pega a distância, não a mentira.

**1.6.0 — os três portões de quem instala.** Medido em 16/09: quem instala o ShvIA e escolhe
o motor Claude atravessa **runner ausente → sem login → sem conta**, nessa ordem, e até esta
semana só o terceiro tinha resposta na tela. O runner passou a viajar no instalador (1.5.16) e
o login ganhou URL e campo de código (1.5.17 → 1.6.0). **The screen halves landed in
`SHVIA-WEB` on 16/09** (2.110.308 login on the screen, 2.110.310 accounts from Settings), **but
gate 2 has not closed**: the main login path has never run end to end on a real install.

🔴 **Dois defeitos estavam embaixo do portão 1, calados.** O `install.sh` não copiava o
`parada.mjs` desde a 1.5.0, então **a Run não existia em nenhuma máquina que não reinstalou** —
esta aqui estava com o runner 1.4.20 contra o repo 1.5.14. Isso é pré-requisito do **B9**, que
teria medido nada no motor Claude. Corrigido na 1.5.15, com régua que lê os imports contra as
duas listas.

⚠️ **Estado de conta nesta máquina, em 17/09:** o slot padrão está **deslogado** e o
`claude-me` está logado. Os dois por gesto meu durante medição, não por uso. O `claude-b3`
responde `loggedIn: false`, e **isso é leitura, não conclusão** — a proveniência por leitura
não é afirmável para perfil de credencial (ver o runbook).

**1.4.29** — o Modo Code alcança mais de uma conta do Claude Code: um perfil grava **qual
variável** ele seta (1.4.28), e `npm run contas` cadastra as que a máquina já tem
perguntando ao shell (1.4.29). O diagnóstico do motor virou runbook em
[`../docs/code/MOTOR-CLAUDE-DIAGNOSTICO.md`](../docs/code/MOTOR-CLAUDE-DIAGNOSTICO.md), e o
instalador do runner passou a conferir a própria instalação antes de dizer que deu certo
(1.4.22). O que mudou em cada versão está no [`../CHANGELOG.md`](../CHANGELOG.md) — não é
copiado para cá.

~~**Em aberto, e é a próxima peça:** a tela de Configurações que substitui o
`npm run contas`.~~ **Landed in SHVIA-WEB 2.110.310 (16/09)** — `public/js/app.js` calls
`claudeAccountsDetect` and `claudeAccountAdd` (measured on its `origin/master`, 23/09). The
design decision is in [`contas-claude-macos.md`](contas-claude-macos.md).

**Em aberto desde 10/09/2026, e é a frente seguinte: a Run.** O Modo Code passa a
continuar os turnos sozinho e a parar só onde um humano é necessário — uma pill
(Autonomia) ao lado da Aprovação, uma barra viva entre turnos, e as decisões do
orquestrador visíveis na timeline. O orquestrador é um perfil do gateway, escolhido
independente do coder. **Nada implementado.** Plano em blocos, contratos e a tela
proposta em [`../docs/code/RUN-20260910.md`](../docs/code/RUN-20260910.md); decisão em
ADR-034. O primeiro bloco (B0) era um defeito do runner que o plano achou: o `case
"result"` comparava com um subtipo que o SDK não emite, então erro de API encerrava o
turno sem linha de erro. **Corrigido na 1.4.39**, com prova por reversão.

🔴 **Onde cada lado está, medido em 12/09/2026 — agora os dois lados estão no mesmo
lugar.** No SHVIA-WEB, B4, B5, B6 e B7 **pousaram**: o endpoint e a regra, a máquina de
estados e a tela, a camada de modelo com a pill Orquestrador, e o Histórico agrupado por
run. No SHVIA-CODE, B1 **pousou** na `0.11.22` (12/09). Aqui, B0, B2 e B3 pousam com este
commit. A casca passa a declarar `recursos.run`; antes disso a página não armava run
nenhuma — tratava a capacidade ausente como trata orquestrador ausente, parando na pessoa.
B8 (esta doc) está feito na 1.5.7.

⚠️ **O que NÃO pousou é o B9** — as cinco runs reais que medem a regra, nos três motores.
Nada do Run foi exercitado contra um turno de verdade; o que existe são provas de módulo
puro e o CI. Enquanto o B9 não rodar, não há perfil padrão de orquestrador e o Q5 do plano
segue aberto.

✅ **Revisar os blocos do WEB produziu três PRs de correção, e os três POUSARAM em
11/09/2026** — `SHVIA-WEB` #123, #124 e #125 (2.110.259–2.110.261). Um deles era o portão de
permissão que a camada de modelo nunca teve: o único caminho de inferência daquele
repositório que despachava sem `enforceProfileLimits`, e dava para nomear como orquestrador
um perfil que o papel da pessoa proíbe. Hoje a chamada está em `RunModelTier.php:127`. Outro
corrigiu três bordas do agrupamento do Histórico, uma delas documentada no plano e não
implementada.

> 🔴 **Esta frase dizia "e eles estão abertos… esses defeitos estão em produção" até
> 16/09/2026** — cinco dias depois de os três terem sido mesclados. A afirmação envelheceu
> sozinha, como toda afirmação sobre estado de terceiro, e **a régua do F-22 passou verde o
> tempo todo**: ela compara o CABEÇALHO deste arquivo com o `version.md`, e o cabeçalho
> estava em dia. É a mesma classe do saneamento de 09/09, quando o cabeçalho dizia 1.4.3 e o
> corpo dizia 1.1.19 — a régua mede distância, não mentira.
>
> **Estender a régua ao corpo foi tentado e medido em 16/09, e NÃO funciona aqui.** O modo
> "maior versão citada" que serve ao `funcionalidades.md` lê deste arquivo o **`7.1.0` do
> pacman** (linha 138) como se fosse afirmação dele: seis majors à frente do repositório, um
> número que ninguém acredita — exatamente o absurdo que o comentário do próprio script cita
> como o que ensina a ignorar uma régua. E a frase que apodreceu aqui fala de **PR de outro
> repositório**, que este clone não tem como conferir offline. Fica registrado para ninguém
> refazer a tentativa: **esta classe de podridão não tem guarda mecânica barata; ela se
> conserta relendo.**

**B9 é o que falta de verdade, e é do dono:** cinco runs reais, só regra, nos três motores.
Sem esse número não existe perfil padrão de orquestrador — a pill abre em *Regra, sem
modelo*, e é assim que fica.

**Herança, para não ser lida como estado atual:** o app se auto-atualiza (ADR-022), tem
bandeja nos 3 SOs (ADR-024),
diagnóstico próprio (ADR-025), diálogo de arquivo nativo (ADR-027) e empacota
para Linux (deb/rpm/AppImage/**pacman**), macOS (dmg) e Windows (msi/nsis). O
`anna` viaja dentro do instalador desde a 0.18.0 — o Modo Code não tem mais
pré-requisito externo.

As fases F1 e F2 estão entregues. O detalhe do que funciona está em
[`../docs/funcionalidades.md`](../docs/funcionalidades.md); como buildar, em
[`../docs/build.md`](../docs/build.md).

## Pendências ativas

### 0. ✅ Atualização das duas plataformas — RESOLVIDO em 22/08/2026

**O que estava aberto às 19h30:** o manifesto publicado estava na **1.1.29 com
`linux` apenas** enquanto o repo estava na 1.1.34 — quem usa macOS não recebia
nem o aviso de versão nova. **Foi publicado no mesmo dia**: o manifesto agora
está em **1.1.34 com `linux` e `macos`**, e o app instalado aqui é o 1.1.34.

**A regra que isso ensinou, e que continua valendo** (é o motivo desta seção
ficar aqui em vez de sumir): o `release-manifest.mjs` **recomeça o manifesto
quando a VERSÃO muda** e só mescla plataformas *da mesma versão*. Publicar de uma
máquina só, depois de bumpar, **apaga os artefatos da outra plataforma**. Para o
manifesto ter as duas:

1. buildar e publicar numa máquina;
2. buildar e publicar na outra **sem bumpar a versão entre uma e outra**.

Bumpou no meio? A segunda publicação recomeça o manifesto e derruba a primeira.

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

A ponte do Modo Code no Windows (WebView2) entrou na ADR-010. 🔴 **The claim that the
cross `cargo check` passes stopped being true at 0.9.0 (19/07)**: the origin check called
`args.Source()` with a signature webview2-com-sys 0.38 never had, so **no Windows build was
possible until 1.6.8** — and nothing noticed, because CI runs only on Linux and no Windows
build was attempted. Fixed in 1.6.8 and measured with
`cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings` (MSVC cannot be
checked from Linux: `llvm-rc`/`lib.exe`). **O loop nunca foi validado numa máquina Windows
real.** O
sub-item "colocar o `anna.exe` no PATH" **caiu**: desde a 0.18.0 o `anna` vai
dentro do instalador.

Falta também o **Authenticode EV** (o macOS já tem Developer ID + notarização +
staple no `build-local.sh`).

## Decisões em aberto (confirmar com o time)

**Answered by the owner on the panel on 23/09/2026.** Kept here as the record of what was
asked, with where each answer now lives:

- [x] **Online-only is acceptable** — "Aceitável — documenta". The README's signed trade-off
      now says it is confirmed.
- [x] **Certificates** — "Só Apple": Apple Developer stays; there is **no Windows code-signing
      certificate**, so Windows installers keep the SmartScreen warning. What to do the day one
      exists (E14) is in [`../docs/build.md`](../docs/build.md), "Windows". The **updater key
      rotation** was answered separately: "rotate — prepare the transition" (1.6.40, the runbook
      in `docs/build.md`); generating the key and publishing it is the owner's act.
- [x] **Screens of its own** — "Pode ter telas próprias":
      [ADR-035](../docs/decisoes.md#adr-035--the-desktop-may-have-screens-of-its-own).
- [x] **DEV URL** — `dev.shvia.org`, which the owner is creating (it did not resolve on
      23/09). How to point the app at it: [`../docs/arquitetura.md`](../docs/arquitetura.md),
      "Pointing the app at DEV".
- [x] **Directory case** (`SHVIA-DESKTOP` on this disk, `shvia-desktop` on GitHub) — no action:
      the repository's name is the lowercase one, and the uppercase directory is this machine's
      leftover, left as it is by the owner's choice.

## Ponteiros

- O que já funciona: [`../docs/funcionalidades.md`](../docs/funcionalidades.md)
- Build/empacotamento: [`../docs/build.md`](../docs/build.md)
- Arquitetura: [`../docs/arquitetura.md`](../docs/arquitetura.md) · ADRs:
  [`../docs/decisoes.md`](../docs/decisoes.md)
- Escopo/fases: [`escopo-projeto.md`](escopo-projeto.md)
- Pacote Arch nos repos irmãos: [`arch.md`](arch.md)
