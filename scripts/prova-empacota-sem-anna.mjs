#!/usr/bin/env node
/**
 * "The app ships without the engine" is true when it says so (1.6.24).
 *
 * `externalBin` makes Tauri REQUIRE `src-tauri/binaries/anna-<triple>`. Measured on 23/09:
 * without it the build script stops ("resource path … doesn't exist"). So `--no-anna`, which
 * deletes the folder, failed the build; and when no anna was found, stage-anna printed "o app sai
 * SEM o motor" while a stale anna from an earlier build — gitignored, persistent — was bundled,
 * never checked against ANNA_MINIMO.
 *
 * Three checks:
 *  1. stage-anna, run from a COPY in a temp tree (never the repo's own binaries/): no anna →
 *     the stale file is removed; a (fake) anna → it is staged.
 *  2. the real bundle-override block of build-local.sh composes one valid --config: no anna →
 *     `externalBin: []`; no updater key → `createUpdaterArtifacts: false`; both merged.
 *  3. (CI step, not here) `cargo check` with no binaries and `TAURI_CONFIG` externalBin=[]
 *     proves Tauri accepts the override.
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const triple = /host: (\S+)/.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))[1];
const sufixo = process.platform === "win32" ? ".exe" : "";
let falhas = 0;
const falha = (m) => { falhas++; console.error(`🔴 ${m}`); };

function arvore() {
  const dir = mkdtempSync(join(tmpdir(), "shvia-anna-"));
  mkdirSync(join(dir, "scripts"));
  mkdirSync(join(dir, "src-tauri/binaries"), { recursive: true });
  copyFileSync(join(RAIZ, "scripts/stage-anna.mjs"), join(dir, "scripts/stage-anna.mjs"));
  return dir;
}

// 1a. no anna: the stale one goes
{
  const dir = arvore();
  const velho = join(dir, `src-tauri/binaries/anna-${triple}${sufixo}`);
  writeFileSync(velho, "anna de um build anterior");
  try {
    execFileSync(process.execPath, [join(dir, "scripts/stage-anna.mjs"), "--from", join(dir, "nao-existe")], { stdio: "ignore" });
    if (existsSync(velho)) falha("no anna found, and the stale anna from an earlier build is still there to be bundled");
  } catch (e) { falha(`stage-anna without anna must not fail the build (exit ${e.status})`); }
  rmSync(dir, { recursive: true, force: true });
}
// 1b. a (fake) anna at or above the floor: it is staged
if (process.platform !== "win32") {
  const dir = arvore();
  const falso = join(dir, "anna");
  writeFileSync(falso, "#!/bin/sh\necho 'anna 99.0.0'\n", { mode: 0o755 });
  try {
    execFileSync(process.execPath, [join(dir, "scripts/stage-anna.mjs"), "--from", falso], { stdio: "ignore" });
    if (!existsSync(join(dir, `src-tauri/binaries/anna-${triple}`))) falha("a found anna was not staged");
  } catch (e) { falha(`staging a valid anna failed (exit ${e.status})`); }
  rmSync(dir, { recursive: true, force: true });
}

// 2. the real override block
const FONTE = readFileSync(join(RAIZ, "build-local.sh"), "utf8");
const a = FONTE.indexOf("  # ── bundle overrides (1.6.24) ──");
const b = FONTE.indexOf("  # ── end bundle overrides ──");
if (a < 0 || b < 0) { falha("the bundle-override block moved out of build-local.sh"); }
else {
  const BLOCO = FONTE.slice(a, b);
  for (const [comAnna, updater, esperado] of [
    [true, 1, ""],
    [false, 1, { bundle: { externalBin: [] } }],
    [true, 0, { bundle: { createUpdaterArtifacts: false } }],
    [false, 0, { bundle: { createUpdaterArtifacts: false, externalBin: [] } }],
  ]) {
    const dir = mkdtempSync(join(tmpdir(), "shvia-cfg-"));
    mkdirSync(join(dir, "src-tauri/binaries"), { recursive: true });
    if (comAnna) writeFileSync(join(dir, `src-tauri/binaries/anna-${triple}`), "x");
    const out = execFileSync("bash", ["-c", `set -euo pipefail\nUPDATER_ARTIFACTS=${updater}\n${BLOCO}\nprintf '%s' "$_CFG"`], { cwd: dir, encoding: "utf8" });
    rmSync(dir, { recursive: true, force: true });
    // The block also logs a line for the build output; the config is what printf wrote last.
    const cfg = out.slice(out.lastIndexOf("\n") + 1);
    const obtido = cfg === "" ? "" : JSON.parse(cfg);
    if (JSON.stringify(obtido) !== JSON.stringify(esperado)) {
      falha(`anna=${comAnna} updater=${updater}: expected ${JSON.stringify(esperado)}, got ${JSON.stringify(obtido)}`);
    }
  }
}

if (falhas) { console.error(`\n${falhas} problem(s): the bundle would carry a stale engine, or the build would fail without one.`); process.exit(1); }
console.log("[empacota-sem-anna] sem anna o velho sai e o Tauri é avisado; com anna ele entra — 6 casos");
