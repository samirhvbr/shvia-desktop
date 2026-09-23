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
function rodar(corpoDoFalso, args) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-ciclo-"));
  try {
    const fake = join(dir, "codex");
    writeFileSync(fake, `#!${process.execPath}\n${corpoDoFalso}\n`, { mode: 0o755 });
    return spawnSync(process.execPath, [new URL("./codex-runner.mjs", import.meta.url).pathname, ...args, "--cwd", dir], {
      env: { ...process.env, SHVIA_CODEX_BIN: fake }, encoding: "utf8", timeout: 5000,
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
  assert.ok(erros(run).some((e) => /encerrou/.test(e.message)), run.stdout);
});
