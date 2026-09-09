#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Classificador ASK × DOC da última mensagem do agente.

Contrato normativo em SPEC.md §2. Resumo da tese:

    O sinal não está na pontuação — está na ZONA e na DIREÇÃO.

Um `?` no meio da narrativa costuma ser retórica que o próprio texto responde na
frase seguinte; um fecho que entrega o bastão ("o que sobra está do teu lado da
mesa: A, B, C") não tem `?` nenhum e é exatamente onde o agente parou de trabalhar
e passou a esperar. Detector de `?` erra nos dois sentidos — por isso este módulo
pesa os sinais por zona (narrativa × fecho) e por direção (relata feito × pede ação).

Python 3, stdlib apenas (SECURITY.md — meta de zero dependência).
"""

import re
import unicodedata

# ─────────────────────────────────────────────────────────────────────────────
# Léxico
# ─────────────────────────────────────────────────────────────────────────────

# Entrega de bastão: o agente declara que a bola passou para o humano.
# Só conta na zona de fecho — no meio da narrativa "cabe a você" costuma ser
# citação de regra, não handoff.
HANDOFF = [
    # PT-BR — posse
    r"\bdo (?:teu|seu) lado\b",
    r"\blado da mesa\b",
    r"\bcabe a (?:voc[êe]|vc|ti)\b",
    r"\bdepende de (?:voc[êe]|vc|ti)\b",
    r"\bfica (?:com|na m[ãa]o d)(?:voc[êe]|vc|e voc[êe])\b",
    r"\b(?:sua|tua) (?:decis[ãa]o|chamada|escolha)\b",
    r"\b(?:voc[êe]|vc) (?:decide|escolhe|define)\b",
    r"\bna (?:sua|tua) m[ãa]o\b",
    # PT-BR — espera explícita
    r"\baguardo\b",
    r"\bno aguardo\b",
    r"\bfico no aguardo\b",
    r"\bme (?:diz|diga|avisa|avise|confirma|confirme|fala|responde)\b",
    r"\bme (?:d[êe]|passa) (?:o|a|um|uma)\b",
    r"\bpreciso (?:que voc[êe]|da sua|de uma decis[ãa]o|do seu aval)\b",
    r"\besperando (?:sua|seu|voc[êe])\b",
    r"\bantes de (?:prosseguir|seguir|continuar|avan[çc]ar)\b",
    r"\bassim que (?:voc[êe]|vc) \w+",
    # PT-BR — oferta de caminho (pergunta disfarçada de afirmação)
    r"\b(?:quer|queres) que eu\b",
    r"\bposso (?:seguir|prosseguir|continuar|come[çc]ar|aplicar|mexer)\b",
    r"\bsigo (?:com|por|para|pra)\b",
    r"\bse (?:voc[êe]|vc) (?:quiser|preferir|topar)\b",
    r"\bo que (?:sobra|falta|resta) (?:de |do |para |pra )?(?:maior valor )?(?:est[áa] |fica |[ée] )?(?:do|no|com) (?:teu|seu)\b",
    r"\bfica (?:para|pra) (?:voc[êe]|vc|humano|decis[ãa]o)\b",
    # EN
    r"\bup to you\b",
    r"\byour call\b",
    r"\blet me know\b",
    r"\bwaiting (?:on|for) you\b",
    r"\bshould i\b",
    r"\bdo you want\b",
    r"\bwhich (?:one|option)\b",
    r"\bplease (?:confirm|advise|decide|choose)\b",
]

# Interrogativas diretas sem `?` (imperativo de decisão dirigido ao humano).
INTERROGATIVA_SEM_PONTO = [
    r"\bqual (?:deles|delas|op[çc][ãa]o|caminho|prefere|voc[êe] prefere|dos dois)\b",
    r"\b(?:escolha|escolhe|defina|define|decida|decide|confirme|confirma) (?:qual|entre|se|um|uma|o|a)\b",
    r"\bme (?:diz|diga) qual\b",
]

# Marcas de relato: o texto descreve trabalho concluído.
RELATO = [
    r"\b\d+\s*(?:testes?|tests?|casos?|arquivos?|commits?|opera[çc][õo]es)\b",
    r"\b\d+\s*ok\b",
    r"\b0\s*(?:falhas?|erros?|failures?)\b",
    r"\bcommits? (?:hoje|nesta|neste|desta)\b",
    r"^\s*\d+\.\d+\.\d+\b",
    r"\b(?:entregue|conclu[íi]do|conclu[íi]|feito|pronto|aplicado|corrigido|removido|adicionado)\b",
    r"\b(?:varri|rodei|criei|ajustei|corrigi|removi|adicionei|migrei|commitei|converti|registrei|implementei|escrevi)\b",
]

# Anúncio de retórica: "a pergunta seguinte:", "me perguntei", "resta saber".
# Quando o `?` vem logo depois de um destes, ele é do texto para o texto.
ANUNCIO_RETORICO = [
    r"\ba pergunta\b",
    r"\bpergunta seguinte\b",
    r"\bme perguntei\b",
    r"\bfica a pergunta\b",
    r"\ba quest[ãa]o (?:era|é|e)\b",
    r"\bresta saber\b",
    r"\bmerecia a pergunta\b",
    r"\bthe question (?:is|was)\b",
]

# Trabalho que o próprio agente declarou pendente. Não é pergunta e não é
# handoff — é o relato honesto nomeando o que ficou de fora. Vale fila do mesmo
# jeito: foi exatamente onde o loop original perdeu o fio (ADR-005).
DECLARADO_PENDENTE = [
    r"\bdeclarado e n[ãa]o feito\b",
    r"\bfica (?:nomeado|para a pr[óo]xima|de fora|pendente|para depois)\b",
    r"\bcandidato natural\b",
    # Os DOIS-PONTOS são o discriminador, e custaram três colheitas erradas
    # (17/08). "próxima rodada" solto é substantivo de narrativa — *"fica
    # registrado para a próxima rodada não repetir a varredura"* fala sobre um
    # REGISTRO, não declara trabalho —, e foi assim que uma frase de relatório
    # virou linha de fila. Com `:` a expressão ANUNCIA itens, que é o único uso
    # que o `colher_declarados` sabe ler. Os irmãos desta lista já carregam
    # verbo de adiamento (`fica para a próxima`, `ficou de fora`); estes dois
    # eram os únicos sintagmas nus.
    r"\bpr[óo]xima rodada\s*:",
    r"\bpr[óo]ximo ciclo\s*:",
    r"\bfora deste commit\b",
    r"\bfora do escopo de hoje\b",
    r"\bficou de fora\b",
    # Mesmo modo de falha dos dois acima, achado na varredura que os
    # consertou (17/08): "não coberto" é adjetivo de narrativa —
    # *"esse caminho ficou NÃO COBERTO pela imutabilidade do banco"* fala do
    # alcance de um REVOKE, não declara trabalho, e virava item com a frase
    # inteira junto. Com `:` é cabeçalho de seção, que é o uso legítimo.
    r"\bn[ãa]o coberto\s*:",
    r"\bpend[êe]ncia(?:s)? (?:declarada|conhecida|aberta)",
    r"\bnot done\b",
    r"\bleft undone\b",
    r"\bfollow[- ]ups?\b",
    r"\bnext round\b",
    r"\bout of scope for (?:this|today)\b",
]

TOOLS_QUE_SAO_PERGUNTA = {"AskUserQuestion"}

# ─────────────────────────────────────────────────────────────────────────────
# Utilidades de texto
# ─────────────────────────────────────────────────────────────────────────────

_FENCE = re.compile(r"```.*?```", re.DOTALL)
_INLINE_CODE = re.compile(r"`[^`\n]+`")


def _sem_codigo(texto):
    """Remove blocos de código: `?` dentro de código não é pergunta."""
    texto = _FENCE.sub(" \n ", texto)
    return _INLINE_CODE.sub(" ", texto)


def _norm(texto):
    """Minúsculas sem acento — o léxico é escrito nas duas grafias, mas usuário
    real escreve 'voce' e 'nao'; normalizar evita duplicar cada padrão."""
    texto = texto.lower()
    return "".join(
        c for c in unicodedata.normalize("NFD", texto)
        if unicodedata.category(c) != "Mn"
    )


def _paragrafos(texto):
    return [p.strip() for p in re.split(r"\n\s*\n", texto) if p.strip()]


def _frases(paragrafo):
    """Fatia grosseira em frases. Sem NLP: o corte é em . ! ? seguidos de espaço
    ou fim, preservando o terminador."""
    partes = re.split(r"(?<=[.!?])\s+", paragrafo.strip())
    return [p for p in partes if p]


def zona_de_fecho(paragrafos, n=2):
    """Os dois últimos parágrafos — onde mora o handoff.

    Corte fixo por posição, não por contagem de caracteres. A versão anterior
    acumulava até 300 chars e, em mensagem curta, o fecho engolia o texto
    inteiro — o que desligava a leitura de zona justamente onde ela decide.
    """
    return paragrafos[-n:] if paragrafos else []


# ─────────────────────────────────────────────────────────────────────────────
# Perguntas e retórica
# ─────────────────────────────────────────────────────────────────────────────

def _e_resposta(frase):
    """A frase relata ação concluída? É o que denuncia o `?` auto-respondido."""
    alvo = _norm(frase)
    return any(re.search(r, alvo) for r in RELATO)


def _perguntas_do_paragrafo(paragrafo):
    """Devolve (diretas, retoricas) — listas de frases terminadas em `?`.

    Supressão de retórica, **independente de zona** (é propriedade da frase, não
    da posição — ver ADR-004):
      R1 — anúncio retórico antes do `?` ("a pergunta seguinte:", "me perguntei").
      R2 — a frase logo depois do `?` **relata ação concluída**: o texto se
           respondeu sozinho.

    R2 exige o relato de propósito. "Removo o endpoint antigo? Fico esperando
    para não quebrar o app." também tem frase depois — mas ela é justificativa,
    não resposta, e a pergunta continua de pé.
    """
    frases = _frases(paragrafo)
    diretas, retoricas = [], []
    for i, frase in enumerate(frases):
        if not frase.rstrip().endswith("?"):
            continue
        antes = _norm(" ".join(frases[: i + 1]))
        anunciada = any(re.search(p, antes) for p in ANUNCIO_RETORICO)
        respondida = i < len(frases) - 1 and _e_resposta(frases[i + 1])
        if anunciada or respondida:
            retoricas.append(frase.strip())
        else:
            diretas.append(frase.strip())
    return diretas, retoricas


# ─────────────────────────────────────────────────────────────────────────────
# Colheita de itens do fecho
# ─────────────────────────────────────────────────────────────────────────────

_ITEM_LISTA = re.compile(r"^\s*(?:[-*+•]|\d+[.)])\s+(.{3,})$")


def _split_topo(texto):
    """Separa enumeração em prosa respeitando parênteses.

    Corta em ", a " / ", o " / " e as " etc. — artigo obrigatório depois da
    vírgula, senão qualquer vírgula viraria item. Profundidade de parênteses
    zera o corte: "(coluna versao no Ciclo e no Período)" continua inteiro.
    """
    itens, atual, prof = [], [], 0
    i = 0
    padrao = re.compile(r"^(?:,\s+|\s+e\s+|;\s+)(?=(?:a|o|as|os|um|uma)\s)", re.IGNORECASE)
    while i < len(texto):
        c = texto[i]
        if c in "([{":
            prof += 1
        elif c in ")]}":
            prof = max(0, prof - 1)
        if prof == 0:
            m = padrao.match(texto[i:])
            if m:
                itens.append("".join(atual).strip())
                atual = []
                i += m.end()
                continue
        atual.append(c)
        i += 1
    if atual:
        itens.append("".join(atual).strip())
    return [x.strip(" .;,") for x in itens if len(x.strip(" .;,")) >= 4]


#: Padrões do HANDOFF que descrevem um ESTADO, não pedem uma ação.
#:
#: Eles continuam valendo para CLASSIFICAR (a mensagem vira `ASK` e o loop
#: para): errar para o lado de parar demais custa uma interrupção, e essa é a
#: direção segura. O que eles deixam de fazer é **alimentar a fila** — porque
#: aí a direção do erro se inverte: item fabricado é trabalho que alguém tem de
#: reconhecer como prosa e apagar à mão.
#:
#: Medido em 17/08 com prosa real: *"é **sua decisão**, e está registrada na
#: fila com destino e gatilho"* e *"**antes de prosseguir** com a extração, medi
#: o custo"* — as duas relatam, nenhuma pede. E a prova final foi a mensagem
#: que DESCREVIA este defeito: ela casou o primeiro padrão, virou ASK e colheu
#: um item, que é exatamente o custo que ela dizia ser hipotético.
#:
#: O discriminador honesto seria o tempo do verbo seguinte (*"medi"* ×
#: *"preciso"*), e isso não cabe em regex. Separar os dois usos do sinal cabe.
HANDOFF_QUE_NAO_ANUNCIA_TRABALHO = (
    r"\b(?:sua|tua) (?:decis[ãa]o|chamada|escolha)\b",
    r"\bantes de (?:prosseguir|seguir|continuar|avan[çc]ar)\b",
)


def colher_itens(fecho):
    """Extrai candidatos a item de fila do fecho.

    Heurística deliberadamente conservadora: pega lista markdown quando existe,
    e enumeração em prosa depois de `:` numa frase de handoff. O que escapar daqui
    é colhido pelo próprio agente — o hook manda o texto do fecho junto e pede a
    extração. Plumbing determinístico é do script; entender língua é do modelo.
    """
    itens = []
    for p in fecho:
        alvo = _norm(p)
        # só os handoffs que ANUNCIAM trabalho contam para colher; os que
        # descrevem estado seguem valendo para classificar (ver o bloco acima)
        handoff = any(re.search(h, alvo) for h in HANDOFF
                      if h not in HANDOFF_QUE_NAO_ANUNCIA_TRABALHO)
        # a LISTA aceita os dois sinais; a PROSA continua só com handoff, como
        # sempre foi — alargar o ramo de prosa aqui seria consertar um excesso
        # criando outro, e o `colher_declarados` já cobre a via DECLARADO
        anuncia_pendencia = handoff \
            or any(re.search(d, alvo) for d in DECLARADO_PENDENTE)
        linhas = p.split("\n")
        de_lista = [m.group(1).strip() for m in
                    (_ITEM_LISTA.match(l) for l in linhas) if m]
        if de_lista:
            # A LISTA precisa do mesmo sinal que a prosa — 17/08, quarta
            # colheita errada da mesma rodada. O ramo de lista disparava em
            # QUALQUER bullet do fecho, sem exigir nada: um "**Saldo da
            # rodada:**" com seis marcadores de trabalho FEITO virou seis itens
            # de fila. O ramo de prosa logo abaixo sempre foi conservador
            # (exige handoff + `:`); a assimetria entre os dois é que era o
            # defeito, e nenhum discriminador por conteúdo do item resolve —
            # medido: `RELATO` casa ZERO tanto nos seis relatos quanto nos seis
            # pendentes de verdade. O que separa é o parágrafo que INTRODUZ a
            # lista, e ele já tem marcador para isso.
            if anuncia_pendencia:
                itens.extend(de_lista)
            continue
        if not handoff:
            continue
        if ":" not in p:
            continue
        cauda = p.rsplit(":", 1)[1].strip()
        if len(cauda) < 8:
            continue
        itens.extend(_split_topo(cauda))
    return _dedup(itens)


_ROTULO = re.compile(r"^\s*[*_#>\s]*(.{0,60}?):\s*[*_\s]*")


def colher_declarados(paragrafos):
    """Extrai o trabalho que o agente declarou pendente, em qualquer zona.

    Varre o texto inteiro (não só o fecho) porque "Declarado e não feito" é uma
    seção, não um fecho — e é o item de fila mais valioso que uma mensagem de
    relato produz: veio do próprio agente, já nomeado, sem eu ter que perguntar.
    """
    itens = []
    for p in paragrafos:
        if not any(re.search(m, _norm(p)) for m in DECLARADO_PENDENTE):
            continue
        linhas = p.split("\n")
        de_lista = [m.group(1).strip() for m in
                    (_ITEM_LISTA.match(l) for l in linhas) if m]
        if de_lista:
            itens.extend(de_lista)
            continue
        corpo = _ROTULO.sub("", p.replace("\n", " "), count=1).strip()
        frases = _frases(corpo)
        if not frases:
            continue
        item = frases[0].strip()
        if len(item) < 25 and len(frases) > 1:
            item = (item + " " + frases[1]).strip()
        itens.append(item[:240])
    return _dedup(itens)


def _dedup(itens, teto=12):
    vistos, saida = set(), []
    for it in itens:
        # `*_\`` entram no strip porque a colheita corta no último `:` e herda o
        # que sobra do negrito: `**Pendente:** rodar o lint` deixava `** rodar o
        # lint` na fila. Só nas pontas — marcação no meio do item é do item.
        it = it.strip(" .;,*_`")
        chave = _norm(it)[:60]
        if it and chave not in vistos:
            vistos.add(chave)
            saida.append(it)
    return saida[:teto]


def _sem_marcacao(texto):
    """Texto sem as marcas de ênfase, para comparar conteúdo e não formatação.

    `**Pergunta:** sigo com X?` e `** sigo com X?` são a mesma frase para efeito
    de *"isto é a pergunta?"*; o que os separa é negrito partido no meio por um
    `rsplit(':')`.
    """
    return re.sub(r"[*_`~#>]+", " ", texto)


def _e_a_pergunta(item, perguntas):
    """O item colhido é — ou é um pedaço de — uma pergunta já detectada?

    Comparação nos **dois** sentidos: o item pode ser um pedaço da pergunta (o
    que sobra depois do último `:`, que foi o caso do EOP) ou tê-la engolido
    inteira.

    O piso de 12 caracteres existe porque contenção com alvo curto acha
    qualquer coisa dentro de uma pergunta longa. Abaixo dele o item passa: errar
    para o lado de colher a mais deixa uma linha extra na fila, e errar para o
    lado de colher a menos apaga trabalho declarado — só o segundo é silencioso.
    """
    alvo = " ".join(_norm(_sem_marcacao(item)).split())
    if len(alvo) < 12:
        return False
    for q in perguntas:
        outro = " ".join(_norm(_sem_marcacao(q)).split())
        if outro and (alvo in outro or outro in alvo):
            return True
    return False


def _sem_as_perguntas(itens, perguntas):
    """A pergunta não é item de fila (emenda ao ADR-005).

    A colheita procura **trabalho declarado pendente**; a pergunta é o oposto
    disso — é trabalho que o agente declarou que *não* faz sem resposta. Ela já
    tem três lugares seus: a entry, o `INDEX.md` e a premissa do
    `ASSUMPTIONS.md` (ADR-003). Um quarto, na fila, não a preserva melhor: a
    torna **marcável como feita**, e um `- [x]` numa pergunta é o loop dizendo
    que respondeu a si mesmo.

    Em 17/08 no EOP foi exatamente isso, e não parou na contabilidade: a
    pergunta era o 22º item, o `contagem_fila()` chegou a zero pendentes e a
    condição "fila zerada" encerrou a rodada. Uma condição de fim correta
    disparando sobre uma fila que continha uma pergunta.

    O ADR-005 segue de pé — a colheita continua **independente do veredito**
    ASK/DOC, e este filtro roda nos dois. O que ele corrige não é *quando* se
    colhe, é *o quê*.
    """
    return [i for i in itens if not _e_a_pergunta(i, perguntas)]


# ─────────────────────────────────────────────────────────────────────────────
# API
# ─────────────────────────────────────────────────────────────────────────────

class Resultado(object):
    """kind ∈ {ASK, DOC} · sinal explica POR QUE · evidencias justificam."""

    __slots__ = ("kind", "sinal", "confianca", "evidencias", "perguntas",
                 "retoricas", "itens", "fecho")

    def __init__(self, kind, sinal, confianca, evidencias, perguntas,
                 retoricas, itens, fecho):
        self.kind = kind
        self.sinal = sinal
        self.confianca = confianca
        self.evidencias = evidencias
        self.perguntas = perguntas
        self.retoricas = retoricas
        self.itens = itens
        self.fecho = fecho

    def as_dict(self):
        return {s: getattr(self, s) for s in self.__slots__}

    def __repr__(self):
        return "<%s %s/%s %s>" % (self.kind, self.sinal, self.confianca,
                                  self.evidencias[:2])


def classificar(texto, ultimo_tool=None):
    """Classifica a última mensagem do agente.

    `ultimo_tool` é o nome da última tool chamada no turno, quando houver:
    AskUserQuestion curto-circuita para ASK — não há heurística que supere uma
    pergunta declarada pelo próprio protocolo.
    """
    texto = (texto or "").strip()
    if ultimo_tool in TOOLS_QUE_SAO_PERGUNTA:
        return Resultado("ASK", "tool", "alta",
                         ["tool %s no fim do turno" % ultimo_tool],
                         [], [], [], [])
    if not texto:
        return Resultado("DOC", "vazio", "baixa", ["mensagem sem texto"],
                         [], [], [], [])

    limpo = _sem_codigo(texto)
    paras = _paragrafos(limpo)
    fecho = zona_de_fecho(paras)

    diretas_fecho, diretas_narr, retoricas = [], [], []
    for i, p in enumerate(paras):
        d, r = _perguntas_do_paragrafo(p)
        (diretas_fecho if i >= len(paras) - len(fecho) else diretas_narr).extend(d)
        retoricas.extend(r)

    texto_fecho = _norm("\n".join(fecho))
    handoffs = [h for h in HANDOFF if re.search(h, texto_fecho)]
    imperativos = [h for h in INTERROGATIVA_SEM_PONTO if re.search(h, texto_fecho)]
    relatos = [r for r in RELATO if re.search(r, _norm(limpo), re.MULTILINE)]

    declarados = colher_declarados(paras)
    # Todas as perguntas detectadas, das três zonas: a retórica também não é
    # item — ela já foi respondida pelo próprio texto, e ir para a fila seria
    # mandar o agente refazer o que ele acabou de concluir.
    itens = _dedup(_sem_as_perguntas(colher_itens(fecho) + declarados,
                                     diretas_fecho + diretas_narr + retoricas))
    ev = []
    if declarados:
        ev.append("%d pendência(s) declarada(s) pelo próprio agente" % len(declarados))

    if diretas_fecho:
        ev.append("pergunta direta no fecho: %r" % diretas_fecho[-1][:120])
        if retoricas:
            ev.append("%d retórica(s) suprimida(s) na narrativa" % len(retoricas))
        return Resultado("ASK", "pergunta-direta", "alta", ev,
                         diretas_fecho, retoricas, itens, fecho)

    if handoffs or imperativos:
        marca = (handoffs + imperativos)[0]
        ev.append("entrega de bastão no fecho (%s)" % marca)
        if not diretas_fecho:
            ev.append("sem `?` — handoff é o único sinal; detector de pontuação erraria aqui")
        if itens:
            ev.append("%d item(ns) pendente(s) colhido(s) do fecho" % len(itens))
        return Resultado("ASK", "handoff", "alta", ev,
                         diretas_fecho, retoricas, itens, fecho)

    if diretas_narr:
        ev.append("pergunta fora do fecho, não auto-respondida: %r"
                  % diretas_narr[-1][:120])
        return Resultado("ASK", "pergunta-narrativa", "media", ev,
                         diretas_narr, retoricas, itens, fecho)

    if retoricas:
        ev.append("%d pergunta(s) retórica(s) suprimida(s): %r"
                  % (len(retoricas), retoricas[0][:120]))
    if relatos:
        ev.append("marcas de relato: %d" % len(relatos))
    if not ev:
        ev.append("nenhum sinal de pergunta ou de espera")
    conf = "alta" if relatos else "media"
    return Resultado("DOC", "relato", conf, ev, [], retoricas, itens, fecho)
