#!/usr/bin/env node
/**
 * The install's load proof does not wait on a terminal (1.6.25).
 *
 * `claude-runner/install.sh` ends by importing the installed runner to prove it loads. Importing
 * RUNS it, and the runner's top level opens `readline` on stdin and exits only on EOF. From the
 * app that never showed (`output()` gives a null stdin); from a terminal — where the runner's own
 * error message sends people — stdin never ends, and the finished install hung on its proof.
 *
 * This runs the REAL probe (the `if … fi` block cut out of install.sh) against a stand-in runner
 * that does exactly what the real one does with stdin, with a stdin that never closes, like a
 * terminal. A hang is a failure: the probe must return within 5 s.
 */
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const FONTE = readFileSync(join(RAIZ, "claude-runner/install.sh"), "utf8");
const i = FONTE.indexOf('if ! ERRO="$("$NODE_ABS" --input-type=module -e "import(');
if (i < 0) { console.error("🔴 the load proof moved out of claude-runner/install.sh."); process.exit(1); }
const SONDA = FONTE.slice(i, FONTE.indexOf("\nfi\n", i) + 4);

const dest = mkdtempSync(join(tmpdir(), "shvia-sonda-"));
writeFileSync(join(dest, "claude-runner.mjs"),
  'import * as readline from "node:readline";\n' +
  'readline.createInterface({ input: process.stdin }).on("close", () => process.exit(0));\n');

const filho = spawn("bash", ["-c", `set -euo pipefail\nNODE_ABS=${JSON.stringify(process.execPath)}\nDEST=${JSON.stringify(dest)}\n${SONDA}\necho sonda-ok`], {
  stdio: ["pipe", "pipe", "pipe"], // stdin stays OPEN, like a terminal
});
let saida = "";
filho.stdout.on("data", (d) => { saida += d; });
const venceu = setTimeout(() => {
  filho.kill("SIGKILL");
  rmSync(dest, { recursive: true, force: true });
  console.error("🔴 the install's load proof hung on an open stdin — run from a terminal, the install never ends");
  process.exit(1);
}, 5000);
filho.on("exit", (code) => {
  clearTimeout(venceu);
  rmSync(dest, { recursive: true, force: true });
  if (code !== 0 || !saida.includes("sonda-ok")) {
    console.error(`🔴 the load proof failed on a runner that loads (exit ${code})`);
    process.exit(1);
  }
  console.log("[sonda-do-instalador] a prova de carga volta com o stdin aberto, como num terminal");
});
