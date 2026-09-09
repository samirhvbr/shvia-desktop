#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Por que o loop não continuou — os portões, na ordem em que o hook os testa.

O hook é **fail-open por desenho** (ADR-009): quando algo não bate, ele sai com
`exit 0` e não escreve nada. Isso é certo para não travar o Claude Code da
máquina, e é justamente o que deixa o operador no escuro — em 17/08/2026 o
"continua" digitado no EOP não continuou, e havia **três** portões fechados ao
mesmo tempo sem uma linha de log sobre nenhum deles.

Este módulo existe para que a pergunta *"por que não continuou?"* tenha resposta
de comando, e para que ela não seja uma **quarta** cópia da lista de condições:

- `condicoes_de_fim` é a **fonte** — o hook a consome no lugar da própria cadeia
  `if/elif`. Uma cópia só, e os testes de ponta a ponta do hook a provam.
- `portoes_de_inercia` é **espelho** dos quatro portões que o hook atravessa
  antes de mexer em qualquer coisa. Espelho e não fonte porque o hook *muta*
  estado em dois deles (consome `fase: encerrando`, auto-amarra a sessão), e
  mutação não cabe num diagnóstico. O que garante o alinhamento é teste
  emparelhado, não confiança: `tests/test_diagnostico.py` alimenta os mesmos
  estados ao hook (subprocesso) e a este espelho, e exige que o silêncio de um
  corresponda ao portão nomeado pelo outro.

Python 3, stdlib apenas. Nada aqui escreve em disco.
"""

import json
import os
from collections import namedtuple

from estado import fora_da_janela, minutos_desde

# `ok`: True passou · False barra · None informativo (não barra, mas explica).
Portao = namedtuple("Portao", "nome ok detalhe conserto")


# ── o hook está instalado nesta máquina? ────────────────────────────────────
def hook_instalado(settings=None):
    """`(instalado, caminho)` — o `Stop` do LOOP está registrado no settings?

    É a única checagem que o hook não pode fazer sobre si mesmo: se ele não está
    registrado, ele não roda para dizer que não está registrado.

    `instalado` é `None` quando o arquivo não deu para ler — ausência de leitura
    não é o mesmo fato que ausência do hook, e o diagnóstico não inventa o que
    não mediu.
    """
    caminho = (settings or os.environ.get("CLAUDE_SETTINGS")
               or os.path.expanduser("~/.claude/settings.json"))
    try:
        with open(caminho, encoding="utf-8") as f:
            dados = json.load(f)
    except (IOError, OSError, ValueError):
        return None, caminho
    if not isinstance(dados, dict):
        return None, caminho
    hooks = dados.get("hooks")
    grupos = hooks.get("Stop") if isinstance(hooks, dict) else None
    for grupo in grupos or []:
        if not isinstance(grupo, dict):
            continue
        for h in grupo.get("hooks") or []:
            if isinstance(h, dict) and "loop-stop.py" in str(h.get("command", "")):
                return True, caminho
    return False, caminho


# ── portões de inércia: o hook sai calado em todos eles ─────────────────────
def portoes_de_inercia(loop, st, sid=None, settings=None):
    """Lista ordenada de `Portao` até o primeiro que barra (inclusive).

    A ordem é a do hook: `.loop/` existe → `ativo` → `fase` → amarração. A
    checagem do settings entra antes de todas porque é pré-condição de o hook
    existir. Para depois do primeiro `ok is False`: portão fechado torna o resto
    inalcançável, e listar o que não foi testado como se tivesse passado seria
    mentira.
    """
    fora = []

    instalado, caminho = hook_instalado(settings)
    if instalado is True:
        fora.append(Portao("hook instalado", True, "Stop → loop-stop.py (%s)" % caminho, None))
    elif instalado is False:
        fora.append(Portao("hook instalado", False,
                           "nenhum Stop aponta para loop-stop.py em %s" % caminho,
                           "rode ./install.sh na raiz do skill-LOOP"))
        return fora
    else:
        fora.append(Portao("hook instalado", None,
                           "não deu para ler %s — não sei dizer" % caminho, None))

    if not loop.existe:
        fora.append(Portao(".loop/", False, "sem STATE.json em %s" % loop.dir,
                           "loop-ctl armar --raiz %s" % loop.raiz))
        return fora
    fora.append(Portao(".loop/", True, loop.dir, None))

    if not st:
        fora.append(Portao("STATE.json", False, "ilegível ou não é objeto JSON",
                           "loop-ctl armar --raiz %s (reescreve o estado)" % loop.raiz))
        return fora

    if not st.get("ativo"):
        detalhe = "false"
        if st.get("encerrado_por"):
            detalhe += " — encerrado por %r em %s" % (st["encerrado_por"],
                                                      st.get("encerrado_em") or "?")
        fora.append(Portao("ativo", False, detalhe,
                           "loop-ctl retomar --raiz %s (ou armar, se a rodada "
                           "acabou de verdade)" % loop.raiz))
        return fora
    fora.append(Portao("ativo", True, "true", None))

    if st.get("fase") == "encerrando":
        fora.append(Portao("fase", False,
                           "encerrando — a próxima parada encerra de vez, "
                           "sem classificar (a notificação já foi pedida)",
                           "loop-ctl retomar --raiz %s" % loop.raiz))
        return fora
    fora.append(Portao("fase", True, st.get("fase") or "rodando", None))

    fora.append(_portao_sessao(loop, st, sid))
    return fora


def _portao_sessao(loop, st, sid):
    """A amarração do ADR-008 vista de fora do hook.

    Sem `sid` não há veredito: o `session_id` de hoje é dado da sessão viva, e
    adivinhá-lo pelo mtime dos transcripts foi exatamente o caminho que o
    ADR-008 descartou como frágil e indireto. Então: amarração é **fato**,
    consequência é **inferência**, e cada uma sai rotulada como o que é.
    """
    preso = st.get("session_id")
    if not st.get("bind_session"):
        return Portao("amarração", True, "bind_session: false — qualquer sessão "
                                         "no repositório dirige o loop", None)
    if not preso:
        return Portao("amarração", True,
                      "livre — a primeira parada amarra a sessão que armou", None)
    if sid and sid != preso:
        # Ids diferentes que encurtam igual virariam "preso a X; esta é X" — a
        # mensagem que mais precisa ser legível dizendo coisa nenhuma.
        a, b = ((curto(preso), curto(sid)) if curto(preso) != curto(sid)
                else (preso, sid))
        return Portao("amarração", False,
                      "preso à sessão %s; esta é %s — o hook sai em silêncio"
                      % (a, b),
                      "loop-ctl retomar --raiz %s (re-amarra na próxima parada)"
                      % loop.raiz)
    if sid:
        return Portao("amarração", True, "sessão %s confere" % curto(sid), None)
    return Portao("amarração", None,
                  "preso à sessão %s — se a sessão de hoje for outra, o hook "
                  "sai em silêncio (passe --sessao para eu conferir)" % curto(preso),
                  "loop-ctl retomar --raiz %s re-amarra" % loop.raiz)


def curto(sid):
    return (str(sid)[:8] + "…") if sid and len(str(sid)) > 8 else str(sid)


# ── condições de fim: a cadeia do hook, em um só lugar ──────────────────────
def tem_relogio(st):
    """A rodada foi armada por **tempo**? (ADR-015)

    É o que decide se `fila zerada` encerra ou vira turno de reabastecimento.
    Derivado de campos que já existem — `duracao_max_min` e `janela` —, e não de
    flag nova: quem escreveu `--duracao 6h` já declarou que a missão é o relógio
    e a fila é rascunho. Estado de versão anterior lê `None` nos dois e cai no
    comportamento antigo, que é o certo para rodada sem relógio.
    """
    return bool(st.get("duracao_max_min") or st.get("janela"))


def _primeira_linha(texto):
    """Primeira linha não vazia — o detalhe cabe numa linha do STATUS/painel."""
    for linha in (texto or "").splitlines():
        if linha.strip():
            return linha.strip()[:200]
    return "veredito não escrito em .loop/SEM-ESCOPO"


def condicoes_de_fim(loop, st, res=None, texto=None, irreversivel=None,
                     contagem=None, iteracao=None):
    """`(motivo, detalhe)` da primeira condição que encerraria, ou `None`.

    Esta é **a** cadeia do hook (SPEC.md §5), na ordem em que ele a testa —
    kill-switch primeiro porque é o comando explícito do dono, e política de ASK
    por último porque é a única que depende de classificar a mensagem.

    **`fila zerada` só encerra rodada sem relógio** (ADR-015). Com `--duracao` ou
    `--janela`, a fila vazia não é fim: é o gatilho do turno de reabastecimento,
    e quem fecha a rodada é o relógio, o `escopo esgotado` escrito pelo agente,
    ou os tetos. Enquanto a fila mandava, o `--duracao` nunca chegava a valer —
    três rodadas do EOP encerraram na iteração 1 com ~5h50 sobrando.

    - `contagem`: `(pendentes, feitos)` já lido. O hook passa o seu, colhido
      **depois** da colheita de itens; recontar aqui daria número de antes.
    - `iteracao`: qual iteração está fechando. O hook já incrementou o estado,
      então omite; quem só está diagnosticando passa `iteracao + 1` para
      perguntar "e na próxima parada?".
    - `res` / `texto` / `irreversivel`: sem classificação não há veredito de ASK,
      e as duas últimas condições saem de cena. O léxico de ação sem volta fica
      no hook (SECURITY.md aponta para ele lá) e entra aqui como função.
    """
    it = st.get("iteracao", 0) if iteracao is None else iteracao
    pendentes, feitos = contagem if contagem is not None else loop.contagem_fila()

    if loop.kill_switch:
        return "kill-switch", "existe .loop/STOP"
    if it > st.get("max_iteracoes", 200):
        return "teto de iterações", "%d atingido" % st.get("max_iteracoes", 200)
    if st.get("sem_progresso", 0) >= st.get("max_sem_progresso", 3):
        return ("sem progresso",
                "%d paradas sem mudar árvore nem fila — loop degenerado"
                % st.get("sem_progresso", 0))
    if loop.sem_escopo:
        return "escopo esgotado", _primeira_linha(loop.veredito_sem_escopo())
    if pendentes == 0 and not tem_relogio(st):
        return "fila zerada", "%d item(ns) concluído(s)" % feitos
    if fora_da_janela(st.get("janela"), st.get("dias")):
        return ("fora da janela de trabalho",
                "janela %s%s" % (st.get("janela"),
                                 " (%s)" % st["dias"] if st.get("dias") else ""))
    if st.get("duracao_max_min") and \
            minutos_desde(st.get("armado_em")) >= st["duracao_max_min"]:
        return ("duração máxima",
                "%d min de relógio desde que armou" % st["duracao_max_min"])
    if st.get("escopo_itens") and \
            (feitos - st.get("feitos_ao_armar", 0)) >= st["escopo_itens"]:
        return ("escopo concluído",
                "%d item(ns) fechados nesta rodada" % st["escopo_itens"])
    if st.get("escopo_ate") and loop.feito_contem(st["escopo_ate"]):
        return "escopo concluído", "marcador alcançado: %s" % st["escopo_ate"]
    if res is not None and getattr(res, "kind", None) == "ASK":
        if st.get("politica_ask") == "parar":
            return "política ASK=parar", res.sinal
        if st.get("politica_ask") == "continuar-exceto-irreversivel" and irreversivel:
            gatilhos = irreversivel(texto)
            if gatilhos:
                return "ação irreversível", gatilhos[0]
    return None
