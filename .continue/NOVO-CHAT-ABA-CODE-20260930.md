# Novo chat na aba Code — limpar a janela e abrir um item no histórico

**Estado: PEDIDO de 30/09/2026 — nada implementado.** Este documento é a especificação; ele
sai desta fila quando o comportamento abaixo existir (regra da fila: `CLAUDE.md`, "The queue
empties by production").

**O código que muda NÃO mora neste repo.** O Desktop é um shell fino: a janela carrega o ShvIA
web, e a aba Code é a página `public/js/code-mode.js` do **SHVIA-WEB**. A entrega é um commit no
SHVIA-WEB; assim que ele for ao ar o Desktop já o mostra, **sem release nova do Desktop**.

## O pedido, nas palavras do dono

> Em shvia-desktop, na aba Code, quando clicar em **novo chat** vamos literalmente **limpar a
> janela** e **criar um novo item no history abaixo**. Na aba Chat isso funciona direitinho, mas
> na aba Code não está funcionando: o botão é decorativo.

## Comportamento esperado (aceite)

1. Clicar em "Novo chat" na aba Code **limpa a timeline por inteiro**: nenhuma bolha, divisória
   de sessão ou card pendente sobra na janela.
2. **A conversa que estava na janela continua no histórico** (a seção "Histórico" do cartão da
   pasta), como um item. O chat novo ganha o seu próprio item. É o mesmo desenho da aba Chat.
3. O agente termina como hoje: processo encerrado, aprovações pendentes abortadas, run parada,
   fila esvaziada (`resetSession` já faz tudo isso — só a parte visual muda).
4. **Todo caminho de "Novo chat" na face Code faz isso**, e nenhum deles mexe na aba Chat.
5. Com a timeline **vazia** o clique também responde (o item novo aparece); "não acontece nada"
   é exatamente o defeito relatado.
6. **Prova esperada:** com a aba Code com conversa, clicar → janela vazia, histórico com um item
   a mais e o anterior intacto; a conversa ativa da aba Chat não muda e nenhum `POST /reset` sai.

## Diagnóstico — lido no código, **ainda não reproduzido no app rodando**

Lido em `origin/master` do SHVIA-WEB (`9e0daba7`, 29/09/2026). Há **três** caminhos de "Novo
chat", e nenhum faz o que o pedido descreve na face Code:

| Caminho | Onde | O que faz | Na face Code |
|---|---|---|---|
| Botão da barra lateral (`#reset-button`) e `+` do trilho (`data-rail="new"`) | `app.js:10393`, `app.js:10426` | `resetChat()` (`app.js:8874`): `POST /reset`, `messagesEl.innerHTML = ''`, `activeConversationId = null` | **Decorativo.** O CSS só esconde `#reset-button` em `data-mode="chat"` (`chat-workspace.css:1177`), então na face Code ele aparece. Mas `#messages` está `display: none` ali (`code-mode.css:94`): o clique limpa uma janela invisível. |
| Botão do painel de chats (`#panel-new-chat`) | `app.js:10402` | o mesmo `resetChat()` | Escondido na face Code (`code-mode.css:91`). Não é o botão que se clica. |
| Botão "Novo chat" do cartão da pasta (`cf-btn--wide`) | `code-mode.js:965-969` | `resetSession('novo chat')` (`code-mode.js:2719`) | **Quase invisível.** Encerra o agente mas, por decisão registrada, **não apaga a timeline**: só desenha uma divisória (`marcarFimDeSessao`, `code-mode.js:2705`). Com a timeline vazia, `if (!timeline.firstChild) return` não desenha nada. |

**Efeito colateral a cortar.** Como o primeiro caminho chama `resetChat()`, clicar em "Novo chat"
**na face Code** ainda faz `POST /reset` e zera a conversa ativa **da aba Chat**, sem que nada
mude na tela em que a pessoa está. É o candidato mais forte para o "botão decorativo", e é um
defeito por si só.

**Não confirmado:** qual dos dois botões o dono clica quando "não acontece nada". Os dois têm de
ficar certos, então a especificação vale para ambos.

## A decisão que este pedido inverte

O código de `code-mode.js` registra (27/07, reafirmado no pedido de 31/08) que `resetSession`
**não apaga a timeline**: *"apagar o que o usuário já leu vinha de carona e era a causa de 'meu
histórico desapareceu'"*. O pedido de hoje inverte isso, e o motivo antigo continua atendido se
valer esta condição:

> **Nada que estava na janela pode sumir.** Limpar a janela passa a *mover* a conversa para o
> histórico, nunca a perdê-la.

Quem entregar registra a inversão num ADR e troca os comentários `🔴 Não apagar é decisão
registrada` de `code-mode.js` (perto de `resetSession` e do botão do cartão), que ficariam
mentindo.

## Onde mexer (SHVIA-WEB)

- `public/js/code-mode.js` — `resetSession`, `marcarFimDeSessao`, `podarTimeline`, o botão do
  cartão da pasta; o histórico (`pintarHistorico`, `agruparSessoes`, `histPush`).
- `public/js/app.js` — os handlers de `#reset-button`, do trilho (`data-rail="new"`) e de
  `#panel-new-chat`: na face Code eles devem chamar a rotina do Code, **não** `resetChat()`. O
  guard mora no handler (ou dentro de `resetChat`), não em cada botão: são vários caminhos até
  lá, e um ficaria de fora na próxima refatoração — o mesmo argumento que `resetChat` já faz.
- `public/js/code-transcript.js` — o store das sessões do histórico.

## Perguntas em aberto

1. O item novo aparece **no clique** (vazio, esperando a primeira mensagem) ou **na primeira
   mensagem**, como na aba Chat? Conferir o que a aba Chat faz e copiar.
2. **Fora de escopo por ora:** clicar num item do histórico para *retomar* aquela conversa. Hoje
   o histórico da aba Code é só leitura ("não é uma lista de conversas para retomar") e este
   pedido não muda isso.
