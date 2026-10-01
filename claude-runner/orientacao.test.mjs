import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { criarOrientacao } from "./orientacao.mjs";

/**
 * Steering the turn in flight (1.11.0) in the Claude runner.
 *
 * The rules are unit-tested here on the pure module, with a fake Query that records what
 * `streamInput` receives. What reached the real model was measured once against the Agent SDK
 * (01/10/2026, see `orientacao.mjs`); CI does not install the SDK, so the wiring inside
 * `claude-runner.mjs` is a SOURCE check, and it says so.
 */
function montar() {
  const linhas = [];
  const fila = [];
  const o = criarOrientacao({ emitir: (ev) => linhas.push(ev), enfileirar: (t) => fila.push(t) });
  const enviados = [];
  const consulta = {
    streamInput: async (iteravel) => { for await (const m of iteravel) enviados.push(m); },
  };
  return { o, linhas, fila, enviados, consulta };
}
const inicio = (pai = null) => ({ type: "stream_event", event: { type: "message_start" }, parent_tool_use_id: pai });
const tick = () => new Promise((r) => setImmediate(r));

test("a steer during a turn goes into the turn's query, as a next-priority user message", async () => {
  const { o, enviados, consulta, fila } = montar();
  assert.equal(o.orientar("não mexa nos testes", consulta, "sess-1"), "injetada");
  await tick();
  assert.equal(enviados.length, 1);
  assert.equal(enviados[0].type, "user");
  assert.equal(enviados[0].priority, "next", "without `next` the SDK does not read it inside the turn");
  assert.deepEqual(enviados[0].message, { role: "user", content: "não mexa nos testes" });
  assert.equal(enviados[0].session_id, "sess-1");
  assert.deepEqual(fila, [], "an injected steer is not also queued");
});

test("it is confirmed only on the main conversation's NEXT request, never by a sub-agent's", async () => {
  const { o, linhas, consulta } = montar();
  o.orientar("pare de editar o README", consulta, "s");
  o.observar({ type: "assistant", message: { content: [{ type: "text", text: "..." }] } });
  o.observar(inicio("toolu_sub"));
  assert.deepEqual(linhas, [], "an assistant message or a sub-agent request is not proof the model read it");
  o.observar(inicio(null));
  assert.deepEqual(linhas, [{ type: "steer_applied", text: "pare de editar o README" }]);
  o.observar(inicio(null));
  assert.equal(linhas.length, 1, "confirmed once");
});

test("a steer the turn never read runs as the next message, once", () => {
  const { o, linhas, fila, consulta } = montar();
  o.orientar("e rode os testes", consulta, "s");
  o.encerrarTurno();
  assert.deepEqual(fila, ["e rode os testes"]);
  assert.deepEqual(linhas, [{ type: "steer_deferred", text: "e rode os testes" }]);
  o.encerrarTurno();
  assert.equal(fila.length, 1, "deferred once, not at every turn end");
});

test("with no turn running a steer is simply the next message; an empty one does nothing", () => {
  const { o, fila, linhas } = montar();
  assert.equal(o.orientar("liste os arquivos", null, "s"), "enfileirada");
  assert.deepEqual(fila, ["liste os arquivos"]);
  assert.equal(o.orientar("   ", null, "s"), "vazia");
  assert.equal(fila.length, 1);
  assert.deepEqual(linhas, []);
});

test("a query that rejects the input does not throw out of the runner; the turn end defers it", async () => {
  const { o, fila } = montar();
  const fechada = { streamInput: async () => { throw new Error("Query closed"); } };
  assert.equal(o.orientar("x", fechada, "s"), "injetada");
  await tick();
  o.encerrarTurno();
  assert.deepEqual(fila, ["x"]);
});

// ---- the wiring, by source (the runner cannot be imported here)
const fonte = readFileSync(new URL("./claude-runner.mjs", import.meta.url), "utf8");

test("the runner wires it: steer is read from stdin, every SDK message is observed, the turn end defers", () => {
  const leitor = fonte.slice(fonte.indexOf('rl.on("line"'));
  assert.match(leitor, /msg\.type === "steer"[\s\S]{0,200}orientacao\.orientar\(String\(msg\.text \?\? ""\), busy \? consultaAtual : null, sessionId\)/);
  assert.match(fonte, /consultaAtual = query\(\{ prompt: montarPrompt\(text, images\), options \}\);\s*for await \(const message of consultaAtual\) \{\s*orientacao\.observar\(message\);/);
  assert.match(fonte, /finally \{\s*consultaAtual = null;\s*orientacao\.encerrarTurno\(\);\s*busy = false;/);
});
