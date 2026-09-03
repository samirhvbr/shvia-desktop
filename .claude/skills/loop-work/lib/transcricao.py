#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Leitura do transcript JSONL da sessão.

O hook `Stop` não recebe a mensagem do agente — recebe o **caminho** do
transcript. Ler o arquivo inteiro seria caro numa sessão de meses, então a
leitura é pela cauda: os últimos `MAX_BYTES`, descartando a primeira linha
(quase sempre partida no meio de um JSON).

Python 3, stdlib apenas.
"""

import json
import os
import time

MAX_BYTES = 2 * 1024 * 1024

# Espera pela mensagem final do turno (ADR-012). O hook `Stop` dispara ANTES de
# o Claude Code terminar de gravar o último bloco de texto no JSONL — medido em
# 16/08 no EOP: a parada às 00:19:22 leu texto de 00:12:30, 154 entradas atrás,
# e o relato verdadeiro tinha timestamp 00:19:22. Teto bem abaixo do timeout de
# 15 s do hook.
ESPERA_MAX_S = float(os.environ.get("LOOP_ESPERA_MAX_S", "3.0"))
INTERVALO_S = 0.1

# Tool que ENCERRA o turno por si: o agente pergunta e o turno para ali, à
# espera do humano. Não há texto de fecho para aguardar — esperar seria só
# gastar o teto à toa.
TOOLS_QUE_FECHAM = ("AskUserQuestion",)

# Tipos que são conteúdo de turno. O resto (attachment, system, mode,
# last-prompt, ai-title, permission-mode, bridge-session, file-history-delta…)
# é metadado do harness e não diz nada sobre o turno ter fechado.
CONTEUDO = ("assistant", "user")


def _linhas_da_cauda(caminho, max_bytes=MAX_BYTES):
    tamanho = os.path.getsize(caminho)
    with open(caminho, "rb") as f:
        if tamanho > max_bytes:
            f.seek(tamanho - max_bytes)
            f.readline()          # a primeira linha da janela está partida
        bruto = f.read()
    return bruto.decode("utf-8", "replace").split("\n")


def _varrer(linhas):
    """(texto, ultimo_tool, fresco) da última mensagem do agente principal.

    `fresco` responde a pergunta que decide tudo: **este texto é o fecho do
    turno, ou é resto velho?** É `False` quando existe conteúdo de turno depois
    dele — tool_use, tool_result — o que significa que o agente ainda estava
    trabalhando quando aquele texto saiu, e o fecho verdadeiro não chegou ao
    arquivo ainda.

    `isSidechain` é descartado: aquilo é subagente. Sem o filtro, o loop leria
    o relatório de um Explore e classificaria a coisa errada.
    """
    ultimo_tool = None
    depois = 0                      # entradas de conteúdo vistas após o candidato
    for linha in reversed(linhas):
        linha = linha.strip()
        if not linha or not linha.startswith("{"):
            continue
        try:
            obj = json.loads(linha)
        except ValueError:
            continue
        if obj.get("type") not in CONTEUDO:
            continue
        if obj.get("isSidechain"):
            continue                # subagente: não é o turno que encerrou
        if obj.get("type") != "assistant":
            depois += 1             # tool_result do agente principal
            continue
        blocos = (obj.get("message") or {}).get("content") or []
        if not isinstance(blocos, list):
            continue
        textos, tools = [], []
        for b in blocos:
            if not isinstance(b, dict):
                continue
            if b.get("type") == "text" and (b.get("text") or "").strip():
                textos.append(b["text"])
            elif b.get("type") == "tool_use" and b.get("name"):
                tools.append(b["name"])
        if tools and ultimo_tool is None:
            ultimo_tool = tools[-1]
        if textos:
            return "\n".join(textos).strip(), ultimo_tool, depois == 0
        depois += 1                 # entrada assistant só com tool_use
    return "", ultimo_tool, True


def ultima_mensagem(caminho, max_bytes=MAX_BYTES, espera_max_s=ESPERA_MAX_S,
                    intervalo_s=INTERVALO_S):
    """(texto, ultimo_tool, parcial) — o fecho do turno.

    Espera pelo fecho quando o que está no arquivo é resto velho. O hook `Stop`
    dispara antes de o Claude Code gravar o último bloco de texto; sem a espera,
    o produto arquiva um fragmento de meio de raciocínio e o classifica como se
    fosse o relatório — que é o defeito medido no EOP em 16/08 (ADR-012).

    `parcial=True` significa que a espera estourou e o texto devolvido NÃO é o
    fecho. O loop segue assim mesmo (fail-open: perder a mensagem não pode
    significar perder o trabalho), mas a `entry` registra que é parcial — dado
    duvidoso rotulado vale mais que dado duvidoso silencioso.
    """
    limite = time.time() + max(0.0, espera_max_s)
    melhor = ("", None, False)
    while True:
        try:
            linhas = _linhas_da_cauda(caminho, max_bytes)
        except (IOError, OSError):
            return "", None, False
        texto, tool, fresco = _varrer(linhas)
        if fresco or tool in TOOLS_QUE_FECHAM:
            return texto, tool, False
        melhor = (texto, tool)
        if time.time() >= limite:
            return melhor[0], melhor[1], True
        time.sleep(intervalo_s)
