#!/usr/bin/env node
/**
 * `--publish` ships only what is committed (1.6.34).
 *
 * Nothing tied a publish to the repository, and this checkout is shared by several sessions:
 * uncommitted changes could ship as "version X" and match no commit. The owner chose: refuse
 * when a BUILD input is dirty or untracked; warn for other dirty files and for a HEAD that is not
 * origin/master. This runs the real `confere_arvore_para_publicar` (cut out of build-local.sh)
 * in temp git repositories.
 */
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const FONTE = readFileSync(join(RAIZ, "build-local.sh"), "utf8");
const i = FONTE.indexOf("ENTRADAS_DO_BUILD=");
const j = FONTE.indexOf("confere_arvore_para_publicar() {");
if (i < 0 || j < 0) { console.error("🔴 the publish tree check moved out of build-local.sh."); process.exit(1); }
const BLOCO = FONTE.slice(i, FONTE.indexOf("\n}\n", j) + 3);

function repo() {
  const dir = mkdtempSync(join(tmpdir(), "shvia-publica-"));
  const git = (...a) => execFileSync("git", a, { cwd: dir, stdio: "ignore" });
  git("init", "-q"); git("config", "user.email", "t@t.tld"); git("config", "user.name", "t");
  for (const [f, c] of [["version.md", "1.0.0\n"], ["src/main.ts", "x"], ["README.md", "x"], ["package.json", "{}"], [".gitignore", "src-tauri/target/\n"]]) {
    mkdirSync(dirname(join(dir, f)), { recursive: true }); writeFileSync(join(dir, f), c);
  }
  git("add", "-A"); git("-c", "core.hooksPath=/dev/null", "commit", "-qm", "base");
  git("update-ref", "refs/remotes/origin/master", "HEAD");
  return { dir, git };
}
function roda(dir) {
  try { execFileSync("bash", ["-c", `set -euo pipefail\n${BLOCO}\nconfere_arvore_para_publicar`], { cwd: dir, stdio: "ignore" }); return 0; }
  catch (e) { return e.status ?? 1; }
}

const CASOS = [
  ["clean tree", () => {}, 0],
  ["🔴 build input modified", (r) => writeFileSync(join(r.dir, "src/main.ts"), "mudou"), 1],
  ["🔴 build input UNTRACKED", (r) => writeFileSync(join(r.dir, "src/novo.ts"), "x"), 1],
  // The version the publish announces, bumped on disk and never committed.
  ["🔴 version.md bumped and not committed", (r) => writeFileSync(join(r.dir, "version.md"), "1.0.1\n"), 1],
  ["dirty file outside the build (warning only)", (r) => writeFileSync(join(r.dir, "README.md"), "mudou"), 0],
  ["HEAD ahead of origin/master (warning only)", (r) => { writeFileSync(join(r.dir, "README.md"), "y"); r.git("add", "README.md"); r.git("-c", "core.hooksPath=/dev/null", "commit", "-qm", "c"); }, 0],
  ["ignored build output does not count", (r) => { mkdirSync(join(r.dir, "src-tauri/target"), { recursive: true }); writeFileSync(join(r.dir, "src-tauri/target/x"), "x"); }, 0],
];

let falhas = 0;
for (const [nome, prepara, esperado] of CASOS) {
  const r = repo(); prepara(r);
  const obtido = roda(r.dir);
  rmSync(r.dir, { recursive: true, force: true });
  if ((obtido === 0) !== (esperado === 0)) { falhas++; console.error(`🔴 ${nome}: expected ${esperado === 0 ? "go on" : "REFUSE"}, got status ${obtido}`); }
}
if (falhas) { console.error(`\n${falhas} case(s) failed: a publish could again ship what no commit holds.`); process.exit(1); }
console.log(`[publica-o-commitado] ${CASOS.length} cases · a dirty build input refuses the publish; the rest only warns`);
