#!/usr/bin/env node
/**
 * A macOS build that carries the Pentest engine cannot finish unless the engine starts inside the built .app.
 *
 * On 28/10/2026 a Mac installed a Desktop whose About said the engine "did not report the version": the engine
 * was in the app and did not start. The guard that exists for exactly that (`verify_macos_engine` in
 * build-local.sh) only ran in signed builds and, with no standalone .app, said "try it in the installed app" and
 * let the build through. This runs the REAL function, cut out of build-local.sh, against fake engines in a fake
 * .app, and checks every way it can go: an engine that answers, one that prints junk, one that exits with an
 * error (and what it wrote), one the system kills, one that hangs past the deadline, a build with no .app, a
 * build with no engine, and a machine that is not a Mac. The fakes are shell scripts, so this runs on Linux CI.
 */
import { spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const FONTE = readFileSync(join(RAIZ, "build-local.sh"), "utf8");
const ini = FONTE.indexOf('MACOS_BUNDLE_DIR="${MACOS_BUNDLE_DIR:-');
const fn = FONTE.indexOf("verify_macos_engine() {");
if (ini < 0 || fn < 0) {
  console.error("🔴 verify_macos_engine (or MACOS_BUNDLE_DIR) moved out of build-local.sh.");
  process.exit(1);
}
const BLOCO = FONTE.slice(ini, FONTE.indexOf("\n}\n", fn) + 3);

const falhas = [];
const confere = (ok, texto) => { if (!ok) falhas.push(texto); };

function roda(nome, { engine, os = "macOS", stage = true, app = true, segundos = 20 }) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-trava-"));
  try {
    mkdirSync(join(dir, "src-tauri", "binaries"), { recursive: true });
    if (stage) writeFileSync(join(dir, "src-tauri", "binaries", "pentest-x86_64-apple-darwin"), "");
    const bundle = join(dir, "bundle", "macos");
    mkdirSync(bundle, { recursive: true });
    if (app) {
      const macos = join(bundle, "ShvIA.app", "Contents", "MacOS");
      mkdirSync(macos, { recursive: true });
      writeFileSync(join(macos, "pentest"), `#!/bin/sh\n${engine}\n`, { mode: 0o755 });
      chmodSync(join(macos, "pentest"), 0o755);
    }
    const script = [
      "set -euo pipefail",
      `_BUILD_OS=${os}`,
      BLOCO,
      "verify_macos_engine",
      'echo "RETURNED"',
    ].join("\n");
    const inicio = Date.now();
    const r = spawnSync("bash", ["-c", script], {
      cwd: dir, encoding: "utf8", timeout: 60_000,
      env: { ...process.env, MACOS_BUNDLE_DIR: bundle, SHVIA_ENGINE_CHECK_SECONDS: String(segundos) },
    });
    return { nome, status: r.status, out: r.stdout, err: r.stderr, ms: Date.now() - inicio };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

// 1. An engine that answers: the build goes on, and says how long it took.
let r = roda("answers", { engine: "printf '1.2.43\\n'" });
confere(r.status === 0 && r.out.includes("RETURNED"), `answers: the build must go on (status ${r.status})`);
confere(/starts inside the built \.app \(1\.2\.43, \d+s of 20s allowed\)/.test(r.out), `answers: says it started and for how long: ${r.out}`);

// 2. It exits cleanly but prints something that is not a version.
r = roda("junk", { engine: "printf 'strix 1.7.0\\n'" });
confere(r.status === 1 && !r.out.includes("RETURNED"), `junk: the build must stop (status ${r.status})`);
confere(r.err.includes("not a version: strix 1.7.0"), `junk: names what it printed: ${r.err}`);

// 3. It fails and says why on stderr: the exit status and what it wrote reach the person.
r = roda("fails", { engine: "echo \"[PYI-1:ERROR] Failed to load Python shared library 'x'\" >&2; exit 255" });
confere(r.status === 1, `fails: the build must stop (status ${r.status})`);
confere(r.err.includes("exited with status 255"), `fails: names the status: ${r.err}`);
confere(r.err.includes("Failed to load Python shared library"), `fails: carries what the engine wrote: ${r.err}`);
confere(r.err.includes("library validation"), `fails: still explains the usual cause: ${r.err}`);

// 4. The system ends it with a signal (how a refused signature looks from outside).
r = roda("killed", { engine: "kill -9 $$" });
confere(r.status === 1 && r.err.includes("signal 9"), `killed: names the signal (status ${r.status}): ${r.err}`);

// 5. It hangs: the build does not hang with it. One second allowed here instead of the app's 60 on a Mac.
r = roda("hangs", { engine: "sleep 30", segundos: 1 });
confere(r.status === 1 && r.err.includes("did not answer in 1 s"), `hangs: gives up at the deadline (status ${r.status}): ${r.err}`);
confere(r.ms < 15_000, `hangs: must not wait out the engine (${r.ms} ms)`);

// 6. No standalone .app (a .dmg-only build): the engine cannot be shown to start, so the build stops.
r = roda("no-app", { engine: "printf '1.2.43\\n'", app: false });
confere(r.status === 1 && r.err.includes("no standalone .app"), `no-app: must stop with the reason (status ${r.status}): ${r.err}`);

// 7. A build that carries no engine has nothing to check, and goes on silently.
r = roda("no-engine", { engine: "exit 1", stage: false, app: false });
confere(r.status === 0 && r.out.trim() === "RETURNED" && r.err === "", `no-engine: must go on without a word (status ${r.status}): ${r.out}${r.err}`);

// 8. Not a Mac: nothing to do.
r = roda("linux", { engine: "exit 1", os: "Linux" });
confere(r.status === 0 && r.out.trim() === "RETURNED", `linux: must go on (status ${r.status}): ${r.out}${r.err}`);

// 9. The check is not tied to signing any more: the function no longer looks at SIGN_ENABLED at all.
confere(!/SIGN_ENABLED/.test(BLOCO), "the guard must not depend on whether the build is signed");

if (falhas.length) {
  console.error(`🔴 ${falhas.length} failure(s):\n - ${falhas.join("\n - ")}`);
  process.exit(1);
}
console.log("✔ verify_macos_engine: answers, junk, error + stderr, signal, hang, no .app, no engine, not a Mac, signing-independent");
