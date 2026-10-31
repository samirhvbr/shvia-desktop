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
let casosVerificados = 0;
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

// 1c. the floor (1.35.2): the last anna below it is refused and nothing is staged; the floor
// itself is staged. Run against the 0.11.9 floor, the first case fails: 0.13.5 was staged.
if (process.platform !== "win32") {
  for (const [versao, aceita] of [["0.13.5", false], ["0.13.6", true]]) {
    const dir = arvore();
    const falso = join(dir, "anna");
    writeFileSync(falso, `#!/bin/sh\necho 'anna ${versao}'\n`, { mode: 0o755 });
    let saida = 0;
    try {
      execFileSync(process.execPath, [join(dir, "scripts/stage-anna.mjs"), "--from", falso], { stdio: "ignore" });
    } catch (e) { saida = e.status ?? 1; }
    const staged = existsSync(join(dir, `src-tauri/binaries/anna-${triple}`));
    if (aceita && (saida !== 0 || !staged)) falha(`anna ${versao} is the floor and was not staged (exit ${saida})`);
    if (!aceita && (saida === 0 || staged)) falha(`anna ${versao} is below the floor and was ${staged ? "staged" : "accepted"}`);
    rmSync(dir, { recursive: true, force: true });
  }
}

// 2. the real override block
const FONTE = readFileSync(join(RAIZ, "build-local.sh"), "utf8");
const a = FONTE.indexOf("  # ── bundle overrides (1.6.24) ──");
const b = FONTE.indexOf("  # ── end bundle overrides ──");
if (a < 0 || b < 0) { falha("the bundle-override block moved out of build-local.sh"); }
else {
  const BLOCO = FONTE.slice(a, b);
  // Linux and macOS declare three sidecars (anna, the Pentest engine, the capture sidecar); each one can
  // be absent on its own. The list is built from what is staged: emptying it for one missing file would
  // drop the others from the bundle. Every combination is checked, with and without the updater key, and
  // the expected config is derived here from the combination alone, never read from build-local.sh.
  const NOMES = ["anna", "pentest", "traffic-capture"];
  let casos = 0;
  for (let mascara = 0; mascara < 8; mascara++) {
    for (const updater of [1, 0]) {
      const presentes = NOMES.filter((_, i) => mascara & (1 << i));
      const bundle = {};
      if (updater === 0) bundle.createUpdaterArtifacts = false;
      if (presentes.length !== NOMES.length) bundle.externalBin = presentes.map((n) => `binaries/${n}`);
      const esperado = Object.keys(bundle).length ? { bundle } : "";
      const dir = mkdtempSync(join(tmpdir(), "shvia-cfg-"));
      mkdirSync(join(dir, "src-tauri/binaries"), { recursive: true });
      for (const n of presentes) writeFileSync(join(dir, `src-tauri/binaries/${n}-${triple}`), "x");
      const out = execFileSync("bash", ["-c", `set -euo pipefail\nUPDATER_ARTIFACTS=${updater}\n${BLOCO}\nprintf '%s' "$_CFG"`], { cwd: dir, encoding: "utf8" });
      rmSync(dir, { recursive: true, force: true });
      // The block also logs a line for the build output; the config is what printf wrote last.
      const cfg = out.slice(out.lastIndexOf("\n") + 1);
      const obtido = cfg === "" ? "" : JSON.parse(cfg);
      casos++;
      if (JSON.stringify(obtido) !== JSON.stringify(esperado)) {
        falha(`staged=[${presentes}] updater=${updater}: expected ${JSON.stringify(esperado)}, got ${JSON.stringify(obtido)}`);
      }
    }
  }
  casosVerificados = casos;

  // --no-anna must remove anna's files only: the folder also holds the other sidecars.
  if (/rm -rf src-tauri\/binaries(?!\/)/.test(FONTE)) {
    falha("build-local.sh still deletes the whole src-tauri/binaries folder; --no-anna would take the other sidecars with it");
  }
  if (!FONTE.includes("rm -f src-tauri/binaries/anna-*")) falha("--no-anna no longer removes anna's own files");
}

if (falhas) { console.error(`\n${falhas} problem(s): the bundle would carry a stale engine, or the build would fail without one.`); process.exit(1); }
console.log(`[empacota-sem-anna] sem anna o velho sai e o Tauri é avisado; cada sidecar ausente sai da lista sozinho — ${4 + casosVerificados} casos`);
