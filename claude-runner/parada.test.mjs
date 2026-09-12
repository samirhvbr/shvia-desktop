import { test } from "node:test";
import assert from "node:assert/strict";
import {
  LAST_MAX,
  MENSAGEM_PADRAO,
  encerrarPendentes,
  esperarDecisao,
  montarStopRequest,
  normalizarDecisao,
  opcoesDaRun,
  saidaDoHook,
} from "./parada.mjs";

/**
 * The stop handshake of the Claude runner — block B2 of docs/code/RUN-20260910.md.
 *
 * Run: `node --test claude-runner/` (CI runs it through `npm run prova:politica`).
 *
 * What is proved here is the SHAPE of the event and the two defaults that err toward the
 * alarm: no answer → stop, stdin closed → stop. The runner never decides; this file proves
 * it cannot, because every path that is not an explicit `continue` ends the turn.
 */

test("stop_request carries the four fields and cuts `last` at LAST_MAX, keeping the TAIL", () => {
  // The contract's markers and the rule's closing zone live at the END of the message:
  // a cut that kept the head threw away exactly what the orchestrator reads.
  const longo = "x".repeat(LAST_MAX + 500) + "\n\nRUN_DONE: entregue";
  const ev = montarStopRequest({ id: "s-7", iteration: 7, last: longo, reason: "model_stopped" });
  assert.equal(ev.type, "stop_request");
  assert.equal(ev.id, "s-7");
  assert.equal(ev.iteration, 7);
  assert.equal(ev.last.length, LAST_MAX);
  assert.ok(ev.last.endsWith("RUN_DONE: entregue"), "the tail survives the cut");
  assert.equal(ev.reason, "model_stopped");
});

test("stop_request defaults: no text, no iteration, no reason", () => {
  const ev = montarStopRequest({ id: 3, iteration: NaN, last: undefined, reason: "" });
  assert.equal(ev.id, "3");
  assert.equal(ev.iteration, 0);
  assert.equal(ev.last, "");
  assert.equal(ev.reason, "model_stopped");
});

test("only an explicit `continue` continues; everything else is stop", () => {
  assert.deepEqual(normalizarDecisao({ decision: "continue", message: " siga " }), { decision: "continue", message: "siga" });
  assert.deepEqual(normalizarDecisao({ decision: "continue" }), { decision: "continue", message: MENSAGEM_PADRAO });
  assert.deepEqual(normalizarDecisao({ decision: "continue", message: "   " }), { decision: "continue", message: MENSAGEM_PADRAO });
  assert.deepEqual(normalizarDecisao({ decision: "stop" }), { decision: "stop" });
  // A gate word is not a stop decision — the two handshakes share the `id`+`decision`
  // shape, and a host that answered the wrong one must not keep the turn alive.
  assert.deepEqual(normalizarDecisao({ decision: "approve" }), { decision: "stop" });
  assert.deepEqual(normalizarDecisao(null), { decision: "stop" });
  assert.deepEqual(normalizarDecisao("continue"), { decision: "stop" });
});

test("the hook output: continue blocks the stop with the message as reason; stop is empty", () => {
  assert.deepEqual(saidaDoHook({ decision: "continue", message: "vai" }), { decision: "block", reason: "vai" });
  assert.deepEqual(saidaDoHook({ decision: "stop" }), {});
  assert.deepEqual(saidaDoHook(undefined), {});
  // `{}` and not `{decision:'approve'}`: the SDK's Stop hook only knows `block`; any
  // other decision key is noise the runner would be inventing.
  assert.equal("decision" in saidaDoHook({ decision: "stop" }), false);
});

test("the host's answer resolves the wait and clears the map", async () => {
  const pendentes = new Map();
  const cancelados = [];
  const p = esperarDecisao(pendentes, "s-1", { timer: () => 42, cancelar: (t) => cancelados.push(t) });
  assert.equal(pendentes.has("s-1"), true);
  pendentes.get("s-1")({ decision: "continue", message: "segue" });
  assert.deepEqual(await p, { decision: "continue", message: "segue" });
  assert.equal(pendentes.has("s-1"), false);
  assert.deepEqual(cancelados, [42]);
});

test("no answer within the ceiling → stop, and the request leaves the map", async () => {
  const pendentes = new Map();
  // A timer that fires at once stands in for the 60 s.
  const p = esperarDecisao(pendentes, "s-2", { timer: (fn) => { fn(); return 1; }, cancelar: () => {} });
  assert.deepEqual(await p, { decision: "stop", motivo: "teto" });
  assert.equal(pendentes.size, 0);
});

test("an answer after the ceiling does not resurrect the request", async () => {
  const pendentes = new Map();
  let disparo;
  const p = esperarDecisao(pendentes, "s-3", { timer: (fn) => { disparo = fn; return 1; }, cancelar: () => {} });
  disparo();
  assert.deepEqual(await p, { decision: "stop", motivo: "teto" });
  // The stdin reader looks the id up before calling; a gone id is a "decision for an
  // unknown request", not a second resolution.
  assert.equal(pendentes.get("s-3"), undefined);
});

test("stdin closed: every pending request is answered stop", async () => {
  const pendentes = new Map();
  const a = esperarDecisao(pendentes, "s-4", { timer: () => 1, cancelar: () => {} });
  const b = esperarDecisao(pendentes, "s-5", { timer: () => 1, cancelar: () => {} });
  encerrarPendentes(pendentes);
  assert.deepEqual(await a, { decision: "stop" });
  assert.deepEqual(await b, { decision: "stop" });
  assert.equal(pendentes.size, 0);
});

const flags = (lista) => (flag) => {
  const i = lista.indexOf(flag);
  return i >= 0 && i + 1 < lista.length ? lista[i + 1] : undefined;
};

test("without flags the runner behaves as before 1.5.0: no hook, no caps", () => {
  assert.deepEqual(opcoesDaRun(flags([])), { parada: false, maxTurns: undefined, maxBudgetUsd: undefined });
});

test("the armed run: --parada host and the two caps", () => {
  const o = opcoesDaRun(flags(["--parada", "host", "--teto-iteracoes", "100", "--teto-custo", "5"]));
  assert.deepEqual(o, { parada: true, maxTurns: 100, maxBudgetUsd: 5 });
});

test("an invalid cap is dropped, never coerced", () => {
  // 0 would end every turn before it started; a word is not a number; a negative cost
  // is not a budget. `--parada` with any other value keeps the hook off.
  const o = opcoesDaRun(flags(["--parada", "motor", "--teto-iteracoes", "0", "--teto-custo", "-1"]));
  assert.deepEqual(o, { parada: false, maxTurns: undefined, maxBudgetUsd: undefined });
  assert.equal(opcoesDaRun(flags(["--teto-iteracoes", "cem"])).maxTurns, undefined);
  assert.equal(opcoesDaRun(flags(["--teto-iteracoes", "2.9"])).maxTurns, 2);
  assert.equal(opcoesDaRun(flags(["--teto-custo", "0.5"])).maxBudgetUsd, 0.5);
});
