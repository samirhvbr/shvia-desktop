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
| [`estado-atual.md`](estado-atual.md) | **Ler primeiro.** Onde paramos, decisões tomadas, próximos passos. |
| [`escopo-projeto.md`](escopo-projeto.md) | Escopo detalhado, arquitetura, plano de fases, riscos, decisões em aberto. |

## Convenção

Quando uma nota/roteiro **amadurece e vira doc estável**, migre para
**[`/docs`](../docs/)** (versionada e perene) e remova daqui — mantendo o
`.continue/` enxuto, só com o que está realmente em andamento.
