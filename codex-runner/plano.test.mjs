import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

/**
 * PLAN turns (1.9.0), end to end: the REAL runner against a fake app-server that records every
 * `turn/start` it receives. Three turns go in on stdin — plan, normal, normal — and the record
 * is what the server was actually asked, not what a helper returns.
 *
 * The fake follows `ciclo.test.mjs`, plus what a turn needs: the two commands of the startup
 * sandbox proof (the control inside the project exits 0, the write outside is refused), a
 * `thread/start` that reports its own sandbox the way Codex does, and a `turn/completed` after
 * each `turn/start`.
 */
const FALSO = `
const fs = require("node:fs");
const rl = require("node:readline").createInterface({ input: process.stdin });
const out = (o) => process.stdout.write(JSON.stringify(o) + "\\n");
rl.on("line", (l) => {
  const q = JSON.parse(l);
  if (q.method === "initialize") return out({ id: q.id, result: {} });
  if (q.method === "command/exec") {
    const dentro = JSON.stringify(q.params.command).includes("shvia-sandbox-controle");
    return out({ id: q.id, result: { exitCode: dentro ? 0 : 2, stdout: "", stderr: dentro ? "" : "Read-only file system" } });
  }
  if (q.method === "thread/start") {
    return out({ id: q.id, result: { thread: { id: "t1" }, sandbox: { type: "workspaceWrite", networkAccess: true, writableRoots: ["/srv/extra"] } } });
  }
  if (q.method === "turn/start") {
    fs.appendFileSync(process.env.SHVIA_TESTE_REGISTRO, JSON.stringify(q.params) + "\\n");
    out({ id: q.id, result: { turn: { id: "u" + Date.now() } } });
    return out({ method: "turn/completed", params: { threadId: "t1", turn: { id: "u", status: "completed" } } });
  }
  if (q.id !== undefined) out({ id: q.id, result: {} });
});`;

function rodar(linhas) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-plano-"));
  const registro = join(dir, "turnos.ndjson");
  try {
    const fake = join(dir, "codex");
    writeFileSync(fake, `#!${process.execPath}\n${FALSO}\n`, { mode: 0o755 });
    const run = spawnSync(process.execPath, [new URL("./codex-runner.mjs", import.meta.url).pathname, "--cwd", dir, "--model", "gpt-teste"], {
      env: { ...process.env, SHVIA_CODEX_BIN: fake, SHVIA_TESTE_REGISTRO: registro },
      input: linhas.map((l) => JSON.stringify(l)).join("\n") + "\n",
      encoding: "utf8", timeout: 8000,
    });
    const turnos = existsSync(registro) ? readFileSync(registro, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)) : [];
    return { run, turnos };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("plan → normal → normal: read-only, then the thread's own policy back, then no override", () => {
  const { run, turnos } = rodar([
    { type: "user", text: "planeje a migração", plano: true },
    { type: "user", text: "execute o plano" },
    { type: "user", text: "e rode os testes" },
  ]);
  assert.equal(run.error, undefined, `the runner hung: ${run.stderr}`);
  assert.equal(turnos.length, 3, `three turns were asked of the server; stdout: ${run.stdout} stderr: ${run.stderr}`);
  assert.deepEqual(turnos[0].sandboxPolicy, { type: "readOnly" }, "the plan turn runs read-only");
  assert.deepEqual(turnos[1].sandboxPolicy, { type: "workspaceWrite", networkAccess: true, writableRoots: ["/srv/extra"] },
    "the next turn puts back exactly what thread/start reported");
  assert.equal("sandboxPolicy" in turnos[2], false, "a normal turn after a normal one sends no override");
  for (const t of turnos) assert.equal("approvalPolicy" in t, false, "the approval policy is never touched per turn");
});

test("a turn without `plano` sends exactly what it sent before 1.9.0", () => {
  const { run, turnos } = rodar([{ type: "user", text: "oi" }, { type: "user", text: "de novo", plano: "true" }]);
  assert.equal(run.error, undefined, run.stderr);
  assert.equal(turnos.length, 2, run.stdout + run.stderr);
  for (const t of turnos) assert.equal("sandboxPolicy" in t, false, `no override: ${JSON.stringify(t)}`);
});
