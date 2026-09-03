#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`loop-watch` — acompanhar o loop de longe.

    python3 loop_watch.py                 # atualiza a cada 30 s
    python3 loop_watch.py -n 10           # outro intervalo
    python3 loop_watch.py --uma-vez       # imprime uma vez e sai (para cron/CI)
    python3 loop_watch.py --ate-encerrar  # sai quando o loop encerrar
    python3 loop_watch.py --raiz ~/x/EOP  # de qualquer lugar

Existe porque `watch -n 30 loop_ctl.py status` re-renderiza a mesma tela e
**não responde as duas perguntas de quem está longe do monitor**: *andou?* e
*quanto falta?*. Aqui as duas ficam explícitas — delta desde a última leitura,
e o tempo restante de cada condição de fim, com a que vai bater primeiro
marcada.

Python 3, stdlib apenas. Sem cor quando a saída não é terminal, então
`loop_watch.py --uma-vez >> registro.log` sai limpo.
"""

import argparse
import os
import re
import sys
import time
from datetime import datetime

sys.path.insert(0, os.path.join(os.path.dirname(os.path.realpath(__file__)), "lib"))

from diagnostico import (condicoes_de_fim, curto,          # noqa: E402
                         tem_relogio)
from estado import (NUM_DE_ENTRY, Loop, achar_raiz, dur,   # noqa: E402
                    minutos_ate_fechar, minutos_desde,
                    objetivo_para_exibir)

# ── cor ─────────────────────────────────────────────────────────────────────
class C(object):
    RESET = "\033[0m"; DIM = "\033[2m"; NEG = "\033[1m"
    VERDE = "\033[32m"; AMAR = "\033[33m"; VERM = "\033[31m"
    AZUL = "\033[36m"; ROXO = "\033[35m"

    @classmethod
    def desligar(cls):
        for k in list(vars(cls)):
            if k.isupper():
                setattr(cls, k, "")


_ISO = re.compile(r"^(\d{4})-(\d{2})-(\d{2})T(\d{2}:\d{2})")


def carimbo(ts):
    """`2026-08-17T12:56:03-03:00` → `17/08/2026-12:56`.

    O painel mostrava só `hh:mm`. Em 17/08 as quatro últimas paradas saíram como
    `09:32 · 09:03 · 21:19 · 20:24` — as duas de baixo eram do **dia anterior** e
    nada na tela dizia isso. Hora sem data, num registro que atravessa a
    meia-noite, é um número que parece informação.

    Formato malformado devolve o texto cru truncado em vez de data inventada: o
    painel pode não saber ler um carimbo, mas não pode fabricar um.
    """
    m = _ISO.match(ts or "")
    if not m:
        return (ts or "?")[:16]
    ano, mes, dia, hm = m.groups()
    return "%s/%s/%s-%s" % (dia, mes, ano, hm)


def barra(feitos, total, largura=22):
    if total <= 0:
        return "·" * largura
    cheio = int(round(largura * feitos / float(total)))
    return "█" * cheio + "░" * (largura - cheio)


# ── leitura das entries ─────────────────────────────────────────────────────
_CAMPO = re.compile(r"^(\w+):\s*(.*)$")


def _minutos_entre(antes, depois):
    """Minutos entre dois carimbos ISO, ou `None` se algum não der para ler."""
    try:
        return (datetime.fromisoformat(depois)
                - datetime.fromisoformat(antes)).total_seconds() / 60.0
    except (ValueError, TypeError):
        return None


def ultimas_paradas(loop, quantas=4):
    # Lê uma parada A MAIS do que vai mostrar: o intervalo da linha mais antiga
    # da tela precisa da anterior a ela, que já saiu da janela. Sem isso a
    # primeira linha nunca teria duração, justamente a que o operador olha
    # quando quer saber "quanto tempo faz que isso começou".
    try:
        nomes = sorted(os.listdir(loop.entries))[-(quantas + 1):]
    except (IOError, OSError):
        return []
    out = []
    for nome in nomes:
        dados = {"arquivo": nome}
        try:
            # `errors="replace"`: o painel lê um diretório que **outro processo
            # escreve**, e a cada 30 s. Entry pega no meio da gravação decodifica
            # com byte cortado, e `UnicodeDecodeError` é `ValueError` — passava
            # por baixo do `except (IOError, OSError)` e derrubava o painel
            # inteiro, com traceback, no lugar de uma linha ruim. Aconteceu em
            # 17/08 às 16:39:57, no refresh seguinte a um painel que renderizou
            # certo 30 s antes. Ferramenta de acompanhar de longe não pode morrer
            # por causa do arquivo que ela acompanha.
            with open(os.path.join(loop.entries, nome), encoding="utf-8",
                      errors="replace") as f:
                dentro = False
                for linha in f:
                    if linha.strip() == "---":
                        if dentro:
                            break
                        dentro = True
                        continue
                    m = _CAMPO.match(linha.strip()) if dentro else None
                    if m:
                        dados[m.group(1)] = m.group(2).strip().strip('"')
        except (IOError, OSError, UnicodeDecodeError):
            continue
        # O nome do arquivo vence o campo `n:` do front-matter — depois de lê-lo,
        # de propósito. Até 17/08 o hook gravava `n = iteracao + 1`, e `iniciar()`
        # zera a iteração a cada rodada: as entries de ontem e as de hoje trazem
        # réguas diferentes dentro de si. O painel mostrou `#4 #1 #2 #1` para
        # quatro paradas que no disco são 0001..0004. Front-matter é o que o
        # escritor achou na hora; o nome do arquivo é o que sobrevive à rodada.
        num = NUM_DE_ENTRY.match(nome)
        if num:
            dados["n"] = str(int(num.group(1)))
        out.append(dados)
    # `intervalo` é o tempo entre uma parada e a anterior — **fato medido**, não
    # "tempo de trabalho": o agente pode ter ficado esperando alguém digitar
    # entre as duas, e foi o que aconteceu entre a #5 e a #6 em 17/08 (o hook
    # tinha encerrado e o `retomar` veio meia hora depois). O painel mede o
    # relógio; quem infere produtividade a partir dele é quem lê.
    for i, d in enumerate(out):
        d["intervalo"] = (None if i == 0 else
                          _minutos_entre(out[i - 1].get("ts"), d.get("ts")))
    return out[-quantas:]


# ── render ──────────────────────────────────────────────────────────────────
def condicoes(loop, st, pendentes, feitos):
    """`[(motivo, rótulo, restante, minutos_ou_None)]` na ordem em que o hook testa.

    A ordem é a de `diagnostico.condicoes_de_fim` — kill-switch primeiro porque é
    o comando explícito do dono —, **não** a do relógio. Ordenar por tempo
    restante foi o defeito de 17/08 no EOP: o painel marcou `← primeira` na
    janela, faltando 2h18, enquanto *"fila zerada"* já estava verdadeira com 0
    pendentes duas linhas abaixo. O menor número só decide entre as condições que
    ainda **não** bateram; qual delas manda é a cadeia que decide.

    O `motivo` de cada linha é a chave que amarra o painel ao nome que a cadeia
    dá à mesma condição. Sem ela isto voltaria a ser uma lista paralela — a
    quarta cópia que o `diagnostico.py` foi escrito para impedir (ADR-013).
    """
    linhas = []
    linhas.append(("kill-switch", "kill-switch",
                   "PRESENTE" if loop.kill_switch else "ausente", None))
    restam_it = st.get("max_iteracoes", 0) - st.get("iteracao", 0)
    linhas.append(("teto de iterações", "iterações %d" % st.get("max_iteracoes", 0),
                   "restam %d" % max(0, restam_it), None))
    linhas.append(("sem progresso", "sem progresso",
                   "%d/%d parada(s)" % (st.get("sem_progresso", 0),
                                        st.get("max_sem_progresso", 0)), None))
    if loop.sem_escopo or tem_relogio(st):
        linhas.append(("escopo esgotado", "escopo esgotado",
                       "DECLARADO" if loop.sem_escopo
                       else "não (sem .loop/SEM-ESCOPO)", None))
    if tem_relogio(st):
        # Sob relógio a fila vazia NÃO encerra (ADR-015) — e dizer "0 pendente(s)"
        # nesta linha, calado, foi exatamente como o painel de 17/08 apontou o fim
        # para o lugar errado. A linha fica, porque quem lê quer saber da fila; o
        # que ela não pode mais é parecer condição de fim.
        linhas.append((None, "fila (não encerra)",
                       "%d pendente(s) → reabastece" % pendentes, None))
    else:
        linhas.append(("fila zerada", "fila zerada",
                       "%d pendente(s)" % pendentes, None))
    if st.get("janela"):
        falta = minutos_ate_fechar(st["janela"], st.get("dias"))
        rot = "janela %s%s" % (st["janela"],
                               " (%s)" % st["dias"] if st.get("dias") else "")
        linhas.append(("fora da janela de trabalho", rot,
                       "fecha em %s" % dur(falta), falta))
    if st.get("duracao_max_min"):
        resta = st["duracao_max_min"] - minutos_desde(st.get("armado_em"))
        linhas.append(("duração máxima", "relógio %s" % dur(st["duracao_max_min"]),
                       "resta %s" % dur(resta), resta))
    if st.get("escopo_itens"):
        fechados = feitos - st.get("feitos_ao_armar", 0)
        falta_n = st["escopo_itens"] - fechados
        linhas.append(("escopo concluído", "escopo %d itens" % st["escopo_itens"],
                       "faltam %d" % max(0, falta_n), None))
    if st.get("escopo_ate"):
        linhas.append(("escopo concluído", "marcador %r" % st["escopo_ate"][:28],
                       "—", None))
    return linhas


def _linha_do_motivo(linhas, motivo, detalhe):
    """Índice da linha que corresponde ao motivo — ou `None` se ele não tem linha.

    `escopo concluído` nomeia **duas** condições distintas (N itens × marcador
    alcançado), e só o detalhe as separa: a cadeia diz `"marcador alcançado: X"`
    numa e `"N item(ns) fechados nesta rodada"` na outra. Casar só pelo motivo
    marcaria a linha errada metade das vezes.
    """
    candidatos = [i for i, l in enumerate(linhas) if l[0] == motivo]
    if not candidatos:
        return None
    if len(candidatos) > 1 and (detalhe or "").startswith("marcador"):
        return candidatos[-1]
    return candidatos[0]


def quem_encerra(loop, st, pendentes, feitos):
    """`(motivo, detalhe, marca)` — quem manda no fim **agora**.

    São três perguntas diferentes, e o painel respondia sempre a terceira:

    1. a rodada já morreu? então o motivo é fato gravado (`encerrado_por`), e
       projeção de futuro não tem o que dizer sobre ela;
    2. está viva, mas alguma condição **já** é verdadeira? então a próxima
       parada encerra — isso é aviso, não previsão. Vale `iteracao + 1` porque a
       pergunta de quem acompanha é *"e na próxima parada?"*, e o estado em
       disco é o da parada que já passou;
    3. nenhuma bateu? aí, e só aí, vale a que chega primeiro no relógio.
    """
    if st.get("encerrado_por"):
        return (st["encerrado_por"], st.get("encerrado_detalhe") or "",
                "← encerrou aqui")
    achado = condicoes_de_fim(loop, st, contagem=(pendentes, feitos),
                              iteracao=st.get("iteracao", 0) + 1)
    if achado:
        return achado[0], achado[1], "← já bateu: a próxima parada encerra"
    return None, None, None


def render(loop, st, anterior):
    pend, feitos = loop.contagem_fila()
    total = pend + feitos
    it = st.get("iteracao", 0)
    ativo = st.get("ativo")
    fase = st.get("fase")

    if not ativo:
        cor, estado = C.DIM, "PARADO"
        if st.get("encerrado_por"):
            # "há 10min" no cabeçalho porque o painel é lido de longe e a hora
            # que ele carimba é a da LEITURA, não a do fim: em 17/08 um painel
            # das 09:42 mostrava uma rodada morta às 09:32 sem nada dizendo que
            # havia dez minutos entre as duas coisas.
            estado = "ENCERRADO · %s%s" % (
                st["encerrado_por"],
                " há %s" % dur(minutos_desde(st["encerrado_em"]))
                if st.get("encerrado_em") else "")
            cor = C.AMAR
    elif fase == "encerrando":
        cor, estado = C.AMAR, "ENCERRANDO · %s" % (st.get("encerrado_por") or "")
    else:
        cor, estado = C.VERDE, "RODANDO"

    L = []
    # A data entra também no cabeçalho por causa do `--uma-vez >> registro.log`:
    # num arquivo que acumula por dias, `12:57:11` sozinho não diz de quando é.
    L.append("%s%s╭─ loop-work · %s ─ %s%s" % (
        C.NEG, C.AZUL, os.path.basename(loop.raiz),
        time.strftime("%d/%m/%Y-%H:%M:%S"), C.RESET))
    L.append("  %s%s%s   iteração %d/%d"
             % (cor + C.NEG, estado, C.RESET, it, st.get("max_iteracoes", 0)))
    # O painel dizia "PARADO" e o operador lia "está entre duas iterações".
    # Parado é inerte: o hook sai no primeiro portão e o chat não tem como
    # reativá-lo — foi assim que um "continua" digitado em 17/08 virou um turno
    # de 2min30 e mais nada. `porque` no lugar de `armar`/`retomar` porque o
    # verbo certo depende do portão, e só ele sabe qual é.
    if not ativo or fase == "encerrando":
        L.append("  %s%s⚠ hook inerte — \"continua\" no chat não reativa%s"
                 % (C.VERM, C.NEG, C.RESET))
        L.append("    %so que fazer: loop-ctl porque --raiz %s%s"
                 % (C.VERM, loop.raiz, C.RESET))
    if st.get("bind_session") and st.get("session_id"):
        L.append("  %ssessão %s — outra sessão no mesmo repo é ignorada%s"
                 % (C.DIM, curto(st["session_id"]), C.RESET))
    if st.get("objetivo"):
        L.append("  %s%s%s" % (C.DIM, objetivo_para_exibir(st["objetivo"], 96),
                               C.RESET))
    L.append("")
    L.append("  Fila   %s  %d/%d" % (barra(feitos, total), feitos, total)
             + ("  (%d%%)" % round(100.0 * feitos / total) if total else ""))
    prox = loop.proximo_item()
    L.append("  Agora  %s→ %s%s" % (C.NEG, (prox or "—")[:88], C.RESET))
    L.append("")

    linhas = condicoes(loop, st, pend, feitos)
    motivo, detalhe, marca = quem_encerra(loop, st, pend, feitos)
    alvo = None
    if motivo:
        alvo = _linha_do_motivo(linhas, motivo, detalhe)
        if alvo is None:
            # Condição sem linha própria no painel — política de ASK e ação
            # irreversível dependem de classificar a mensagem, então o painel
            # não as mede. Entram como linha assim mesmo: o painel pode não
            # saber prever um fim, mas nunca pode deixar de dizer qual foi.
            linhas.append((motivo, motivo, detalhe or "—", None))
            alvo = len(linhas) - 1
    else:
        com_tempo = [(i, l) for i, l in enumerate(linhas) if l[3] is not None]
        if com_tempo:
            alvo = min(com_tempo, key=lambda x: x[1][3])[0]
            marca = "← primeira"

    cabeca = "Fim por"
    if st.get("encerrado_por"):
        cabeca += "   %sa rodada acabou — o que resta abaixo é tempo que não " \
                  "corre mais%s" % (C.DIM, C.RESET)
    L.append("  %s%s%s" % (C.DIM, cabeca, C.RESET))
    for i, (mot, rot, resta, _min) in enumerate(linhas):
        perigo = (mot == "kill-switch" and loop.kill_switch) or \
                 (mot == "sem progresso" and st.get("sem_progresso", 0))
        cor = C.VERM if perigo else ""
        if i == alvo:
            cor_marca = C.VERM if st.get("encerrado_por") else C.AMAR
            sufixo = "  %s%s%s" % (cor_marca, marca, C.RESET)
        else:
            sufixo = ""
        L.append("    %s%-30s %s%s%s" % (cor, rot, resta,
                                         C.RESET if cor else "", sufixo))

    paradas = ultimas_paradas(loop)
    if paradas:
        L.append("")
        L.append("  %sÚltimas paradas%s" % (C.DIM, C.RESET))
        for p in reversed(paradas):
            k = p.get("kind", "?")
            cor_k = C.ROXO if k == "ASK" else C.DIM
            aviso = ""
            if p.get("fecho_do_turno") == "PARCIAL":
                aviso = "  %s⚠ fecho parcial%s" % (C.VERM, C.RESET)
            elif k == "ASK":
                aviso = "  %s⚠ premissa registrada%s" % (C.AMAR, C.RESET)
            gap = p.get("intervalo")
            desde = "" if gap is None else "  %s+%s%s" % (
                C.DIM, "<1min" if gap < 1 else dur(gap), C.RESET)
            L.append("    #%-5s %s%-4s%s %-16s %-22s %s%s%s"
                     % (p.get("n", "?"), cor_k, k, C.RESET, p.get("sinal", "")[:16],
                        p.get("decisao", "")[:22], carimbo(p.get("ts")), desde, aviso))

    if anterior:
        d_it = it - anterior["it"]
        d_feitos = feitos - anterior["feitos"]
        if d_it or d_feitos:
            L.append("")
            L.append("  %s%sDesde a última leitura: +%d parada(s), +%d item(ns) fechado(s)%s"
                     % (C.VERDE, C.NEG, d_it, d_feitos, C.RESET))
        elif ativo:
            L.append("")
            L.append("  %ssem mudança desde a última leitura%s" % (C.DIM, C.RESET))

    L.append("%s╰%s" % (C.AZUL, C.RESET))
    return "\n".join(L), {"it": it, "feitos": feitos}


def imprimir_status_final(loop):
    caminho = loop.p("STATUS.md")
    if not os.path.exists(caminho):
        return
    # Mesma regra da leitura das entries: o `--ate-encerrar` imprime isto no
    # instante em que o loop encerra, que é exatamente quando o hook acabou de
    # gravar o arquivo. Morrer aqui perderia o motivo do encerramento — o único
    # dado que o operador está esperando.
    try:
        with open(caminho, encoding="utf-8", errors="replace") as f:
            print("\n" + f.read())
    except (IOError, OSError):
        pass


def main(argv=None):
    ap = argparse.ArgumentParser(prog="loop-watch", description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("-n", "--intervalo", type=float, default=30.0)
    ap.add_argument("--raiz", default=None)
    ap.add_argument("--uma-vez", action="store_true")
    ap.add_argument("--ate-encerrar", action="store_true",
                    help="sai (código 0) quando o loop encerrar")
    ap.add_argument("--sem-cor", action="store_true")
    ap.add_argument("--sem-limpar", action="store_true",
                    help="não limpa a tela — o histórico rola, útil para log")
    args = ap.parse_args(argv)

    if args.sem_cor or not sys.stdout.isatty():
        C.desligar()

    raiz = os.path.abspath(os.path.expanduser(args.raiz)) if args.raiz \
        else achar_raiz(os.getcwd())
    if not raiz:
        print("sem .loop/ aqui nem acima — use --raiz, ou arme com /loop-work")
        return 1
    loop = Loop(raiz)

    anterior = None
    while True:
        st = loop.ler()
        if not st:
            print("sem .loop/STATE.json em %s" % loop.dir)
            return 1
        texto, anterior = render(loop, st, anterior)
        if not args.sem_limpar and sys.stdout.isatty() and not args.uma_vez:
            sys.stdout.write("\033[H\033[J")
        print(texto, flush=True)

        encerrou = not st.get("ativo") or st.get("fase") == "encerrando"
        if args.uma_vez:
            return 0
        if encerrou and args.ate_encerrar:
            if sys.stdout.isatty():
                sys.stdout.write("\a")      # sino: você pode estar em outra janela
                sys.stdout.flush()
            imprimir_status_final(loop)
            return 0
        try:
            time.sleep(max(1.0, args.intervalo))
        except KeyboardInterrupt:
            print()
            return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print()
        sys.exit(0)
