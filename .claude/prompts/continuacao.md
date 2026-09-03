<!--
Template do `reason` que o hook Stop devolve para retomar o agente.
Artefato do produto — mudar aqui é mudar comportamento (bump em version.md).

Placeholders (str.format): iteracao, max_iteracoes, kind, sinal, entry, item,
pendentes, feitos, objetivo, bloco_ask, bloco_colhidos.

Regras de escrita deste prompt (aprendidas em campo, SPEC.md §4):
- Dizer que o chat NÃO está sendo lido. Sem isso o agente volta a resumir.
- Dar o item exato. "Continua" sozinho faz o agente re-planejar e derivar.
- Dar a condição de parada explícita, senão ele para na primeira dúvida.
- Nunca pedir confirmação de nada aqui — este texto existe para eliminá-la.
-->
[LOOP-WORK · iteração {iteracao}/{max_iteracoes} · sua última mensagem foi arquivada como {kind} em {entry}]

**Ninguém está lendo o chat agora.** O relato que você acabou de escrever já foi
gravado em disco e será lido depois, em lote. Escrever resumo de novo é trabalho
perdido: siga produzindo.

**Item atual da fila** (`.loop/QUEUE.md` — {feitos} feito(s), {pendentes} pendente(s)):

> {item}

{bloco_ask}{bloco_colhidos}
**Como seguir:**

1. Execute o item atual **até o fim** — não relate progresso parcial, não peça
   confirmação, não proponha alternativas para eu escolher.
2. Marque `- [x]` no `.loop/QUEUE.md` quando concluir.
3. Vá **direto** para o próximo item pendente, no mesmo turno, sem me avisar.
4. Se descobrir trabalho novo necessário, acrescente-o como `- [ ]` na fila em
   vez de me perguntar se deve fazer.

**Só encerre o turno de verdade se** uma destas for verdade:

- a fila zerou (nenhum `- [ ]` restante);
- existe o arquivo `.loop/STOP`;
- a próxima ação é **destrutiva ou irreversível** e não está coberta por uma
  premissa já registrada (apagar dados, migração sem volta, push forçado,
  gastar dinheiro, mexer em produção, mandar mensagem para terceiro);
- você está **tecnicamente bloqueado** por algo fora do seu alcance (credencial
  ausente, serviço fora do ar) — e aí diga exatamente o que falta, em uma linha.

Objetivo do loop: {objetivo}
