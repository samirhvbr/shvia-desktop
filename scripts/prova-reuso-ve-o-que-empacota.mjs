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
 *  1. consistency — every resource, icon and external binary that `tauri.conf.json` and the
 *     per-platform `tauri.<os>.conf.json` bundle has its root in the list, and so has each of those
 *     config files (read from the configs, not copied here, so a new resource fails this). Since
 *     1.38.13 also every file the Rust source compiles in (`include_str!`, `include_bytes!`): the
 *     Pentest pin, `pentest-engine.json`, is one, and a commit that only bumped it reused the old
 *     bundle. And every root in the list is inside ENTRADAS_DO_BUILD, the inputs `--publish` refuses
 *     to ship dirty, which also missed the pin;
 *  2. behavior — the real `find`, cut out of the script, sees a newer runner file and ignores a
 *     newer file under node_modules;
 *  3. the engine (1.35.4) — the real `can_reuse_build`, cut out of the script and run in a temp tree
 *     with stand-in `anna` binaries, refuses the reuse when `--anna` names another version than the
 *     bundle's, or PATH has a newer one, and keeps it otherwise. Before 1.35.4 the owner's Mac
 *     reused 1.35.3 with `--anna` 0.15.3 and the bundle kept its older engine;
 *  4. the rest of the run's choices (1.38.13) — the same `can_reuse_build`: `--bundles` must be what
 *     the bundle on disk recorded, `--no-pentest`/`--no-traffic` need no staged sidecar of theirs, a
 *     default run needs the capture sidecar, and the Pentest engine staged must have the SHA-256 of
 *     the one this run would bundle (`--pentest`, or the pin where the platform bundles it). Until
 *     1.38.13 a reused bundle ignored all four.
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync, mkdirSync, readdirSync, readFileSync, writeFileSync, rmSync, utimesSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { dirname, join, posix, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { bundledByDefault } from "./pentest-bundling.mjs";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const FONTE = readFileSync(join(RAIZ, "build-local.sh"), "utf8");
const i = FONTE.indexOf('novas="$(find ');
if (i < 0) { console.error("🔴 the reuse freshness `find` moved out of build-local.sh."); process.exit(1); }
const FIND = FONTE.slice(i, FONTE.indexOf(')"', i) + 2);
const lista = new Set(FIND.replace(/\\\n/g, " ").split(/\s+/));

const CONFIGS = readdirSync(join(RAIZ, "src-tauri")).filter((n) => /^tauri(\.[a-z]+)?\.conf\.json$/.test(n)).sort();
const confs = CONFIGS.map((n) => JSON.parse(readFileSync(join(RAIZ, "src-tauri", n), "utf8")));
/** The root of a path relative to the repository: `src-tauri/<first folder>`, a top-level folder, or the file. */
const raizDoRepo = (n) => {
  const partes = posix.normalize(n).split("/");
  if (partes[0] === "src-tauri") return partes.length > 2 ? partes.slice(0, 2).join("/") : n;
  return partes.length > 1 ? partes[0] : n;
};
const raizDe = (p) => {
  // bundle paths are relative to src-tauri/; take the first real directory, or the file itself.
  const n = posix.normalize(posix.join("src-tauri", p));
  const partes = n.split("/");
  if (partes[0] === "src-tauri") return partes.length > 2 ? partes.slice(0, 2).join("/") : n;
  return partes.length > 1 ? partes[0] : n;
};
// Every file the Rust source compiles in, resolved from the file that includes it. Tests' includes count
// too: telling them apart would take a Rust parser, and an extra root only costs a rebuild.
const incluidos = [];
const varrer = (dir) => {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) varrer(p);
    else if (e.name.endsWith(".rs")) {
      for (const m of readFileSync(p, "utf8").matchAll(/include_(?:str|bytes)!\(\s*"([^"]+)"\s*\)/g)) {
        incluidos.push(relative(RAIZ, resolve(dirname(p), m[1])).split(sep).join("/"));
      }
    }
  }
};
varrer(join(RAIZ, "src-tauri/src"));

const exigidos = new Set([
  ...confs.flatMap((conf) => [
    ...(conf.bundle?.resources ?? []).map(raizDe),
    ...(conf.bundle?.icon ?? []).map(raizDe),
    ...(conf.bundle?.externalBin ?? []).map(raizDe),
  ]),
  ...CONFIGS.map((n) => `src-tauri/${n}`),
  ...incluidos.map(raizDoRepo),
  // The capture sidecar is BUILT from here (scripts/stage-traffic-capture.mjs); no config names it.
  "src-tauri/traffic-capture",
  "src-tauri/Cargo.lock", "src-tauri/build.rs", "package-lock.json",
]);

let falhas = 0;
for (const r of exigidos) {
  if (!lista.has(r)) { falhas++; console.error(`🔴 ${r} is bundled (or builds the bundle) but the reuse check does not look at it`); }
}

// What the reuse check looks at must be committed to publish: ENTRADAS_DO_BUILD lists it by coarser roots.
const k = FONTE.indexOf('ENTRADAS_DO_BUILD="');
const entradas = k < 0 ? [] : FONTE.slice(k + 19, FONTE.indexOf('"', k + 19)).split(/\s+/).filter(Boolean);
if (!entradas.length) { falhas++; console.error("🔴 ENTRADAS_DO_BUILD moved out of build-local.sh"); }
// The find's starting points: everything between `find` and the first `\(`.
const pontos = FIND.slice(FIND.indexOf("find ") + 5, FIND.indexOf("\\(")).replace(/\\\n/g, " ").split(/\s+/).filter(Boolean);
if (!pontos.includes("src")) { falhas++; console.error("🔴 the reuse find's starting points could not be read"); }
for (const r of pontos) {
  if (!entradas.some((e) => r === e || r.startsWith(`${e}/`))) {
    falhas++;
    console.error(`🔴 ${r} makes the bundle stale but is not in ENTRADAS_DO_BUILD: --publish would ship it uncommitted`);
  }
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
  rmSync(join(dir, "claude-runner/claude-runner.mjs"));
  // 1.38.13: the capture crate's own build output is not a source; its source and the Pentest pin are.
  mkdirSync(join(dir, "src-tauri/traffic-capture/target/release"), { recursive: true });
  writeFileSync(join(dir, "src-tauri/traffic-capture/target/release/traffic-capture"), "x");
  if (roda() !== "") { falhas++; console.error("🔴 a newer file under the capture crate's target/ made the build look stale"); }
  mkdirSync(join(dir, "src-tauri/traffic-capture/src"), { recursive: true });
  writeFileSync(join(dir, "src-tauri/traffic-capture/src/main.rs"), "x");
  if (!roda().includes("src-tauri/traffic-capture/src/main.rs")) { falhas++; console.error("🔴 a newer capture sidecar source did not stop the reuse"); }
  rmSync(join(dir, "src-tauri/traffic-capture/src/main.rs"));
  writeFileSync(join(dir, "pentest-engine.json"), "{}");
  if (!roda().includes("pentest-engine.json")) { falhas++; console.error("🔴 a newer Pentest pin did not stop the reuse"); }
} finally { rmSync(dir, { recursive: true, force: true }); }

// 3 and 4: the real can_reuse_build, cut out of the script, in a temp tree that holds a bundle of 9.9.9 whose one
// artifact matches release.json and is newer than every source, so steps 1 to 4 pass and the case decides. Shell
// stand-ins, so not on Windows, which has no reuse path (build-local.ps1 always rebuilds).
let casosAnna = 0;
let casosEscolhas = 0;
if (process.platform !== "win32") {
  const corta = (nome) => {
    const a = FONTE.indexOf(`\n${nome}() {`);
    const b = FONTE.indexOf("\n}\n", a);
    if (a < 0 || b < 0) { falhas++; console.error(`🔴 ${nome}() moved out of build-local.sh`); return ""; }
    return FONTE.slice(a, b + 3);
  };
  const FUNCOES = corta("_sha256") + corta("can_reuse_build");
  const triple = /host: (\S+)/.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))[1];
  const plataforma = { darwin: "macos", linux: "linux" }[process.platform];
  const falso = (caminho, versao) => writeFileSync(caminho, `#!/bin/sh\necho 'anna ${versao}'\n`, { mode: 0o755 });
  const sha = (texto) => createHash("sha256").update(texto).digest("hex");

  /**
   * One run of can_reuse_build. `prepara(dir)` stages what the case needs, `vars(dir)` are the run's flags over a run
   * that asks for no sidecar at all, and `confere(dir)` looks at the tree before it is removed. Returns the output.
   */
  function reusaria({ prepara = () => {}, vars = () => ({}), confere = () => {} } = {}) {
    const dir = mkdtempSync(join(tmpdir(), "shvia-reuso-"));
    try {
      mkdirSync(join(dir, "scripts"));
      mkdirSync(join(dir, "src-tauri/binaries"), { recursive: true });
      mkdirSync(join(dir, "src-tauri/target/release/bundle/deb"), { recursive: true });
      mkdirSync(join(dir, "bin"));
      for (const f of ["stage-anna.mjs", "stage-pentest.mjs", "pentest-bundling.mjs"]) copyFileSync(join(RAIZ, "scripts", f), join(dir, "scripts", f));
      copyFileSync(join(RAIZ, "pentest-engine.json"), join(dir, "pentest-engine.json"));
      writeFileSync(join(dir, "version.md"), "9.9.9\n");
      writeFileSync(join(dir, "src-tauri/target/release/bundle/.bundles"), "all\n");
      writeFileSync(join(dir, "release.json"), JSON.stringify({ version: "9.9.9", platforms: { [plataforma]: { artifacts: [{ file: "ShvIA_9.9.9_amd64.deb", sha256: sha("pacote") }] } } }));
      prepara(dir);
      const artefato = join(dir, "src-tauri/target/release/bundle/deb/ShvIA_9.9.9_amd64.deb");
      writeFileSync(artefato, "pacote");
      const depois = new Date(Date.now() + 3600_000); // newer than every source: step 4 passes
      utimesSync(artefato, depois, depois);
      const run = { FORCE_BUILD: 0, NO_ANNA: 1, ANNA_FROM: "", BUNDLES: "", NO_PENTEST: 1, PENTEST_FROM: "", NO_TRAFFIC: 1, ...vars(dir) };
      const script = `set -euo pipefail
${Object.entries(run).map(([k, v]) => `${k}=${JSON.stringify(String(v))}`).join("\n")}
REUSE_REF=""; REUSE_N=0; REUSE_MOTIVO=""; REUSE_ANNA=""; REUSE_PENTEST=""
${FUNCOES}
if can_reuse_build; then printf 'REUSA|%s|%s' "$REUSE_ANNA" "$REUSE_PENTEST"; else printf 'NAO|%s' "$REUSE_MOTIVO"; fi`;
      const out = execFileSync("bash", ["-c", script], {
        cwd: dir, encoding: "utf8",
        // The cache too: a check that downloaded would show here as a download, never in the real cache.
        env: { ...process.env, PATH: `${join(dir, "bin")}:${process.env.PATH}`, XDG_CACHE_HOME: join(dir, "cache"),
          SHVIA_ANNA_BIN: "", SHVIA_PENTEST_BIN: "", SHVIA_BUNDLE_PENTEST_ON_MAC: "" },
      });
      confere(dir);
      return out;
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }
  const julga = (porque, esperado, out) => {
    if (out.startsWith("REUSA|") !== esperado) {
      falhas++;
      console.error(`🔴 ${porque}: expected ${esperado ? "reuse" : "rebuild"}, got ${out}`);
    }
  };

  // 3. [no bundle staged = null] [--anna version or null] [PATH version] [--no-anna] → reuse?
  const casos = [
    { bundle: "0.16.2", flag: "0.16.3", path: "0.16.2", reusa: false, porque: "--anna newer than the bundle's " },
    { bundle: "0.16.3", flag: "0.16.3", path: "0.16.2", reusa: true, porque: "--anna is the bundle's version" },
    { bundle: "0.16.3", flag: "0.16.2", path: "0.16.3", reusa: false, porque: "--anna older than the bundle's: asked for by name, it wins either way" },
    { bundle: "0.16.3", flag: null, path: "0.16.2", reusa: true, porque: "an older anna on PATH does not displace the bundle's" },
    { bundle: "0.16.2", flag: null, path: "0.16.3", reusa: false, porque: "a newer anna on PATH (install.sh ran after the build)" },
    { bundle: null, flag: "0.16.3", path: "0.16.3", reusa: false, porque: "the bundle has no anna and this run would package one" },
    { bundle: "0.16.3", flag: null, path: "0.16.3", noAnna: true, reusa: false, porque: "--no-anna and the bundle carries anna" },
    { bundle: "0.16.1", flag: null, path: "0.16.1", reusa: false, porque: "a staged Anna below the credential floor cannot ship" },
  ];
  for (const c of casos) {
    const noBundle = (dir) => join(dir, `src-tauri/binaries/anna-${triple}`);
    const out = reusaria({
      prepara: (dir) => {
        if (c.bundle) falso(noBundle(dir), c.bundle);
        falso(join(dir, "bin/anna"), c.path);
        if (c.flag) falso(join(dir, "pedido"), c.flag);
      },
      vars: (dir) => ({ NO_ANNA: c.noAnna ? 1 : 0, ANNA_FROM: c.flag ? join(dir, "pedido") : "" }),
      confere: (dir) => {
        if (c.bundle && readFileSync(noBundle(dir), "utf8") !== `#!/bin/sh\necho 'anna ${c.bundle}'\n`) {
          falhas++;
          console.error(`🔴 ${c.porque}: the reuse check changed the staged anna`);
        }
      },
    });
    casosAnna++;
    julga(c.porque, c.reusa, out);
  }

  // 4. The rest of the run's choices (1.38.13).
  const estagia = (dir, nome, conteudo) => writeFileSync(join(dir, `src-tauri/binaries/${nome}-${triple}`), conteudo, { mode: 0o755 });
  const fixaPin = (dir, conteudo) => {
    const m = JSON.parse(readFileSync(join(dir, "pentest-engine.json"), "utf8"));
    m.assets = { ...m.assets, [triple]: { file: `pentest-${triple}`, sha256: sha(conteudo) } };
    writeFileSync(join(dir, "pentest-engine.json"), JSON.stringify(m));
  };
  const comPentest = (extra = {}) => (dir) => ({ NO_PENTEST: 0, ...(typeof extra === "function" ? extra(dir) : extra) });
  const escolhas = [
    { porque: "the run asks for what the bundle was built with", reusa: true },
    { porque: "--bundles deb against a bundle built with every target", vars: () => ({ BUNDLES: "deb" }), reusa: false },
    { porque: "--bundles deb against a bundle built with --bundles deb", vars: () => ({ BUNDLES: "deb" }),
      prepara: (d) => writeFileSync(join(d, "src-tauri/target/release/bundle/.bundles"), "deb\n"), reusa: true },
    { porque: "a default run against a bundle built with --bundles deb", prepara: (d) => writeFileSync(join(d, "src-tauri/target/release/bundle/.bundles"), "deb\n"), reusa: false },
    { porque: "a bundle that does not record its --bundles (built before 1.38.13)", prepara: (d) => rmSync(join(d, "src-tauri/target/release/bundle/.bundles")), reusa: false },
    { porque: "--no-traffic, and the bundle carries the capture sidecar", prepara: (d) => estagia(d, "traffic-capture", "captura"), reusa: false },
    { porque: "a run without --no-traffic, and the bundle has no capture sidecar", vars: () => ({ NO_TRAFFIC: 0 }), reusa: false },
    { porque: "a run without --no-traffic, and the bundle carries the capture sidecar", vars: () => ({ NO_TRAFFIC: 0 }),
      prepara: (d) => estagia(d, "traffic-capture", "captura"), reusa: true },
    { porque: "--no-pentest, and the bundle carries the Pentest engine", prepara: (d) => estagia(d, "pentest", "motor A"), reusa: false },
    { porque: "--pentest names the engine the bundle carries", vars: comPentest((d) => ({ PENTEST_FROM: join(d, "motor") })),
      prepara: (d) => { estagia(d, "pentest", "motor A"); writeFileSync(join(d, "motor"), "motor A"); }, reusa: true },
    { porque: "--pentest names another engine than the bundle's", vars: comPentest((d) => ({ PENTEST_FROM: join(d, "motor") })),
      prepara: (d) => { estagia(d, "pentest", "motor A"); writeFileSync(join(d, "motor"), "motor B"); }, reusa: false },
    { porque: "--pentest names a file that does not exist", vars: comPentest((d) => ({ PENTEST_FROM: join(d, "nao-existe") })),
      prepara: (d) => estagia(d, "pentest", "motor A"), reusa: false },
    // A run without --pentest bundles the pinned engine where the platform bundles it by default (Linux), none elsewhere (a Mac).
    ...(bundledByDefault(process.platform, {}) ? [
      { porque: "a default run, and the bundle carries the pinned engine", vars: comPentest(),
        prepara: (d) => { estagia(d, "pentest", "motor A"); fixaPin(d, "motor A"); }, reusa: true },
      { porque: "a default run after a --no-pentest build", vars: comPentest(), prepara: (d) => fixaPin(d, "motor A"), reusa: false },
      { porque: "a default run, and the bundle carries another engine than the pin", vars: comPentest(),
        prepara: (d) => { estagia(d, "pentest", "motor B"); fixaPin(d, "motor A"); }, reusa: false },
    ] : [
      { porque: "a default run where the engine is not bundled, and the bundle has none", vars: comPentest(), reusa: true },
      { porque: "a default run where the engine is not bundled, and the bundle carries one", vars: comPentest(),
        prepara: (d) => estagia(d, "pentest", "motor A"), reusa: false },
    ]),
  ];
  for (const c of escolhas) {
    julga(c.porque, c.reusa, reusaria({ prepara: c.prepara, vars: c.vars }));
    casosEscolhas++;
  }
}

if (falhas) {
  console.error(`\n${falhas} problem(s): a commit that changed only a bundled input would ship the old bundle.`);
  process.exit(1);
}
console.log(`[reuso] ${exigidos.size} entradas do bundle na lista de frescor (${CONFIGS.length} configs, ${incluidos.length} includes do Rust), todas em ENTRADAS_DO_BUILD · runner, pin e fonte da captura invalidam o reuso, node_modules e target não · ${casosAnna} casos do anna · ${casosEscolhas} das outras escolhas da rodada`);
