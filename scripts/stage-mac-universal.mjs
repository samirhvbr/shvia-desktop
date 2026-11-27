#!/usr/bin/env node
// Stages the sidecars of a universal macOS build (Apple Silicon + Intel in one app, 1.44.2).
//
// WHY UNIVERSAL AND NOT A SECOND INTEL BUILD: the update endpoint (shvia-web
// DesktopUpdateController) reads only the OS from the plugin's `darwin-<arch>` target and hands
// every Mac the one `.app.tar.gz` the manifest lists. A separate Intel build would need a second
// updater file with a different name, a manifest that knows architectures, and an endpoint that
// picks by them; until all three agree, an Apple Silicon build reaching an Intel Mac through the
// updater would install an app that cannot start there. A universal app is the one file that is
// right on both, with nothing to change on the server. ADR-052 in docs/decisoes.md.
//
// WHAT TAURI READS: `tauri build --target universal-apple-darwin` compiles the app once per
// architecture, and each compile requires `binaries/<name>-<that triple>` to exist; the bundle
// then takes `binaries/<name>-universal-apple-darwin`. So every staged sidecar needs three files.
// The Apple Silicon one is what stage-anna / stage-traffic-capture already staged on this Mac;
// this script adds the Intel one and joins the two with `lipo`.
//
// Where the Intel slices come from:
//   anna             `--anna-x86_64 <path>`, $SHVIA_ANNA_BIN_X86_64, or built here from a clean
//                    shvia-code checkout whose version.md is the version of the staged anna
//                    ($SHVIA_CODE_DIR, else the shvia-code folder next to this repository's
//                    main checkout). It must answer the same `anna X.Y.Z` under Rosetta.
//   traffic-capture  built from src-tauri/traffic-capture for x86_64-apple-darwin, and run once
//                    under Rosetta (`--list-interfaces`), the check the host copy passes.
//   pentest          refused: no Intel Mac engine is published (pentest-engine.json), and a Mac
//                    build does not bundle the engine by default anyway (ADR-046).
//
// Every check runs the Intel binary, so the build Mac needs Rosetta
// (`softwareupdate --install-rosetta --agree-to-license`) and the Rust target
// (`rustup target add x86_64-apple-darwin`).
import { chmodSync, copyFileSync, existsSync, readFileSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const BIN = join(ROOT, "src-tauri/binaries");
const HOST = "aarch64-apple-darwin";
const INTEL = "x86_64-apple-darwin";
const UNIVERSAL = "universal-apple-darwin";

const args = process.argv.slice(2);
const opt = (n) => {
  const i = args.indexOf(n);
  return i >= 0 && args[i + 1] ? args[i + 1] : null;
};

function falha(...linhas) {
  console.error(`[stage-mac-universal] ${linhas[0]}`);
  for (const l of linhas.slice(1)) console.error(`                     ${l}`);
  process.exit(1);
}

const run = (cmd, argv, extra = {}) => execFileSync(cmd, argv, { encoding: "utf8", ...extra });

if (process.platform !== "darwin") falha("a universal build is a macOS build; this is not a Mac.");
const host = /^host:\s*(\S+)$/m.exec(run("rustc", ["-vV"]))?.[1];
if (host !== HOST) {
  falha(`this Mac builds for ${host}; the universal build is made on Apple Silicon, which runs`,
    "the Intel slice under Rosetta to check it. An Intel build Mac cannot run the Apple Silicon one.");
}
if (!run("rustup", ["target", "list", "--installed"]).split("\n").includes(INTEL)) {
  falha(`the Rust target ${INTEL} is not installed: rustup target add ${INTEL}`);
}
try {
  run("arch", ["-x86_64", "/usr/bin/true"]);
} catch {
  falha("Rosetta is not installed, so the Intel slices cannot be run to check them:",
    "softwareupdate --install-rosetta --agree-to-license");
}

const archs = (p) => run("lipo", ["-archs", p]).trim().split(/\s+/).sort().join(" ");

/** Copies the Intel binary next to the host one and joins both into the universal file. */
function junta(nome, intel) {
  const hostFile = join(BIN, `${nome}-${HOST}`);
  if (archs(intel) !== "x86_64") falha(`${intel} is not an x86_64 binary (lipo says: ${archs(intel)}).`);
  if (archs(hostFile) !== "arm64") falha(`${hostFile} is not an arm64 binary (lipo says: ${archs(hostFile)}).`);
  const intelFile = join(BIN, `${nome}-${INTEL}`);
  const universal = join(BIN, `${nome}-${UNIVERSAL}`);
  copyFileSync(intel, intelFile);
  chmodSync(intelFile, 0o755);
  run("lipo", ["-create", hostFile, intelFile, "-output", universal]);
  chmodSync(universal, 0o755);
  if (archs(universal) !== "arm64 x86_64") falha(`${universal} did not come out with both slices.`);
  console.log(`[stage-mac-universal] ${nome}: arm64 + x86_64 -> ${universal}`);
}

function limpa(nome) {
  for (const t of [INTEL, UNIVERSAL]) rmSync(join(BIN, `${nome}-${t}`), { force: true });
}

// ── pentest ────────────────────────────────────────────────────────────────────
if (existsSync(join(BIN, `pentest-${HOST}`))) {
  falha("the Pentest engine is staged, and there is no Intel Mac engine to pair it with",
    "(pentest-engine.json pins aarch64-apple-darwin only). Build with --no-pentest, or",
    "--mac-arm-only for a local Apple Silicon build that carries it.");
}
limpa("pentest");

// ── anna ───────────────────────────────────────────────────────────────────────
function shvCodeDir() {
  if (process.env.SHVIA_CODE_DIR) return resolve(process.env.SHVIA_CODE_DIR);
  // The main checkout's parent, so a worktree of this repository finds the same sibling.
  const comum = resolve(ROOT, run("git", ["-C", ROOT, "rev-parse", "--git-common-dir"]).trim());
  for (const nome of ["shvia-code", "SHVIA-CODE"]) {
    const d = join(dirname(dirname(comum)), nome);
    if (existsSync(join(d, "Cargo.toml"))) return d;
  }
  return null;
}

const annaHost = join(BIN, `anna-${HOST}`);
if (!existsSync(annaHost)) {
  limpa("anna");
} else {
  const versao = run(annaHost, ["--version"], { timeout: 10_000 }).trim();
  const numero = /^anna (\d+\.\d+\.\d+)$/.exec(versao)?.[1];
  if (!numero) falha(`the staged ${annaHost} answered "${versao}", not a release version.`);

  let intel = opt("--anna-x86_64") ?? process.env.SHVIA_ANNA_BIN_X86_64 ?? null;
  if (intel) {
    intel = resolve(intel);
    if (!existsSync(intel)) falha(`the Intel anna named for this build does not exist: ${intel}`);
  } else {
    const dir = shvCodeDir();
    if (!dir) {
      falha("no Intel anna was named and no shvia-code checkout was found to build one.",
        "Set SHVIA_CODE_DIR to the checkout, or pass --anna-x86_64 <path>.");
    }
    const naPasta = readFileSync(join(dir, "version.md"), "utf8").trim();
    if (naPasta !== numero) {
      falha(`${dir} is at ${naPasta}; the staged anna is ${numero}. The two slices must be one`,
        `release: check out ${numero} there (git -C ${dir} checkout ${numero}), or pass --anna-x86_64.`);
    }
    const sujo = run("git", ["-C", dir, "status", "--porcelain", "--untracked-files=no"]).trim();
    if (sujo) {
      falha(`${dir} has uncommitted changes, so a binary built there is not release ${numero}:`,
        ...sujo.split("\n").slice(0, 5));
    }
    console.log(`[stage-mac-universal] building anna ${numero} for ${INTEL} in ${dir}...`);
    execFileSync("cargo", ["build", "--release", "--locked", "--target", INTEL], { cwd: dir, stdio: "inherit" });
    intel = join(dir, "target", INTEL, "release", "anna");
  }
  let resposta;
  try {
    resposta = run("arch", ["-x86_64", intel, "--version"], { timeout: 30_000 }).trim();
  } catch (e) {
    falha(`the Intel anna ${intel} did not answer --version under Rosetta (${e.message}).`);
  }
  if (resposta !== versao) {
    falha(`the Intel anna answers "${resposta}"; the staged one answers "${versao}". One release, two slices.`);
  }
  junta("anna", intel);
}

// ── traffic-capture ────────────────────────────────────────────────────────────
const capturaHost = join(BIN, `traffic-capture-${HOST}`);
if (!existsSync(capturaHost)) {
  limpa("traffic-capture");
} else {
  const crate = join(ROOT, "src-tauri/traffic-capture");
  console.log(`[stage-mac-universal] building traffic-capture for ${INTEL}...`);
  execFileSync("cargo", ["build", "--release", "--target", INTEL, "--manifest-path", join(crate, "Cargo.toml")], {
    stdio: "inherit",
  });
  const intel = join(crate, "target", INTEL, "release", "traffic-capture");
  try {
    const resposta = run("arch", ["-x86_64", intel, "--list-interfaces"], { timeout: 15_000 });
    if (!Array.isArray(JSON.parse(resposta.split("\n")[0]).interfaces)) throw new Error("no interface list in the answer");
  } catch (e) {
    // The host copy is already staged, so shipping without capture is not an option here: an
    // app whose Intel slice lacks a sidecar its Apple Silicon slice has is two different apps.
    falha(`the Intel traffic-capture does not run under Rosetta (${e.message}).`,
      "Fix it, or build with --no-traffic to ship both slices without capture.");
  }
  junta("traffic-capture", intel);
}
