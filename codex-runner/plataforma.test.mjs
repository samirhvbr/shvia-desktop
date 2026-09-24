// plataforma.test.mjs — the Windows branches of the Codex runner, run on any OS (1.6.58).
import { test } from "node:test";
import assert from "node:assert/strict";
import { comandosDaProva } from "./plataforma.mjs";

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
