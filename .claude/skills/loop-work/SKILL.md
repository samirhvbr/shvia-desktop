---
name: loop-work
description: >-
  Faz o agente trabalhar horas sem pedir "continua". Arma um hook Stop que, a cada
  fim de turno, classifica o relato (ASK ou DOC), arquiva em .loop/ e devolve o
  agente ao próximo item da fila. Use quando houver semanas de trabalho
  documentado e o agente estiver parando a cada poucos minutos para relatar.
---

# loop-work

O agente para no fim de cada bloco de trabalho e escreve um relato. Quem está
longe do monitor só vê isso 10 minutos depois, digita "continua", e o ciclo se
repete: **5 minutos de produção, 10 minutos parado**. Em trabalho de meses, a
maior parte do calendário é tela apagada.

Esta skill arma um hook `Stop` que ocupa esse vão. Ele dispara no instante em que
o turno encerra, arquiva o relato e devolve o agente ao trabalho com o próximo
item nomeado.

## Parse

`/loop-work [subcomando] [argumento]`

| Forma | Ação |
|---|---|
| `/loop-work <objetivo em texto>` | destila a fila e arma |
| `/loop-work` (sem argumento) | destila a fila e arma, inferindo o objetivo do contexto |
| `/loop-work status` | relatório do ciclo |
| `/loop-work porque` | por que não continuou — os portões do hook, em ordem |
| `/loop-work parar` | desarma |
| `/loop-work retomar` | rearma de onde parou, **re-amarrando a sessão** |
| `/loop-work fila` | mostra pendentes e o próximo |

O motor é `loop_ctl.py`, ao lado deste arquivo:

```bash
python3 <skill>/loop_ctl.py armar --objetivo "..." [condições de fim]
python3 <skill>/loop_ctl.py status | porque | parar | retomar | fila
```

`--raiz` vale antes ou depois do subcomando.

### Condições de fim — pergunte antes de armar

Um loop sem fim é uma fatura sem teto. Se o usuário não disser onde parar,
**pergunte** antes de armar; ele quase sempre tem uma resposta em mente ("até
fechar os 10 primeiros", "das 8h às 18h"). Traduza a resposta em flags:

| O que ele diz | Flags |
|---|---|
| "fecha os 10 primeiros" | `--itens 10` |
| "vai até o item X" | `--ate "X"` |
| "produz das 8h às 18h" | `--janela 08:00-18:00` |
| "só dia útil" | `--dias seg-sex` |
| "no máximo umas 6 horas" | `--duracao 6h` |
| "até acabar" | nada — fila zerada e `--max` respondem |

⚠️ **Relógio muda o que a fila significa** (ADR-015). Com `--duracao`/`--janela`,
`fila zerada` **não** encerra: a fila vazia vira turno de reabastecimento (§1.1), e
a rodada acaba pelo tempo, pelos tetos, ou pelo veredito que o agente escreve em
`.loop/SEM-ESCOPO`. Sem relógio, a fila continua sendo o critério de pronto.

Combinam livremente; a primeira que bater encerra. Na dúvida, prefira **duas**
(uma de escopo e uma de tempo) — elas se cobrem quando a fila é maior ou menor
do que parecia.

⚠️ O loop **não se rearma sozinho**: fechada a janela, retomar é comando.

## Armar

### 1. Destilar a fila — o passo que decide se o loop funciona

**Não arme com a fila vazia** — a não ser por tempo (§1.1). `.loop/QUEUE.md` é o
que o hook injeta no `reason` a cada parada; sem ela a continuação vira "continue
de onde parou", o agente re-planeja a cada turno e o trabalho deriva. Sob relógio
o motor cobre isso na primeira parada, mas mesmo ali um bloco destilado à mão é
melhor começo: ele gasta o primeiro turno produzindo, não triando.

Antes de armar:

1. Localize a documentação que descreve o trabalho (o usuário costuma apontar;
   senão procure `docs/`, `SPEC.md`, `.continue/`, roadmap, backlog).
2. **Leia-a inteira** — não amostre. A fila é o contrato do ciclo.
3. Destile em `.loop/QUEUE.md` uma linha `- [ ]` por unidade executável:

   - **Executável sozinha.** Quem lê o item é um turno futuro que não tem o
     chat de hoje. "Ajustar o billing" não serve; "Converter as 5 observações
     do Billing de comentário em consulta ao banco (SPEC §4.2)" serve.
   - **Verificável.** O item precisa ter um fim reconhecível, senão o agente
     nunca marca `- [x]` e o loop não mede progresso.
   - **Ordenada por dependência.** O hook entrega sempre o primeiro `- [ ]`.
   - Trabalho já feito entra como `- [x]` — dá denominador ao progresso.

4. Mostre a fila ao usuário antes de armar. É a última chance barata de
   corrigir rumo: depois disso o agente executa sem perguntar.

### 1.1 Fila que se reabastece — quando o usuário quer horas, não itens

Se o pedido é por **tempo** ("me deixa isso rodando a tarde inteira", `--duracao
6h`) e há mais documentação do que cabe numa destilação, a fila precisa se
reabastecer. **Com relógio isso é do motor** (ADR-015): quando a fila zera, o hook
devolve o prompt de reabastecimento — escolher o próximo bloco não coberto **dentro
do escopo**, ler a documentação dele inteira, destilar `- [ ]` no fim do `QUEUE.md`,
registrar o que mediu, e seguir trabalhando. Você não precisa colar nada.

O que você **precisa** dar é a fronteira, porque ela é decisão do dono:

- Escreva `.loop/SCOPE.md` com **o que pode entrar** e **o que para e pergunta**
  (dinheiro, autenticação, dado de produção, decisão de produto — o que valer ali).
  O arquivo vai **verbatim** para o prompt.
- Sem ele, o escopo sai só do `--objetivo`, e o prompt avisa o turno de que a
  fronteira não foi declarada — ele passa a recusar o que for duvidoso. Funciona,
  mas é mais estreito do que precisaria ser.

E o fim: se a medição disser que **não há** bloco em escopo, o agente escreve o
veredito com os números em `.loop/SEM-ESCOPO` e a rodada encerra ali, como `escopo
esgotado`. Fila zerada com veredito é o desfecho certo; bloco fabricado para
cumprir a instrução é o pior de todos.

**Sem relógio** (rodada por itens), o mecanismo continua sendo um **item na cauda
que se reproduz**: copie [prompts/reabastecer.md](../../prompts/reabastecer.md),
troque o que está entre ‹› e cole no fim do `QUEUE.md`.

Medido em 17/08/2026 (EOP), com o item na cauda: 14 paradas seguidas sem encerrar,
13 com o REABASTECER como item, fila de 22 → 66 itens, e o fim veio por veredito
escrito — sete hipóteses tabeladas, três viraram bloco, três mediram zero. É essa
medição que virou motor.

### 2. Armar

```bash
python3 <skill>/loop_ctl.py armar --objetivo "<uma linha>"
```

`armar` **recusa** fila sem nenhum pendente (`--mesmo-sem-fila` força). Não é
capricho: rodada sem pendente morre na primeira parada, e três delas em 17/08
ainda gastaram um turno cada para dizer que nada havia acontecido. **Com
`--duracao`/`--janela` a recusa não se aplica** (ADR-015): ali a fila vazia não
morre, ela reabastece.

Confirme em uma linha: objetivo, quantos itens, teto de iterações, política de
ASK, e como parar (`touch .loop/STOP` — funciona sem terminal, de qualquer
lugar do sistema de arquivos).

Depois de armar, **comece a trabalhar imediatamente**, no mesmo turno. Não
encerre para pedir confirmação: a primeira parada é o que amarra o loop à sessão.

`armar` deixa em `.loop/loop.sh` o atalho da **próxima** rodada neste repo —
`./.loop/loop.sh` arma por 6h e abre o painel, `./.loop/loop.sh 10h` por outro
tempo. Ele é do dono a partir daí: nunca é sobrescrito, e é lá que ele guarda
`--objetivo`/`--janela`/`--itens` entre rodadas. Mencione o arquivo ao confirmar,
uma linha — quem arma pelo terminal não vai voltar aqui para lembrar da flag.

## Durante o loop

A cada fim de turno o hook injeta o próximo item e as regras. Enquanto ele estiver
armado:

- **Não escreva relato para o chat.** Ninguém está lendo. O que precisa
  sobreviver vai para o commit, para `docs/` ou para a própria fila.
- **Não peça confirmação.** Decisão em aberto → adote o default mais razoável
  **e reversível**, registre em `.loop/ASSUMPTIONS.md` (pergunta · decisão ·
  alternativa descartada · como reverter) e siga.
- **Marque `- [x]`** ao concluir um item, no mesmo turno, antes de seguir.
- **Trabalho novo vira item**, não pergunta: acrescente `- [ ]` na fila.
- **Encerre de verdade** só se a fila zerar (rodada **sem** relógio), se existir
  `.loop/STOP`, se a próxima ação for destrutiva/irreversível sem premissa que a
  cubra, ou se você estiver bloqueado por algo fora do seu alcance (credencial,
  serviço fora do ar).
- **Rodada por tempo com a fila zerada:** reabasteça (o hook manda como). Só
  encerre escrevendo o veredito medido em `.loop/SEM-ESCOPO` — e só se **não**
  houver bloco em escopo. Nunca fabrique trabalho para manter o loop vivo.

## Acompanhar enquanto roda

De outro terminal, sem atrapalhar a sessão:

```bash
loop-watch              # atualiza a cada 30 s
loop-watch -n 10        # outro intervalo
loop-watch --ate-encerrar   # sai (com sino) quando o loop parar
loop-watch --uma-vez        # uma leitura e sai, para log ou cron
```

Ele mostra o que o `status` cru não mostra: **delta desde a última leitura**
(andou?) e **tempo restante de cada condição de fim**, com a que bate primeiro
marcada. ASK e fecho parcial aparecem sinalizados na lista de paradas. Com o
loop parado, o painel avisa que o **hook está inerte** — digitar "continua" no
chat não reativa nada.

E quando o agente parar sem entrar em loop:

```bash
loop-ctl porque --raiz <repo>          # sai 1 se algum portão barra
loop-ctl porque --raiz <repo> --sessao <session_id>   # confere a amarração
```

Ele percorre os portões na ordem em que o hook os testa — hook instalado,
`.loop/`, `ativo`, `fase`, amarração à sessão — e depois as condições de fim,
dizendo qual barrou e qual é o conserto. Existe porque o hook é fail-open e sai
calado (ADR-009): em 17/08 havia três portões fechados no mesmo `.loop/` e
nenhuma linha de log sobre nenhum deles.

## Ler o que ficou registrado

Depois — nunca durante:

| Arquivo | O que responde |
|---|---|
| `.loop/INDEX.md` | uma linha por parada: tipo, sinal, decisão, item |
| `.loop/entries/NNNN-ASK-*.md` | as paradas que eram decisão sua |
| `.loop/ASSUMPTIONS.md` | **leia isto primeiro** — o que foi decidido sem você |
| `.loop/QUEUE.md` | quanto andou — e o que o reabastecimento puxou |
| `.loop/SEM-ESCOPO` | o veredito: o que foi varrido e o que mediu zero |
| `.loop/STATUS.md` | por que o loop encerrou |

Revisar `ASSUMPTIONS.md` não é opcional. É o preço de não ter sido interrompido.

## O que a skill nunca faz

- Não arma sozinha: `.loop/` só nasce por `armar`, e sem ele o hook global é
  inerte em toda a máquina.
- Não responde perguntas pelo usuário — ela instrui o agente a **assumir e
  registrar**, o que é auditável depois; e a distinção está em `INDEX.md`.
- Não remove o `.loop/` nem edita `ASSUMPTIONS.md` retroativamente.
- Não sobrevive a `.loop/STOP`.
