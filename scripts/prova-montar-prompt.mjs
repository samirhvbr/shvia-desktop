#!/usr/bin/env node
// Prova do `montarPrompt` do claude-runner — a montagem do pedido COM IMAGEM.
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

if (falhas.length) {
  console.error(`[prova-runner] FALHOU: ${falhas.length} régua(s)`);
  for (const f of falhas) console.error(`  ✗ ${f}`);
  process.exit(1);
}
console.log("[prova-runner] OK: 8 réguas do montarPrompt — string sem imagem, ordem dos blocos, mime/base64 e imagem sem texto.");
