#!/usr/bin/env node
/**
 * A runner install that fails keeps the runner that was there (1.38.13).
 *
 * `claude-runner/install.sh` used to copy the new files over the installed runner and run `npm ci`
 * there. `npm ci` deletes `node_modules` first, so an update that lost the network left the new
 * files with no Agent SDK. The app decides whether to reinstall by comparing those files and the
 * version (`code_bridge/atualizacao.rs`), so from the next start on it counted the runner as up to
 * date, and every Code turn died on the SDK import until the next app release.
 *
 * This runs the REAL install.sh, copied beside stand-in runner modules (the installer takes its
 * sources from its own folder), against a temporary XDG_DATA_HOME and HOME that already hold an
 * "old" install. npm runs offline with an empty cache, so a dependency in the lock fails `npm ci` the
 * way a lost network does, in about a second and with no network at all.
 *
 *   1. 🔴 a failed `npm ci` leaves the old install as it was (version, files, node_modules), the
 *      terminal wrapper as it was, and nothing staged beside it;
 *   2. an install that passes replaces the old one whole and leaves nothing beside it.
 *
 * The Windows installer has the same two cases in prova-instaladores-ps1.mjs.
 */
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const MODULOS = ["claude-runner.mjs", "politica.mjs", "parada.mjs", "orientacao.mjs", "uso.mjs"];

/** The runner's package.json and lock at `versao`; with `dependencia`, one package npm must fetch. */
function manifestos(versao, dependencia) {
  const deps = dependencia ? { "left-pad": "1.3.0" } : undefined;
  const pkg = { name: "shvia-claude-runner", version: versao, private: true, type: "module", ...(deps && { dependencies: deps }) };
  const lock = {
    name: "shvia-claude-runner", version: versao, lockfileVersion: 3, requires: true,
    packages: {
      "": { name: "shvia-claude-runner", version: versao, ...(deps && { dependencies: deps }) },
      ...(deps && {
        "node_modules/left-pad": {
          version: "1.3.0",
          resolved: "https://registry.npmjs.org/left-pad/-/left-pad-1.3.0.tgz",
          integrity: "sha512-XI5MPzVNApjAyhQzphX8BkmKsKUxD4LdyK24iZeQCvhHbBhVvB2uagvFg7hrIZRpEBqZW2XrzI9SKH6pIIqC4uw==",
        },
      }),
    },
  };
  return { pkg: JSON.stringify(pkg, null, 2), lock: JSON.stringify(lock, null, 2) };
}

/** A folder with the real install.sh and stand-in modules at `versao`. */
function fonte(base, versao, dependencia) {
  const dir = join(base, "fonte", "claude-runner");
  mkdirSync(dir, { recursive: true });
  copyFileSync(join(RAIZ, "claude-runner/install.sh"), join(dir, "install.sh"));
  for (const m of MODULOS) writeFileSync(join(dir, m), `export const versao = ${JSON.stringify(versao)};\n`);
  const { pkg, lock } = manifestos(versao, dependencia);
  writeFileSync(join(dir, "package.json"), pkg);
  writeFileSync(join(dir, "package-lock.json"), lock);
  return dir;
}

/** An install at 1.0.0 with a stand-in SDK, and its terminal wrapper, where install.sh leaves them. */
function instalacaoAntiga(base) {
  const dest = join(base, "data", "shvia-claude-runner");
  mkdirSync(join(dest, "node_modules", "sdk-antigo"), { recursive: true });
  for (const m of MODULOS) writeFileSync(join(dest, m), 'export const versao = "1.0.0";\n');
  writeFileSync(join(dest, "package.json"), manifestos("1.0.0", false).pkg);
  writeFileSync(join(dest, "node_modules", "sdk-antigo", "package.json"), "{}\n");
  const bin = join(base, "home", ".local", "bin");
  mkdirSync(bin, { recursive: true });
  writeFileSync(join(bin, "claude-runner"), "#!/bin/sh\n# the old wrapper\n", { mode: 0o755 });
  return dest;
}

function instalar(base, dir) {
  const cache = join(base, "npm-cache-vazio");
  mkdirSync(cache, { recursive: true });
  const r = spawnSync("bash", [join(dir, "install.sh")], {
    encoding: "utf8",
    timeout: 120_000,
    env: {
      ...process.env,
      HOME: join(base, "home"),
      XDG_DATA_HOME: join(base, "data"),
      npm_config_offline: "true",
      npm_config_cache: cache,
    },
  });
  return { code: r.status, out: `${r.stdout ?? ""}${r.stderr ?? ""}` };
}

const ler = (p) => (existsSync(p) ? readFileSync(p, "utf8") : "");
const versaoEm = (dest) => { try { return JSON.parse(ler(join(dest, "package.json"))).version; } catch { return null; } };
const falhas = [];
const caso = (nome, ok, detalhe) => { if (!ok) falhas.push(`${nome}: ${detalhe}`); };

const raiz = mkdtempSync(join(tmpdir(), "shvia-runner-troca-"));
try {
  // 1. 🔴 The update's npm ci fails: the old runner stays, whole.
  {
    const base = join(raiz, "falha");
    const dest = instalacaoAntiga(base);
    const r = instalar(base, fonte(base, "2.0.0", true));
    const sobra = readdirSync(join(base, "data")).filter((n) => n !== "shvia-claude-runner");
    caso("a failed npm ci keeps the old runner",
      r.code !== 0
        && versaoEm(dest) === "1.0.0"
        && ler(join(dest, "claude-runner.mjs")).includes('"1.0.0"')
        && existsSync(join(dest, "node_modules", "sdk-antigo", "package.json"))
        && ler(join(base, "home", ".local", "bin", "claude-runner")).includes("the old wrapper")
        && sobra.length === 0,
      `exit ${r.code}, installed version ${versaoEm(dest)}, old SDK ${existsSync(join(dest, "node_modules", "sdk-antigo")) ? "kept" : "GONE"}, left beside it: ${JSON.stringify(sobra)}\n${r.out.slice(-600)}`);
  }

  // 2. The update passes: the new runner replaces the old one whole, and nothing is left beside it.
  {
    const base = join(raiz, "passa");
    const dest = instalacaoAntiga(base);
    const r = instalar(base, fonte(base, "2.0.0", false));
    const sobra = readdirSync(join(base, "data")).filter((n) => n !== "shvia-claude-runner");
    caso("an install that passes replaces the old runner",
      r.code === 0 && /✓ claude-runner instalado/.test(r.out)
        && versaoEm(dest) === "2.0.0"
        && ler(join(dest, "claude-runner.mjs")).includes('"2.0.0"')
        && !existsSync(join(dest, "node_modules", "sdk-antigo"))
        && ler(join(base, "home", ".local", "bin", "claude-runner")).includes(`${dest}/claude-runner.mjs`)
        && sobra.length === 0,
      `exit ${r.code}, installed version ${versaoEm(dest)}, left beside it: ${JSON.stringify(sobra)}\n${r.out.slice(-600)}`);
  }
} finally {
  rmSync(raiz, { recursive: true, force: true });
}

if (falhas.length) {
  for (const f of falhas) console.error(`🔴 ${f}`);
  console.error(`\n${falhas.length} problem(s): a runner update that fails can leave Code mode broken while the app counts it as up to date.`);
  process.exit(1);
}
console.log("[runner-troca] a failed npm ci keeps the old runner whole · an install that passes replaces it whole · nothing left beside it");
