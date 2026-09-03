<!--
Template do `reason` que o hook Stop devolve quando a fila zerou e há **relógio
declarado** (`--duracao` / `--janela`). Artefato do produto (ADR-015) — mudar
aqui é mudar comportamento (bump em version.md).

Placeholders (str.replace): iteracao, max_iteracoes, kind, sinal, entry, feitos,
objetivo, escopo, restante_relogio, bloco_ask, bloco_colhidos.

É o `prompts/reabastecer.md` — o item que o dono colava na cauda da fila — virado
prompt do motor. As cláusulas são as mesmas, e cada uma saiu de uma rodada real,
não de desenho; a tabela do porquê continua lá. Duas não são opcionais (ADR-014):
o **escopo declarado** com o que "para e pergunta", e o **escape** que deixa a
rodada morrer por veredito em vez de fabricar trabalho.
-->
[LOOP-WORK · iteração {iteracao}/{max_iteracoes} · REABASTECIMENTO · sua última mensagem foi arquivada como {kind} em {entry}]

**Ninguém está lendo o chat agora.** A fila zerou — {feitos} item(ns) fechado(s) —
e ainda há **{restante_relogio}** de rodada. Fila vazia aqui não é fim: esta
rodada foi armada por tempo, e o seu trabalho neste turno é **encher a fila de
novo**.

**Objetivo da rodada:** {objetivo}

**Escopo — o que pode entrar e o que para e pergunta:**

{escopo}

{bloco_ask}{bloco_colhidos}
**Como reabastecer:**

1. Escolha o **próximo bloco de trabalho ainda não coberto**, usando os índices
   do repositório como mapa (o que existe × o que já foi entregue) e
   **respeitando o escopo acima**.
2. **Leia a documentação dele inteira** — não amostre. Proxy de palavra-chave
   erra: numa rodada real errou 3 de 7 triagens. Meça pelo que a casa escreveu,
   não pelo que se espera que ela tenha escrito.
3. Destile o bloco em linhas `- [ ]` no fim de `.loop/QUEUE.md`, uma por unidade
   **executável sozinha** — quem lê o item é um turno futuro que não tem este
   contexto. "Ajustar o billing" não serve; "Converter as 5 observações do
   Billing de comentário em consulta ao banco (SPEC §4.2)" serve.
4. Registre, em uma linha, **o que a triagem mediu** — inclusive a hipótese que
   morreu, para a próxima volta não repetir a varredura. Hipótese que mede zero é
   resultado, não desperdício.
5. Siga trabalhando o primeiro item que você acabou de escrever, no mesmo turno.
   Não pare para me mostrar a fila.

**Se não houver bloco em escopo** — e só nesse caso:

- **não invente trabalho.** Escreva o veredito com os números medidos em
  `.loop/SEM-ESCOPO` (uma linha de conclusão, depois o que foi varrido e o que
  cada hipótese mediu) e encerre o turno.
- A rodada termina ali, registrada como `escopo esgotado`. **Fila zerada com
  veredito é o desfecho certo; bloco fabricado para cumprir esta instrução é o
  pior de todos** — prosa sem lastro num repositório onde a documentação é fonte
  de verdade é pior que parar.

**Nunca** peça confirmação sobre o que reabastecer. Se a dúvida cai dentro do
"para e pergunta" do escopo, o bloco **não** entra: trate como fora de escopo e
siga para o próximo candidato. Se a dúvida é menor, adote o default reversível,
registre em `.loop/ASSUMPTIONS.md` e siga.

Para encerrar por ordem sua, a qualquer momento: `touch .loop/STOP`.
