#!/usr/bin/env node
/**
 * The reuse check sees every input the bundle carries (1.6.23).
 *
 * `build-local.sh` reuses the last bundle when release.json matches the version and no source is
 * newer than the artifacts. The "source" list was hand-written and missed what the bundle carries
 * beyond Rust and the web shell — `claude-runner/` (bundle.resources), the icons, Cargo.lock,
 * build.rs — so a second commit under the same version that only changed one of those shipped
 * the OLD bundle.
 *
 * Two checks:
 *  1. consistency — every resource, icon and external binary `tauri.conf.json` bundles has its
 *     root in the list (read from the config, not copied here, so a new resource fails this);
 *  2. behavior — the real `find`, cut out of the script, sees a newer runner file and ignores a
 *     newer file under node_modules.
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, utimesSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, posix } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const FONTE = readFileSync(join(RAIZ, "build-local.sh"), "utf8");
const i = FONTE.indexOf('novas="$(find ');
if (i < 0) { console.error("🔴 the reuse freshness `find` moved out of build-local.sh."); process.exit(1); }
const FIND = FONTE.slice(i, FONTE.indexOf(')"', i) + 2);
const lista = new Set(FIND.replace(/\\\n/g, " ").split(/\s+/));

const conf = JSON.parse(readFileSync(join(RAIZ, "src-tauri/tauri.conf.json"), "utf8"));
const raizDe = (p) => {
  // bundle paths are relative to src-tauri/; take the first real directory, or the file itself.
  const n = posix.normalize(posix.join("src-tauri", p));
  const partes = n.split("/");
  if (partes[0] === "src-tauri") return partes.length > 2 ? partes.slice(0, 2).join("/") : n;
  return partes.length > 1 ? partes[0] : n;
};
const exigidos = new Set([
  ...(conf.bundle?.resources ?? []).map(raizDe),
  ...(conf.bundle?.icon ?? []).map(raizDe),
  ...(conf.bundle?.externalBin ?? []).map(raizDe),
  "src-tauri/Cargo.lock", "src-tauri/build.rs", "package-lock.json",
]);

let falhas = 0;
for (const r of exigidos) {
  if (!lista.has(r)) { falhas++; console.error(`🔴 ${r} is bundled (or builds the bundle) but the reuse check does not look at it`); }
}

const dir = mkdtempSync(join(tmpdir(), "shvia-reuso-"));
try {
  const ref = join(dir, "artefato");
  writeFileSync(ref, "x");
  const antigo = new Date(Date.now() - 3600_000);
  utimesSync(ref, antigo, antigo);
  const roda = () => execFileSync("bash", ["-c", `set -euo pipefail\nref=${JSON.stringify(ref)}\n${FIND}\nprintf '%s' "$novas"`], { cwd: dir, encoding: "utf8" });
  mkdirSync(join(dir, "claude-runner/node_modules/pkg"), { recursive: true });
  writeFileSync(join(dir, "claude-runner/node_modules/pkg/index.js"), "x");
  if (roda() !== "") { falhas++; console.error("🔴 a newer file under node_modules made the build look stale"); }
  writeFileSync(join(dir, "claude-runner/claude-runner.mjs"), "x");
  if (!roda().includes("claude-runner/claude-runner.mjs")) { falhas++; console.error("🔴 a newer runner file did not stop the reuse"); }
} finally { rmSync(dir, { recursive: true, force: true }); }

if (falhas) {
  console.error(`\n${falhas} problem(s): a commit that changed only a bundled input would ship the old bundle.`);
  process.exit(1);
}
console.log(`[reuso] ${exigidos.size} entradas do bundle na lista de frescor · runner novo invalida o reuso, node_modules não`);
