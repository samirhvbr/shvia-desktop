#!/usr/bin/env node
// Stages the `traffic-capture` sidecar for the Tauri `externalBin` bundling step — the same
// mechanism `scripts/stage-anna.mjs` uses, but unlike `anna` (built and released in a separate
// repository), this binary is OUR OWN code (`src-tauri/traffic-capture/`), so the default path
// here is to build it, not to look for it already installed somewhere.
//
// ── Why a cargo build here instead of letting `cargo build` at the top level reach it ──
// `traffic-capture` depends on `pcap` (libpcap/Npcap linkage) on purpose: the GUI crate
// (`src-tauri`) must never link it, so the privilege boundary is a separate compilation unit,
// not a feature flag. That means this crate is NOT a workspace member of `src-tauri` and is
// built with its own `--manifest-path`. See docs/TRAFFIC/README.md.
//
// ── Which platforms ──
// Linux and macOS declare it in `tauri.linux.conf.json` / `tauri.macos.conf.json`, next to `anna` and the
// Pentest engine. Windows does not bundle it in this version: building the crate there needs the Npcap SDK,
// and running it needs the Npcap driver, neither of which this repository can provide (docs/TRAFFIC/README.md).
//
// Absence of a successful build is NOT a fatal error for the app build, same posture as `stage-anna.mjs`:
// the staged file of an earlier build is removed (never bundled stale), `build-local.sh` drops the sidecar
// from `externalBin`, and Traffic then answers "capture is unavailable in this build" instead of failing.
import { chmodSync, copyFileSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const CRATE_DIR = join(ROOT, "src-tauri/traffic-capture");
const DESTINO_DIR = join(ROOT, "src-tauri/binaries");

const args = process.argv.slice(2);
const opt = (n) => {
  const i = args.indexOf(n);
  return i >= 0 && args[i + 1] ? args[i + 1] : null;
};

function targetTriple() {
  try {
    const saida = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
    const m = saida.match(/^host:\s*(\S+)$/m);
    if (m) return m[1];
  } catch {
    /* rustc absent falls through to the error below */
  }
  return null;
}

if (process.platform === "win32") {
  console.log("[stage-traffic-capture] Windows does not bundle the capture sidecar in this version — nothing to stage.");
  process.exit(0);
}

const triple = targetTriple();
if (!triple) {
  console.error("[stage-traffic-capture] could not determine the target triple (rustc missing?).");
  process.exit(1);
}
const exeSuffix = process.platform === "win32" ? ".exe" : "";
const destino = join(DESTINO_DIR, `traffic-capture-${triple}${exeSuffix}`);

const doFlag = opt("--from");
let origem = doFlag ? resolve(doFlag) : null;

if (!origem) {
  console.log("[stage-traffic-capture] building the sidecar (cargo build --release)...");
  try {
    execFileSync("cargo", ["build", "--release", "--manifest-path", join(CRATE_DIR, "Cargo.toml")], {
      stdio: "inherit",
    });
  } catch {
    // Same posture as a missing `anna`: the app ships without live capture rather than
    // failing the whole build. A build failure here is almost always a missing libpcap
    // development package on the BUILD machine (see docs/TRAFFIC/README.md), not a defect
    // in the sidecar's own source.
    console.warn("[stage-traffic-capture] build failed — shipping without the capture sidecar.");
    console.warn("                        Traffic will show its permission/unavailable message.");
    console.warn("                        See docs/TRAFFIC/README.md for the libpcap-dev prerequisite.");
    if (existsSync(destino)) rmSync(destino);
    process.exit(0);
  }
  origem = join(CRATE_DIR, "target/release", `traffic-capture${exeSuffix}`);
}

if (!existsSync(origem)) {
  console.warn(`[stage-traffic-capture] ${origem} not found — shipping without the capture sidecar.`);
  if (existsSync(destino)) rmSync(destino);
  process.exit(0);
}

// A sidecar that cannot start (libpcap missing on this machine, a truncated copy) must not be bundled:
// `--list-interfaces` needs no capture capability, so running it here proves the binary loads and answers.
try {
  const resposta = execFileSync(origem, ["--list-interfaces"], { encoding: "utf8", timeout: 15000 });
  if (!Array.isArray(JSON.parse(resposta.split("\n")[0]).interfaces)) throw new Error("no interface list in the answer");
} catch (erro) {
  console.warn(`[stage-traffic-capture] ${origem} does not run (${erro.message}) — shipping without the capture sidecar.`);
  if (existsSync(destino)) rmSync(destino);
  process.exit(0);
}

mkdirSync(DESTINO_DIR, { recursive: true });
copyFileSync(origem, destino);
if (process.platform !== "win32") chmodSync(destino, 0o755);
console.log(`[stage-traffic-capture] staged ${destino}`);
