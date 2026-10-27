import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

/**
 * The app-server dying on its own must end the runner (1.6.17).
 *
 * Until then nothing listened for the child's exit: a pending request waited for an answer that
 * could not come, and the page sat on "working" forever. Each case runs with a 5 s timeout, so a
 * hang is a FAILURE here, not a skip. The fake app-server follows `catalogue.test.mjs`.
 */
function rodar(corpoDoFalso, args, input) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-ciclo-"));
  try {
    const fake = join(dir, "codex");
    writeFileSync(fake, `#!${process.execPath}\n${corpoDoFalso}\n`, { mode: 0o755 });
    return spawnSync(process.execPath, [new URL("./codex-runner.mjs", import.meta.url).pathname, ...args, "--cwd", dir], {
      env: { ...process.env, SHVIA_CODEX_BIN: fake }, input, encoding: "utf8", timeout: 5000,
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function erros(run) {
  return run.stdout.trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)).filter((m) => m.type === "error");
}

test("an app-server that dies before answering `initialize` ends the runner with an error", () => {
  const run = rodar("process.exit(3);", []);
  assert.equal(run.error, undefined, "the runner hung waiting for an answer from a dead process");
  assert.equal(run.status, 1);
  assert.ok(erros(run).some((e) => /encerrou/.test(e.message)), run.stdout);
});

// In `--modelos` the failure is the `erro` line — the only error line the desktop reads in that mode
// (1.16.2); a `type: "error"` there was dropped and read as "update the runner".
test("an app-server that dies mid-request, after the handshake, ends the runner too", () => {
  const run = rodar(`
const rl = require("node:readline").createInterface({ input: process.stdin });
rl.on("line", (l) => {
  const q = JSON.parse(l);
  if (q.method === "initialize") console.log(JSON.stringify({ id: q.id, result: {} }));
  else process.exit(3);
});`, ["--modelos"]);
  assert.equal(run.error, undefined, "the runner hung on the request the dead process never answered");
  assert.equal(run.status, 1);
  const linhas = run.stdout.trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
  assert.ok(linhas.some((l) => /encerrou/.test(l.erro ?? "")), run.stdout);
  assert.equal(erros(run).length, 0, "a `type: error` line is not one the desktop reads in --modelos mode");
});

test("a rejected turn/start releases the queue without waiting for a turn that never started", () => {
  const run = rodar(`
const rl = require("node:readline").createInterface({ input: process.stdin });
const out = (o) => console.log(JSON.stringify(o));
let turns = 0;
rl.on("line", (l) => {
  const q = JSON.parse(l);
  if (q.method === "initialize") return out({ id: q.id, result: {} });
  if (q.method === "command/exec") {
    const control = JSON.stringify(q.params.command).includes("shvia-sandbox-controle");
    return out({ id: q.id, result: { exitCode: control ? 0 : 2, stdout: "", stderr: "" } });
  }
  if (q.method === "thread/start") return out({ id: q.id, result: { thread: { id: "t1" } } });
  if (q.method === "turn/start") {
    if (++turns === 1) return out({ id: q.id, error: { code: -32600, message: "turn/start refused" } });
    out({ id: q.id, result: { turn: { id: "u2" } } });
    setImmediate(() => {
      out({ method: "item/agentMessage/delta", params: { delta: q.params.input[0].text } });
      out({ method: "turn/completed", params: { threadId: "t1", turn: { id: "u2", status: "completed" } } });
    });
  }
});`, [], '{"type":"user","text":"first"}\n{"type":"user","text":"second"}\n');
  assert.equal(run.error, undefined, `the runner hung after turn/start was refused: ${run.stdout}`);
  assert.equal(run.status, 0, run.stderr);
  const events = run.stdout.trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
  assert.deepEqual(erros(run).map((e) => e.message), ["turn/start refused"]);
  assert.equal(events.filter((e) => e.type === "turn_done").length, 2, "both host messages must finish");
  assert.deepEqual(events.filter((e) => e.type === "text").map((e) => e.delta), ["second"],
    "the rejected message must not run, and the queued message must still reach the server");
  assert.ok(events.findIndex((e) => e.type === "turn_done") < events.findIndex((e) => e.type === "text"),
    "the rejected turn must finish before the queued turn runs");
});
