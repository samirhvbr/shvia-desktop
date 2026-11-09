#!/usr/bin/env node
/**
 * The Windows installers of the two runners (1.6.56) install, and refuse an incomplete install.
 *
 * `claude-runner/install.ps1` and `codex-runner/install.ps1` run here under PowerShell (`pwsh`,
 * which GitHub's ubuntu runners have), against a temporary LOCALAPPDATA whose path has a space
 * and an accent. What this measures is the scripts' logic; the Windows machine itself (cmd.exe,
 * the .cmd wrapper, WebView2) is the owner's runbook. Without pwsh: NOT MEASURED locally (exit
 * 0), exit 1 in CI — a check that quietly skips where it matters is not a check.
 *
 * Eight cases:
 *   1. claude-runner installs: the five files, the Agent SDK from the lock, the terminal .cmd
 *      (CRLF, path through %LOCALAPPDATA%), and the load proof passes;
 *   2. 🔴 a local import the installer does not copy fails the install (the 1.4.7 class) —
 *      the load proof is what turns it red, so this is that proof's reversal;
 *   3. codex-runner installs: its files, politica.mjs next door, a schema, --version answers;
 *   4. 🔴 the same missing-import case for codex-runner;
 *   5. without LOCALAPPDATA the installer stops and says so;
 *   6. a broken link named `node`/`npm` AHEAD in PATH (measured on 24/09/2026: a dangling
 *      ~/.local/bin/npm made pwsh try to "open" it) does not break the install;
 *   7. 🔴 an update whose `npm ci` fails (offline, empty cache: a lost network) leaves the installed
 *      claude-runner as it was, version and node_modules, with nothing staged beside it (1.38.13).
 *      Until 1.38.13 the new files were copied over the old install first, `npm ci` deleted its
 *      node_modules, and the app counted the broken runner as up to date;
 *   8. an update that passes replaces the old install whole and leaves nothing beside it.
 *
 * install.sh has cases 7 and 8 in prova-runner-que-falha-mantem-o-anterior.mjs.
 */
import { execFileSync, spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");

function acharPwsh() {
  for (const c of [process.env.PWSH, "pwsh"].filter(Boolean)) {
    try { execFileSync(c, ["-NoProfile", "-NoLogo", "-Command", "1"], { stdio: "ignore" }); return c; } catch {}
  }
  return null;
}
const PWSH = acharPwsh();
if (!PWSH) {
  if (process.env.CI) { console.error("🔴 pwsh not found in CI: the Windows installers' proof cannot run"); process.exit(1); }
  console.log("[instaladores-ps1] NOT MEASURED: no pwsh (set PWSH=/path/to/pwsh). CI runs it.");
  process.exit(0);
}

const base = mkdtempSync(join(tmpdir(), "shvia-ps1-inst-"));
const falhas = [];
const fonteLimpa = () => {
  // A copy of both runner folders, so a case can break its copy without touching the repo.
  const src = mkdtempSync(join(base, "src-"));
  for (const r of ["claude-runner", "codex-runner"]) {
    cpSync(join(RAIZ, r), join(src, r), { recursive: true, filter: (p) => !p.includes("node_modules") });
  }
  return src;
};
function instalar(src, runner, { semLocalAppData = false, pathExtra = null, antes = null, envExtra = {} } = {}) {
  const lad = mkdtempSync(join(base, "lad com espaço-"));
  if (antes) antes(lad);
  const env = { ...process.env, LOCALAPPDATA: semLocalAppData ? "" : lad, ...envExtra };
  if (pathExtra) env.PATH = pathExtra + delimiter + env.PATH;
  const r = spawnSync(PWSH, ["-NoProfile", "-NonInteractive", "-File", join(src, runner, "install.ps1")], {
    env, encoding: "utf8", timeout: 240_000,
  });
  return { code: r.status, out: `${r.stdout ?? ""}${r.stderr ?? ""}`, lad };
}
const tem = (p) => existsSync(p);
function caso(nome, ok, detalhe) { if (!ok) falhas.push(`${nome}: ${detalhe}`); }

// Cases 7 and 8: an install of claude-runner 1.0.0 already in place, with a stand-in SDK, and a source at
// 2.0.0 whose lock either needs a package npm must fetch (offline with an empty cache, `npm ci` fails the
// way a lost network makes it fail) or needs nothing (it passes, with no network).
const MODULOS = ["claude-runner.mjs", "politica.mjs", "parada.mjs", "orientacao.mjs", "uso.mjs"];
function manifestos(versao, dependencia) {
  const deps = dependencia ? { "left-pad": "1.3.0" } : undefined;
  const raiz = { name: "shvia-claude-runner", version: versao, ...(deps && { dependencies: deps }) };
  const lock = {
    name: "shvia-claude-runner", version: versao, lockfileVersion: 3, requires: true,
    packages: {
      "": raiz,
      ...(deps && { "node_modules/left-pad": {
        version: "1.3.0",
        resolved: "https://registry.npmjs.org/left-pad/-/left-pad-1.3.0.tgz",
        integrity: "sha512-XI5MPzVNApjAyhQzphX8BkmKsKUxD4LdyK24iZeQCvhHbBhVvB2uagvFg7hrIZRpEBqZW2XrzI9SKH6pIIqC4uw==",
      } }),
    },
  };
  return { pkg: JSON.stringify({ ...raiz, private: true, type: "module" }), lock: JSON.stringify(lock) };
}
function fonteEm(versao, dependencia) {
  const src = fonteLimpa();
  const dir = join(src, "claude-runner");
  for (const m of MODULOS) writeFileSync(join(dir, m), `export const versao = ${JSON.stringify(versao)};\n`);
  const { pkg, lock } = manifestos(versao, dependencia);
  writeFileSync(join(dir, "package.json"), pkg);
  writeFileSync(join(dir, "package-lock.json"), lock);
  return src;
}
function instalacaoAntiga(lad) {
  const dest = join(lad, "shvia-claude-runner");
  mkdirSync(join(dest, "node_modules", "sdk-antigo"), { recursive: true });
  for (const m of MODULOS) writeFileSync(join(dest, m), 'export const versao = "1.0.0";\n');
  writeFileSync(join(dest, "package.json"), manifestos("1.0.0", false).pkg);
  writeFileSync(join(dest, "node_modules", "sdk-antigo", "package.json"), "{}\n");
}
const versaoEm = (dest) => { try { return JSON.parse(readFileSync(join(dest, "package.json"), "utf8")).version; } catch { return null; } };
const aoLado = (lad) => readdirSync(lad).filter((n) => n.startsWith("shvia-claude-runner."));
const ler = (p) => (tem(p) ? readFileSync(p, "utf8") : "");

try {
  // 1. claude-runner installs.
  const src = fonteLimpa();
  const c1 = instalar(src, "claude-runner");
  const d1 = join(c1.lad, "shvia-claude-runner");
  const cmd1 = join(c1.lad, "shvia", "bin", "claude-runner.cmd");
  const wrap = tem(cmd1) ? readFileSync(cmd1, "latin1") : "";
  caso("claude-runner installs", c1.code === 0 && /✓ claude-runner instalado/.test(c1.out)
    && ["claude-runner.mjs", "politica.mjs", "parada.mjs", "orientacao.mjs", "package.json", "package-lock.json"].every((f) => tem(join(d1, f)))
    && tem(join(d1, "node_modules", "@anthropic-ai", "claude-agent-sdk", "package.json"))
    && wrap.includes("\r\n") && wrap.includes("%LOCALAPPDATA%\\shvia-claude-runner\\claude-runner.mjs"),
    `exit ${c1.code}\n${c1.out.slice(-800)}`);

  // 2. 🔴 an import the installer does not copy.
  const src2 = fonteLimpa();
  const mjs2 = join(src2, "claude-runner", "claude-runner.mjs");
  writeFileSync(join(src2, "claude-runner", "extra-nao-copiado.mjs"), "export const x = 1;\n");
  writeFileSync(mjs2, 'import "./extra-nao-copiado.mjs";\n' + readFileSync(mjs2, "utf8"));
  const c2 = instalar(src2, "claude-runner");
  caso("claude-runner with an uncopied import is refused", c2.code === 1 && /não carrega/.test(c2.out) && !/✓/.test(c2.out),
    `exit ${c2.code}\n${c2.out.slice(-600)}`);

  // 3. codex-runner installs.
  const c3 = instalar(src, "codex-runner");
  const d3 = join(c3.lad, "shvia-codex-runner");
  caso("codex-runner installs", c3.code === 0 && /✓ codex-runner \S+ instalado/.test(c3.out)
    && ["codex-runner.mjs", "protocolo.mjs", "esquema.mjs", "plataforma.mjs", "package.json"].every((f) => tem(join(d3, f)))
    && tem(join(c3.lad, "claude-runner", "politica.mjs"))
    && tem(join(d3, "schemas", "codex_app_server_protocol.v2.schemas.json"))
    && tem(join(c3.lad, "shvia", "bin", "codex-runner.cmd")),
    `exit ${c3.code}\n${c3.out.slice(-800)}`);

  // 4. 🔴 the same for codex-runner.
  const src4 = fonteLimpa();
  const mjs4 = join(src4, "codex-runner", "codex-runner.mjs");
  writeFileSync(join(src4, "codex-runner", "extra-nao-copiado.mjs"), "export const x = 1;\n");
  writeFileSync(mjs4, 'import "./extra-nao-copiado.mjs";\n' + readFileSync(mjs4, "utf8"));
  const c4 = instalar(src4, "codex-runner");
  caso("codex-runner with an uncopied import is refused", c4.code === 1 && /não responde --version/.test(c4.out) && !/✓/.test(c4.out),
    `exit ${c4.code}\n${c4.out.slice(-600)}`);

  // 5. No LOCALAPPDATA.
  const c5 = instalar(src, "codex-runner", { semLocalAppData: true });
  caso("without LOCALAPPDATA it stops", c5.code === 1 && /LOCALAPPDATA não está definido/.test(c5.out),
    `exit ${c5.code}\n${c5.out.slice(-400)}`);

  // 6. Broken links ahead in PATH (Unix only: that is where the case was measured).
  if (process.platform !== "win32") {
    const lixo = mkdtempSync(join(base, "path-quebrado-"));
    symlinkSync(join(lixo, "sumiu-node"), join(lixo, "node"));
    symlinkSync(join(lixo, "sumiu-npm"), join(lixo, "npm"));
    const c6 = instalar(src, "claude-runner", { pathExtra: lixo });
    caso("a broken node/npm link ahead in PATH does not break the install", c6.code === 0 && /✓ claude-runner instalado/.test(c6.out),
      `exit ${c6.code}\n${c6.out.slice(-600)}`);
  }

  // 7. 🔴 The update's npm ci fails: the installed runner stays as it was.
  const cache7 = mkdtempSync(join(base, "npm-cache-vazio-"));
  const c7 = instalar(fonteEm("2.0.0", true), "claude-runner", {
    antes: instalacaoAntiga, envExtra: { npm_config_offline: "true", npm_config_cache: cache7 },
  });
  const d7 = join(c7.lad, "shvia-claude-runner");
  caso("a failed npm ci keeps the installed claude-runner", c7.code === 1 && /npm ci falhou/.test(c7.out)
    && versaoEm(d7) === "1.0.0" && tem(join(d7, "node_modules", "sdk-antigo", "package.json"))
    && ler(join(d7, "claude-runner.mjs")).includes('"1.0.0"') && aoLado(c7.lad).length === 0,
    `exit ${c7.code}, installed version ${versaoEm(d7)}, left beside it: ${JSON.stringify(aoLado(c7.lad))}\n${c7.out.slice(-600)}`);

  // 8. The update passes: the new runner replaces the old one whole.
  const cache8 = mkdtempSync(join(base, "npm-cache-vazio-"));
  const c8 = instalar(fonteEm("2.0.0", false), "claude-runner", {
    antes: instalacaoAntiga, envExtra: { npm_config_offline: "true", npm_config_cache: cache8 },
  });
  const d8 = join(c8.lad, "shvia-claude-runner");
  caso("an update that passes replaces the installed claude-runner", c8.code === 0 && /✓ claude-runner instalado/.test(c8.out)
    && versaoEm(d8) === "2.0.0" && !tem(join(d8, "node_modules", "sdk-antigo"))
    && ler(join(d8, "claude-runner.mjs")).includes('"2.0.0"') && aoLado(c8.lad).length === 0,
    `exit ${c8.code}, installed version ${versaoEm(d8)}, left beside it: ${JSON.stringify(aoLado(c8.lad))}\n${c8.out.slice(-600)}`);
} finally {
  rmSync(base, { recursive: true, force: true });
}

if (falhas.length) {
  for (const f of falhas) console.error(`🔴 ${f}`);
  console.error(`\n${falhas.length} problem(s): a Windows install of the runners can end broken, or say ✓ when it is.`);
  process.exit(1);
}
console.log(`[instaladores-ps1] claude-runner and codex-runner install under pwsh · an uncopied import is refused in both · a broken PATH link is skipped · a failed update keeps the installed claude-runner, a passing one replaces it whole (pwsh ${PWSH})`);
