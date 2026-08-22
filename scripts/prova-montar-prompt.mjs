#!/usr/bin/env node
// Provas do claude-runner: a montagem do pedido (`montarPrompt`) e a TRADUÇÃO DE
// EVENTO (`traduzirMensagem`), que é o trecho que falha calado.
//
// ⚠️ POR QUE ESTA PROVA EXISTE. O `claude-runner.mjs` tem 367 linhas e não tinha
// prova nenhuma: ele é a metade do Modo Code que roda fora do gateway, com a
// assinatura do dono, e até aqui a única verificação era abrir o app e olhar. A
// imagem chegou nele em 1.1.30 e trouxe a família de defeito que esta casa passou
// a semana consertando — a que falha em SILÊNCIO. Três formas, todas verdes na
// tela:
//
//   1. o turno de TEXTO passar a viajar como iterável "para uniformizar" — muda o
//      caminho de 100% dos pedidos por causa de um caso que talvez nunca aconteça;
//   2. o bloco de texto sair ANTES da imagem — o modelo lê o pedido e depois a
//      figura, e responde sobre a figura errada num turno com várias;
//   3. `text: ""` virar bloco vazio quando se cola imagem sem escrever nada — o
//      SDK recusa, e o erro volta como falha do turno, não como "faltou texto".
//
// Nenhuma das três aparece no `node --check`. Esta prova roda a função REAL,
// extraída do arquivo em produção — testar uma cópia provaria a cópia.
//
// ⚠️ A SEGUNDA METADE, e por que ela importa mais. `traduzirMensagem` transforma
// mensagem do Agent SDK nos eventos que a página consome. Quando a forma de um
// evento do SDK muda, nenhum `case` casa — **nada é emitido e nada falha**. A tela
// do Modo Code emudece e o turno "termina" sem uma linha. Não há exceção, não há
// log, não há vermelho em lugar nenhum: é o defeito perfeito. Cada régua abaixo
// fixa a forma EXATA de um evento; quebrar o contrato passa a doer aqui, em vez
// de doer numa sessão do dono.
//
// Uso: `npm run prova:runner` (sai != 0 se qualquer régua cair).
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fonte = readFileSync(resolve(ROOT, "claude-runner/claude-runner.mjs"), "utf8");

// Recorta a função real. Se o recorte falhar, é ERRO — prova que não achou o que
// mede é prova que não mede, e sair verde aqui seria o defeito de novo.
const m = fonte.match(/^function montarPrompt\(text, images\) \{\n([\s\S]*?)\n\}$/m);
if (!m) {
  console.error("[prova-runner] não achei montarPrompt() em claude-runner.mjs");
  process.exit(1);
}
let sessionId = "sess-1"; // o módulo real tem esta variável no escopo de cima
// `sessionId` entra como PARÂMETRO porque no módulo real ele é uma variável do
// escopo de cima (linha 215) — recortar a função a deixaria livre e o ReferenceError
// só apareceria no caso com imagem, que é justamente o que se quer medir.
const montarPrompt = new Function("text", "images", "sessionId", m[1]);

const t = fonte.match(/^function traduzirMensagem\(message, estado, modelo\) \{\n([\s\S]*?)\n\}$/m);
if (!t) {
  console.error("[prova-runner] não achei traduzirMensagem() em claude-runner.mjs");
  process.exit(1);
}
const traduzirMensagem = new Function("message", "estado", "modelo", t[1]);
const traduzir = (msg, est = { sessionId: undefined, sawTextDelta: false }) =>
  traduzirMensagem(msg, est, "opus");

const falhas = [];
const conferir = (regua, ok, obtido) => {
  if (ok) return;
  falhas.push(`${regua} — obtido: ${JSON.stringify(obtido)}`);
};

const IMG = [{ mime: "image/png", dataBase64: "AAA" }];
const blocosDe = async (p) => {
  for await (const msg of p) return msg.message.content;
  return null;
};

// 1. sem imagem o pedido continua sendo STRING — o caminho de todo turno de texto.
const semImg = montarPrompt("só texto", [], sessionId);
conferir("sem imagem → string crua", typeof semImg === "string" && semImg === "só texto", semImg);
conferir("images ausente → string crua", typeof montarPrompt("x", undefined, sessionId) === "string", null);

// 2. com imagem vira iterável de UM item, e a IMAGEM VEM ANTES do texto.
const comImg = montarPrompt("olha isto", IMG, sessionId);
conferir("com imagem → async iterable", typeof comImg?.[Symbol.asyncIterator] === "function", typeof comImg);
const blocos = await blocosDe(comImg);
conferir("imagem antes do texto", blocos?.map((b) => b.type).join(",") === "image,text",
  blocos?.map((b) => b.type));
conferir("mime viaja no campo próprio", blocos?.[0]?.source?.media_type === "image/png", blocos?.[0]?.source);
conferir("base64 vai cru, sem o prefixo data:", blocos?.[0]?.source?.data === "AAA", blocos?.[0]?.source?.data);
conferir("o texto é o último que o modelo lê", blocos?.[1]?.text === "olha isto", blocos?.[1]);

// 3. imagem colada SEM pedido é um pedido legítimo — e não pode virar bloco vazio.
const soImg = await blocosDe(montarPrompt("   ", IMG, sessionId));
conferir("texto em branco não vira bloco vazio", soImg?.length === 1 && soImg[0].type === "image", soImg);

// ── traduzirMensagem: a forma EXATA de cada evento ───────────────────────────

// `init` é onde o sessionId nasce. Perdê-lo faz o turno seguinte reiniciar a
// conversa em silêncio — o modelo responde do zero e nada acusa.
const ini = traduzir({ type: "system", subtype: "init", session_id: "s-9", model: "claude-opus-5" });
conferir("init guarda o sessionId", ini.estado.sessionId === "s-9", ini.estado);
conferir("init emite `model`", ini.eventos[0]?.type === "model", ini.eventos[0]);
conferir("init leva o modelo do SDK", ini.eventos[0]?.model === "claude-opus-5", ini.eventos[0]);

// Sem `model` no evento, cai no --model da linha de comando — nunca em branco:
// chip vazio no topo do Modo Code parece motor não iniciado.
const semModelo = traduzir({ type: "system", subtype: "init", session_id: "s" });
conferir("sem modelo no evento, usa o --model", semModelo.eventos[0]?.model === "opus", semModelo.eventos[0]);

// `system` que NÃO é init não pode emitir nada — senão o chip do modelo pisca a
// cada mensagem de serviço do SDK.
conferir("system não-init é silencioso", traduzir({ type: "system", subtype: "outro" }).eventos.length === 0, null);

// O delta de texto é o que faz a resposta aparecer letra a letra.
const d = traduzir({ type: "stream_event", event: { type: "content_block_delta", delta: { type: "text_delta", text: "oi" } } });
conferir("text_delta vira {type:text,delta}", JSON.stringify(d.eventos) === '[{"type":"text","delta":"oi"}]', d.eventos);
conferir("text_delta marca sawTextDelta", d.estado.sawTextDelta === true, d.estado);

// Delta de OUTRO tipo (thinking, input_json) não pode virar texto na tela.
const outroDelta = traduzir({ type: "stream_event", event: { type: "content_block_delta", delta: { type: "thinking_delta", thinking: "hmm" } } });
conferir("delta que não é de texto não vira texto", outroDelta.eventos.length === 0, outroDelta.eventos);

// O bloco assistant traz o texto COMPLETO. Depois dos deltas ele duplicaria a
// resposta na tela; é fallback, não caminho normal.
const dep = traduzir({ type: "assistant", message: { content: [{ type: "text", text: "tudo" }] } }, { sessionId: "s", sawTextDelta: true });
conferir("assistant NÃO repete o texto se já houve deltas", dep.eventos.length === 0, dep.eventos);
const sem = traduzir({ type: "assistant", message: { content: [{ type: "text", text: "tudo" }] } });
conferir("sem deltas, o assistant emite o texto", sem.eventos[0]?.delta === "tudo", sem.eventos);

// tool_call: os três campos que a página usa para desenhar a chamada.
const tc = traduzir({ type: "assistant", message: { content: [{ type: "tool_use", id: "t1", name: "read_file", input: { path: "a.php" } }] } });
conferir("tool_use vira tool_call com id/name/arguments",
  JSON.stringify(tc.eventos) === '[{"type":"tool_call","id":"t1","name":"read_file","arguments":{"path":"a.php"}}]', tc.eventos);

// tool_result com content em ARRAY (o SDK manda os dois formatos) não pode virar
// "[object Object]" na tela.
const tr = traduzir({ type: "user", message: { content: [{ type: "tool_result", tool_use_id: "t1", content: [{ type: "text", text: "conteúdo" }] }] } });
conferir("tool_result de array vira JSON, não [object Object]",
  tr.eventos[0]?.content?.includes("conteúdo") && !tr.eventos[0]?.content?.includes("[object"), tr.eventos[0]);
conferir("tool_result conta bytes do que foi enviado",
  tr.eventos[0]?.bytes === Buffer.byteLength(tr.eventos[0]?.content ?? ""), tr.eventos[0]);

// `result` fecha o turno. Sem `turn_done` a interface fica presa em "pensando"
// para sempre, sem erro nenhum — o pior desfecho possível deste arquivo.
const fim = traduzir({ type: "result", usage: { input_tokens: 10, output_tokens: 5 }, total_cost_usd: 0.02 });
conferir("result soma os tokens", fim.eventos[0]?.tokens === 15, fim.eventos[0]);
conferir("custo vai marcado como ESTIMADO (assinatura não fatura por token)",
  fim.eventos[0]?.estimated === true, fim.eventos[0]);
conferir("result SEMPRE fecha com turn_done",
  fim.eventos[fim.eventos.length - 1]?.type === "turn_done", fim.eventos);

// E o erro não pode engolir o fechamento: sem `turn_done`, a tela trava mesmo
// tendo mostrado o erro.
const err = traduzir({ type: "result", subtype: "error", result: "estourou" });
conferir("result de erro emite error E turn_done",
  err.eventos.map((e) => e.type).join(",") === "usage,error,turn_done", err.eventos.map((e) => e.type));

// Mensagem de tipo desconhecido não pode explodir nem inventar evento: SDK novo
// manda tipos que este runner não conhece, e derrubar o turno por isso seria
// trocar uma funcionalidade que falta por uma sessão perdida.
conferir("tipo desconhecido é ignorado sem estourar", traduzir({ type: "coisa_nova" }).eventos.length === 0, null);
conferir("mensagem nula é ignorada sem estourar", traduzir(null).eventos.length === 0, null);

// ── `--version`: a sonda de que o desktop depende ────────────────────────────
//
// ⚠️ Não é detalhe de conveniência. O `engine_status()` do desktop roda o binário
// com `--version` e trata a saída como o NÚMERO. Até a 1.1.32 o runner ignorava a
// flag, subia em modo host, recebia EOF no stdin e saía — devolvendo NDJSON de
// arranque, que a ponte mostraria como se fosse a versão. Sonda que responde
// qualquer coisa é pior que sonda que não responde: "encontrado, mas não
// respondeu" o desktop sabe classificar; lixo exibido como fato, não.
//
// E o subprocesso roda SEM `npm install` neste repo, de propósito: é assim que se
// prova que o `--version` não depende do SDK.
{
  const { execFileSync } = await import("node:child_process");
  const pkg = JSON.parse(readFileSync(resolve(ROOT, "claude-runner/package.json"), "utf8"));
  let saida = "";
  try {
    saida = execFileSync(process.execPath, [resolve(ROOT, "claude-runner/claude-runner.mjs"), "--version"],
      { encoding: "utf8", timeout: 15000 }).trim();
  } catch (e) {
    saida = `ERRO: ${e?.message ?? e}`;
  }
  conferir("--version responde e sai com 0", saida.startsWith("claude-runner "), saida);
  // ⚠️ O que esta régua prova E o que ela NÃO prova. Ela lê o `package.json` do
  // runner e compara — então prova que o runner REPORTA o próprio portador, não
  // que o portador está em dia. Dessincronizar os dois juntos passa aqui (medido:
  // a reversão voltou verde). Quem prova a sincronia é o `npm run prova:bump`,
  // onde `claude-runner/package.json` entrou como portador na 1.1.33 — e lá a
  // mesma reversão derruba o build nomeando o arquivo. Duas provas, cada uma com
  // a sua metade; escrever aqui que esta cobre as duas seria o verde mentindo.
  conferir("--version reporta o portador do próprio runner",
    saida === `claude-runner ${pkg.version}`, saida);
  conferir("--version não vaza NDJSON de arranque", !saida.includes('{"type"'), saida);
}

if (falhas.length) {
  console.error(`[prova-runner] FALHOU: ${falhas.length} régua(s)`);
  for (const f of falhas) console.error(`  ✗ ${f}`);
  process.exit(1);
}
console.log("[prova-runner] OK: 8 réguas do montarPrompt + 19 da traduzirMensagem + 3 do `--version` (forma de cada evento, o fallback de texto e o turn_done que destrava a tela).");
