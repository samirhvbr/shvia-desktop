# `.continue/` — área de trabalho em andamento (WIP)

Pasta de **rascunho de documentação e contexto** de trabalho em andamento:
roteiros de implementação, specs em discussão, notas de features sendo
construídas, briefings para "continuar" depois.

## Como funciona

- **Versionada no git (de propósito — NÃO está no `.gitignore`).** Assim, ao abrir
  o projeto em outra máquina/ambiente, o contexto vem junto e dá pra *continuar* de
  onde parou.
- **Fora do build/empacotamento.** Nada daqui vai para os instaladores.
- O IDE **Continue** também usa esta pasta para configuração própria.

## Arquivos

| Arquivo | Papel |
|---------|-------|
| [`estado-atual.md`](estado-atual.md) | **Ler primeiro.** O que está em aberto. Saneado em 07/08/2026 — antes descrevia a 0.8.0 estando o repo na 1.1.16. |
| [`arch.md`](arch.md) | Pacote Arch (`.pkg.tar.zst`) nos três repos de desktop. Aqui só o que segue em aberto: SSHVTERM-DESKTOP e GITHUB-DESKTOP ainda na rota antiga (`fpm`). |
| [`escopo-projeto.md`](escopo-projeto.md) | Escopo, arquitetura e riscos de 30/06. O **plano de fases está quase todo entregue** e traz aviso no topo — ler como pendência faria alguém refazer coisa pronta. |

Removido em 28/07/2026: `NEW-escopo-projeto.md` — era a especificação de **provedores de
nuvem no gateway**, ou seja, assunto do SHVIA-**WEB**, parada neste repo por engano e com
o trabalho entregue há muito (os três drivers existem). O conteúdo está preservado em
[`SHVIA-WEB/docs/ARQUIVO/NEW-provedores-nuvem-gateway.md`](../../SHVIA-WEB/docs/ARQUIVO/NEW-provedores-nuvem-gateway.md).

## Convenção

Quando uma nota/roteiro **amadurece e vira doc estável**, migre para
**[`/docs`](../docs/)** (versionada e perene) e remova daqui — mantendo o
`.continue/` enxuto, só com o que está realmente em andamento.
