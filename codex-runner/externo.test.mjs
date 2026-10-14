import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

/**
 * The EXTERNAL sandbox mode (1.28.0), end to end: the REAL runner against a fake app-server that records
 * what it is asked. A host whose container is the boundary (the Workspace's runner) declares it with
 * `SHVIA_CODEX_SANDBOX_EXTERNO=<a directory it guarantees read-only>`; the runner must then
 *
 *   - start the thread with `danger-full-access` and send every turn under `externalSandbox`;
 *   - aim the startup proof at the declared directory, under the same policy the turns use;
 *   - stop before the first turn when that directory accepts a write (the host lied);
 *   - refuse a plan turn, and say so;
 *   - and change NOTHING when the variable is absent.
 *
 * The fake follows `plano.test.mjs`: the control command (a write inside the project) exits 0, the write
 * outside is refused with a read-only file system error unless `SHVIA_TESTE_ACEITA_FORA=1`, in which case
 * the fake plays a container that does not confine. It writes one JSON line per request to a registry.
 */
const FALSO = `
const fs = require("node:fs");
const rl = require("node:readline").createInterface({ input: process.stdin });
const out = (o) => process.stdout.write(JSON.stringify(o) + "\\n");
const registrar = (q) => fs.appendFileSync(process.env.SHVIA_TESTE_REGISTRO, JSON.stringify({ method: q.method, params: q.params }) + "\\n");
rl.on("line", (l) => {
  const q = JSON.parse(l);
  if (q.method === "initialize") return out({ id: q.id, result: {} });
  if (q.method === "command/exec") {
    registrar(q);
    const dentro = JSON.stringify(q.params.command).includes("shvia-sandbox-controle");
    const aceita = process.env.SHVIA_TESTE_ACEITA_FORA === "1";
    const ok = dentro || aceita;
    return out({ id: q.id, result: { exitCode: ok ? 0 : 2, stdout: "", stderr: ok ? "" : "Read-only file system" } });
  }
  if (q.method === "thread/start") {
    registrar(q);
    return out({ id: q.id, result: { thread: { id: "t1" }, sandbox: { type: "dangerFullAccess" } } });
  }
  if (q.method === "turn/start") {
    registrar(q);
    out({ id: q.id, result: { turn: { id: "u" + Date.now() } } });
    return out({ method: "turn/completed", params: { threadId: "t1", turn: { id: "u", status: "completed" } } });
  }
  if (q.id !== undefined) out({ id: q.id, result: {} });
});`;

function rodar({ env = {}, linhas = [], args = [] } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-externo-"));
  const registro = join(dir, "registro.ndjson");
  try {
    const fake = join(dir, "codex");
    writeFileSync(fake, `#!${process.execPath}\n${FALSO}\n`, { mode: 0o755 });
    const run = spawnSync(process.execPath, [new URL("./codex-runner.mjs", import.meta.url).pathname, "--cwd", dir, "--model", "gpt-teste", ...args], {
      env: { ...process.env, SHVIA_CODEX_BIN: fake, SHVIA_TESTE_REGISTRO: registro, SHVIA_CODEX_SANDBOX_EXTERNO: "", ...env },
      input: linhas.map((l) => JSON.stringify(l)).join("\n") + (linhas.length ? "\n" : ""),
      encoding: "utf8", timeout: 8000,
    });
    const pedidos = existsSync(registro) ? readFileSync(registro, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)) : [];
    const eventos = run.stdout.trim().split("\n").filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return { naoJson: l }; } });
    return { run, pedidos, eventos, dir };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const EXTERNA = { type: "externalSandbox", networkAccess: "restricted" };

test("external mode: danger-full-access thread, externalSandbox turns, and the proof aimed at the declared directory", () => {
  const { run, pedidos, eventos } = rodar({
    env: { SHVIA_CODEX_SANDBOX_EXTERNO: "/opt/shvia/motores" },
    linhas: [{ type: "user", text: "oi" }, { type: "user", text: "de novo" }],
  });
  assert.equal(run.error, undefined, `the runner hung: ${run.stderr}`);
  assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);

  const thread = pedidos.filter((p) => p.method === "thread/start");
  assert.equal(thread.length, 1);
  assert.equal(thread[0].params.sandbox, "danger-full-access", "the thread runs without Codex's sandbox: the container is the boundary");
  assert.equal(thread[0].params.approvalPolicy, "on-request", "`never` still never reaches the wire");

  const provas = pedidos.filter((p) => p.method === "command/exec");
  assert.equal(provas.length, 2, "the control and the proof, both asked of the server");
  for (const p of provas) assert.deepEqual(p.params.sandboxPolicy, EXTERNA, "the proof runs under the policy the turns use");
  const alvo = provas.map((p) => JSON.stringify(p.params.command)).find((c) => !c.includes("shvia-sandbox-controle"));
  assert.match(alvo, /\/opt\/shvia\/motores\/\.shvia-sandbox-probe-\d+/, "the write that must be refused is INSIDE the declared directory, not in $HOME");

  const turnos = pedidos.filter((p) => p.method === "turn/start");
  assert.equal(turnos.length, 2);
  for (const t of turnos) assert.deepEqual(t.params.sandboxPolicy, EXTERNA, "every turn carries the external policy");
  assert.equal(eventos.filter((e) => e.type === "turn_done").length, 2);
});

test("external mode: a directory that accepts the write stops the engine before any turn", () => {
  const { run, pedidos, eventos } = rodar({
    env: { SHVIA_CODEX_SANDBOX_EXTERNO: "/opt/shvia/motores", SHVIA_TESTE_ACEITA_FORA: "1" },
    linhas: [{ type: "user", text: "oi" }],
  });
  assert.equal(run.error, undefined, `the runner hung: ${run.stderr}`);
  assert.equal(run.status, 3, "the exit code of a sandbox that was not proven");
  const erro = eventos.find((e) => e.type === "error");
  assert.ok(erro && /\/opt\/shvia\/motores/.test(erro.message) && /aceitou uma escrita/.test(erro.message), run.stdout);
  assert.equal(pedidos.filter((p) => p.method === "turn/start").length, 0, "no turn ran on an unproven boundary");
});

test("external mode: a plan turn is refused, said so, and the next turn still runs", () => {
  const { run, pedidos, eventos } = rodar({
    env: { SHVIA_CODEX_SANDBOX_EXTERNO: "/opt/shvia/motores" },
    linhas: [{ type: "user", text: "planeje", plano: true }, { type: "user", text: "execute" }],
  });
  assert.equal(run.error, undefined, `the runner hung: ${run.stderr}`);
  assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
  const turnos = pedidos.filter((p) => p.method === "turn/start");
  assert.equal(turnos.length, 1, "only the normal turn reached the server: a plan turn is read-only by a sandbox this mode does not run");
  assert.ok(eventos.some((e) => e.type === "error" && /Plano/.test(e.message)), run.stdout);
  assert.equal(eventos.filter((e) => e.type === "turn_done").length, 2, "the refused turn is closed too, or the screen waits for it forever");
});

test("external mode: a declaration that is not an absolute path stops the engine with a message", () => {
  for (const ruim of ["relativo/pasta", "/a/../b"]) {
    const { run, pedidos, eventos } = rodar({ env: { SHVIA_CODEX_SANDBOX_EXTERNO: ruim }, linhas: [{ type: "user", text: "oi" }] });
    assert.equal(run.status, 3, `${ruim}: ${run.stdout}`);
    assert.ok(eventos.some((e) => e.type === "error" && /SHVIA_CODEX_SANDBOX_EXTERNO/.test(e.message)), run.stdout);
    assert.equal(pedidos.filter((p) => p.method === "thread/start" || p.method === "turn/start").length, 0, ruim);
  }
});

test("a bad declaration does not break the model picker (a control request never reaches the proof)", () => {
  const { run } = rodar({ env: { SHVIA_CODEX_SANDBOX_EXTERNO: "relativo" }, args: ["--modelos"] });
  // The fake answers `model/list` with `{}`, which the runner calls an invalid answer: what matters is that it
  // got as far as asking, instead of dying on the declaration.
  assert.doesNotMatch(run.stdout, /SHVIA_CODEX_SANDBOX_EXTERNO/, run.stdout);
});

test("without the variable nothing changes: Codex's own sandbox policy and the proof in $HOME", () => {
  const { run, pedidos } = rodar({ linhas: [{ type: "user", text: "oi" }] });
  assert.equal(run.error, undefined, run.stderr);
  assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
  const thread = pedidos.find((p) => p.method === "thread/start");
  assert.equal(thread.params.sandbox, "workspace-write");
  const provas = pedidos.filter((p) => p.method === "command/exec");
  for (const p of provas) assert.equal(p.params.sandboxPolicy.type, "workspaceWrite");
  const alvo = provas.map((p) => JSON.stringify(p.params.command)).find((c) => !c.includes("shvia-sandbox-controle"));
  assert.match(alvo, /\.shvia-sandbox-probe-\d+/);
  assert.doesNotMatch(alvo, /\/opt\/shvia/);
  const turno = pedidos.find((p) => p.method === "turn/start");
  assert.equal("sandboxPolicy" in turno.params, false, "no per-turn override, as before");
});
