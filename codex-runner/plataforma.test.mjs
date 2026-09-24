// plataforma.test.mjs — the Windows branches of the Codex runner, run on any OS (1.6.58).
import { test } from "node:test";
import assert from "node:assert/strict";
import path from "node:path";
import { comandosDaProva, resolverCodex } from "./plataforma.mjs";

const NPM = "C:\\Users\\joão\\AppData\\Roaming\\npm";
const JS = path.win32.join(NPM, "node_modules", "@openai", "codex", "bin", "codex.js");
const NODE = "C:\\Program Files\\nodejs\\node.exe";

test("outside Windows, codex is `codex`, as it always was", () => {
  assert.deepEqual(resolverCodex({ platform: "linux", env: { PATH: "/usr/bin" }, existe: () => true }), { comando: "codex", prefixo: [] });
  assert.deepEqual(resolverCodex({ platform: "darwin", env: {}, existe: () => false }), { comando: "codex", prefixo: [] });
});

test("SHVIA_CODEX_BIN overrides every OS", () => {
  const env = { SHVIA_CODEX_BIN: "C:\\tools\\codex.exe", Path: NPM };
  assert.deepEqual(resolverCodex({ platform: "win32", env, existe: () => true }), { comando: "C:\\tools\\codex.exe", prefixo: [] });
});

test("🔴 on Windows the npm shim becomes node with codex.js — never the .cmd", () => {
  const tem = new Set([path.win32.join(NPM, "codex.cmd"), JS]);
  const r = resolverCodex({ platform: "win32", env: { Path: `C:\\Windows\\system32;${NPM}` }, existe: (p) => tem.has(p), node: NODE });
  assert.deepEqual(r, { comando: NODE, prefixo: [JS] });
  assert.ok(!r.comando.endsWith(".cmd"));
});

test("on Windows a native codex.exe on PATH wins over the shim", () => {
  const exe = "C:\\tools\\codex\\codex.exe";
  const tem = new Set([exe, path.win32.join(NPM, "codex.cmd"), JS]);
  const r = resolverCodex({ platform: "win32", env: { Path: `C:\\tools\\codex;${NPM}` }, existe: (p) => tem.has(p), node: NODE });
  assert.deepEqual(r, { comando: exe, prefixo: [] });
});

test("on Windows a shim without its codex.js is not trusted, and nothing found stays `codex`", () => {
  const tem = new Set([path.win32.join(NPM, "codex.cmd")]);
  assert.deepEqual(resolverCodex({ platform: "win32", env: { Path: NPM }, existe: (p) => tem.has(p), node: NODE }), { comando: "codex", prefixo: [] });
  assert.deepEqual(resolverCodex({ platform: "win32", env: {}, existe: () => false }), { comando: "codex", prefixo: [] });
});

test("🔴 the proof has a control INSIDE the project and a probe OUTSIDE it, on both OS families", () => {
  const posix = comandosDaProva({ platform: "linux", fora: "/home/u/.x", dentro: "/p/.c" });
  assert.deepEqual(posix.controle, ["/bin/sh", "-c", "printf x > '/p/.c' && rm -f '/p/.c'"]);
  assert.deepEqual(posix.prova, ["/bin/sh", "-c", "printf x > '/home/u/.x' && rm -f '/home/u/.x'"]);
  const win = comandosDaProva({ platform: "win32", fora: "C:\\Users\\u\\.x", dentro: "C:\\p\\.c" });
  // No /bin/sh on Windows: the old probe could not run there, and its exit read as "held".
  assert.equal(win.controle[0], "cmd.exe");
  assert.ok(!JSON.stringify(win).includes("/bin/sh"));
  assert.equal(win.controle[3], 'echo x>"C:\\p\\.c" && del /q "C:\\p\\.c"');
  assert.equal(win.prova[3], 'echo x>"C:\\Users\\u\\.x" && del /q "C:\\Users\\u\\.x"');
});
