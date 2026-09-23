#!/usr/bin/env node
/**
 * A Mac build without the Apple signature cannot be published (1.6.22).
 *
 * Until 1.6.22 `--publish` shipped a `--no-sign` test build — directly, or through the reuse
 * path, which checks version, sha256 and freshness but never the signature — and the build-time
 * checks only printed ("BUILD SAI SEM ASSINAR", a failed `codesign --verify`). Two guards now:
 * `--publish` with `--no-sign` is refused right after the arguments are read, and at publish time
 * on macOS the `.app` must pass `codesign --verify --deep --strict` and the `.dmg` `stapler
 * validate`.
 *
 * Both are cut out of build-local.sh and run alone — never the whole script: without the
 * refusal, a reversal of this proof would START A REAL BUILD AND PUBLISH. `codesign` and `xcrun`
 * are fakes on PATH whose exit codes each case chooses, so this runs on Linux CI.
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const FONTE = readFileSync(join(RAIZ, "build-local.sh"), "utf8");
const ini = FONTE.indexOf("# 🔴 A test build cannot be published");
const RECUSA = ini < 0 ? null : FONTE.slice(ini, FONTE.indexOf("\nfi\n", ini) + 4);
const f = FONTE.indexOf("confere_assinatura_apple() {");
const CONFERE = f < 0 ? null : FONTE.slice(f, FONTE.indexOf("\n}\n", f) + 3);
if (!RECUSA || !CONFERE) {
  console.error("🔴 the --publish/--no-sign refusal or confere_assinatura_apple moved out of build-local.sh.");
  process.exit(1);
}

function recusa(publish, noSign) {
  try {
    execFileSync("bash", ["-c", `set -euo pipefail\nPUBLISH=${publish}\nNO_SIGN=${noSign}\n${RECUSA}\n`], { stdio: "ignore" });
    return 0;
  } catch (e) { return e.status ?? 1; }
}

function confere({ os = "macOS", app = true, dmg = true, codesign = 0, stapler = 0 }) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-apple-"));
  try {
    const bin = join(dir, "bin");
    mkdirSync(bin);
    writeFileSync(join(bin, "codesign"), `#!/bin/sh\nexit ${codesign}\n`, { mode: 0o755 });
    writeFileSync(join(bin, "xcrun"), `#!/bin/sh\nexit ${stapler}\n`, { mode: 0o755 });
    if (app) mkdirSync(join(dir, "src-tauri/target/release/bundle/macos/ShvIA.app"), { recursive: true });
    if (dmg) {
      mkdirSync(join(dir, "src-tauri/target/release/bundle/dmg"), { recursive: true });
      writeFileSync(join(dir, "src-tauri/target/release/bundle/dmg/ShvIA_9.9.9_aarch64.dmg"), "x");
    }
    execFileSync("bash", ["-c", `set -euo pipefail\n_BUILD_OS=${os}\n${CONFERE}\nconfere_assinatura_apple`], {
      cwd: dir, stdio: "ignore", env: { ...process.env, PATH: `${bin}:${process.env.PATH}` },
    });
    return 0;
  } catch (e) { return e.status ?? 1; } finally { rmSync(dir, { recursive: true, force: true }); }
}

const CASOS = [
  ["🔴 --publish com --no-sign é recusado", () => recusa(1, 1), 2],
  ["--publish assinado segue", () => recusa(1, 0), 0],
  ["--no-sign local (sem publicar) segue", () => recusa(0, 1), 0],
  [".app e .dmg assinados e grampeados", () => confere({}), 0],
  ["🔴 .app sem assinatura", () => confere({ codesign: 1 }), 1],
  ["🔴 .dmg sem notarização grampeada", () => confere({ stapler: 1 }), 1],
  ["só .dmg (build --bundles dmg), grampeado", () => confere({ app: false }), 0],
  ["🔴 nada no bundle para conferir", () => confere({ app: false, dmg: false }), 1],
  ["fora do macOS a conferência não se aplica", () => confere({ os: "Linux", codesign: 1, stapler: 1 }), 0],
];

let falhas = 0;
for (const [nome, rodar, esperado] of CASOS) {
  const obtido = rodar();
  if (obtido !== esperado) { falhas++; console.error(`🔴 ${nome}: esperado status ${esperado}, obtido ${obtido}`); }
}
if (falhas) {
  console.error(`\n${falhas} de ${CASOS.length} casos falharam: um build do Mac sem assinatura pode voltar a ser publicado.`);
  process.exit(1);
}
console.log(`[assinatura-apple] ${CASOS.length} casos · build sem assinatura Apple não sobe, por nenhum caminho`);
