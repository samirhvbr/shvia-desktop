import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

/**
 * Steering a turn in flight (1.11.0), end to end: the REAL runner against a fake app-server that
 * records every `turn/start` and `turn/steer` it receives.
 *
 * The fake does not finish a turn on its own right away: it waits for a steer (or 1.5 s), so a
 * steer sent on stdin right after the message meets a turn that is still running. Both lines go
 * in at once, which is also the race the runner has to win: the steer reaches it before
 * `turn/start` has answered with the turn id.
 *
 * `SHVIA_TESTE_RECUSA=1` makes the fake refuse the steer the way Codex does when the expected turn
 * is no longer the active one.
 */
const FALSO = `
const fs = require("node:fs");
const rl = require("node:readline").createInterface({ input: process.stdin });
const out = (o) => process.stdout.write(JSON.stringify(o) + "\\n");
const reg = (o) => fs.appendFileSync(process.env.SHVIA_TESTE_REGISTRO, JSON.stringify(o) + "\\n");
let n = 0, aberto = null, timer = null;
const fechar = () => {
  if (!aberto) return;
  clearTimeout(timer);
  out({ method: "turn/completed", params: { threadId: "t1", turn: { id: aberto, status: "completed" } } });
  aberto = null;
};
rl.on("line", (l) => {
  const q = JSON.parse(l);
  if (q.method === "initialize") return out({ id: q.id, result: {} });
  if (q.method === "command/exec") {
    const dentro = JSON.stringify(q.params.command).includes("shvia-sandbox-controle");
    return out({ id: q.id, result: { exitCode: dentro ? 0 : 2, stdout: "", stderr: dentro ? "" : "Read-only file system" } });
  }
  if (q.method === "thread/start") return out({ id: q.id, result: { thread: { id: "t1" } } });
  if (q.method === "turn/start") {
    aberto = "turn-" + (++n);
    reg({ metodo: "turn/start", params: q.params, turno: aberto });
    out({ id: q.id, result: { turn: { id: aberto } } });
    timer = setTimeout(fechar, 1500);
    return;
  }
  if (q.method === "turn/steer") {
    reg({ metodo: "turn/steer", params: q.params, ativo: aberto });
    if (process.env.SHVIA_TESTE_RECUSA === "1") {
      out({ id: q.id, error: { code: -32600, message: "expected turn is not the active turn" } });
    } else {
      out({ id: q.id, result: { turnId: aberto } });
    }
    return fechar();
  }
  if (q.id !== undefined) out({ id: q.id, result: {} });
});`;

function rodar(linhas, env = {}) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-orientar-"));
  const registro = join(dir, "registro.ndjson");
  try {
    const fake = join(dir, "codex");
    writeFileSync(fake, `#!${process.execPath}\n${FALSO}\n`, { mode: 0o755 });
    const run = spawnSync(process.execPath, [new URL("./codex-runner.mjs", import.meta.url).pathname, "--cwd", dir, "--model", "gpt-teste"], {
      env: { ...process.env, SHVIA_CODEX_BIN: fake, SHVIA_TESTE_REGISTRO: registro, ...env },
      input: linhas.map((l) => JSON.stringify(l)).join("\n") + "\n",
      encoding: "utf8", timeout: 10000,
    });
    const pedidos = existsSync(registro) ? readFileSync(registro, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)) : [];
    const saida = run.stdout.trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
    return { run, pedidos, saida };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const do_tipo = (saida, t) => saida.filter((m) => m.type === t);

test("a steer sent while the turn runs goes INTO that turn, aimed at its id", () => {
  const { run, pedidos, saida } = rodar([
    { type: "user", text: "refatore o módulo de login" },
    { type: "steer", text: "não mexa nos testes" },
  ]);
  assert.equal(run.error, undefined, `the runner hung: ${run.stderr}`);
  const inicios = pedidos.filter((p) => p.metodo === "turn/start");
  const orientacoes = pedidos.filter((p) => p.metodo === "turn/steer");
  assert.equal(inicios.length, 1, `the steer must not open a second turn: ${JSON.stringify(pedidos)}`);
  assert.equal(orientacoes.length, 1, `the server never got the steer: ${run.stdout}`);
  assert.equal(orientacoes[0].params.expectedTurnId, inicios[0].turno, "the steer is aimed at the turn in flight");
  assert.equal(orientacoes[0].params.threadId, "t1");
  assert.deepEqual(orientacoes[0].params.input, [{ type: "text", text: "não mexa nos testes" }]);
  assert.deepEqual(do_tipo(saida, "steer_applied").map((m) => m.text), ["não mexa nos testes"]);
  assert.equal(do_tipo(saida, "steer_deferred").length, 0);
});

test("a steer the server refuses is not an error: it goes back to the host, and the runner does not run it", () => {
  const { run, pedidos, saida } = rodar([
    { type: "user", text: "refatore o módulo de login" },
    { type: "steer", text: "não mexa nos testes" },
  ], { SHVIA_TESTE_RECUSA: "1" });
  assert.equal(run.error, undefined, `the runner hung: ${run.stderr}`);
  assert.equal(do_tipo(saida, "error").length, 0,
    `a refused steer reached the page as an error, which the bridge reads as "turn over": ${run.stdout}`);
  assert.deepEqual(do_tipo(saida, "steer_deferred").map((m) => m.text), ["não mexa nos testes"]);
  const inicios = pedidos.filter((p) => p.metodo === "turn/start");
  assert.equal(inicios.length, 1,
    `the page owns the queue: a turn the runner started on its own would skip its bookkeeping: ${JSON.stringify(pedidos)}`);
});

test("a steer with no turn in flight is simply the next message", () => {
  const { run, pedidos, saida } = rodar([{ type: "steer", text: "liste os arquivos" }]);
  assert.equal(run.error, undefined, `the runner hung: ${run.stderr}`);
  assert.equal(pedidos.filter((p) => p.metodo === "turn/steer").length, 0, "there was no turn to steer");
  const inicios = pedidos.filter((p) => p.metodo === "turn/start");
  assert.equal(inicios.length, 1);
  assert.deepEqual(inicios[0].params.input, [{ type: "text", text: "liste os arquivos" }]);
  assert.equal(do_tipo(saida, "error").length, 0, run.stdout);
});

test("an empty steer does nothing", () => {
  const { run, pedidos } = rodar([{ type: "steer", text: "   " }]);
  assert.equal(run.error, undefined, run.stderr);
  assert.equal(pedidos.length, 0, JSON.stringify(pedidos));
});
