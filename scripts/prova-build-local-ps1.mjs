#!/usr/bin/env node
/**
 * build-local.ps1 decides, before `tauri build`, what the bundler would only say at the end
 * (1.6.50): no anna staged → `externalBin: []`; no updater key → stop in the first second, or
 * with -NoSign build without updater artifacts. A TAURI_CONFIG the person set is left alone.
 *
 * Runs with PowerShell (`pwsh`), which GitHub's ubuntu runners have. Locally, set PWSH to a pwsh
 * binary if it is not on PATH. Without one, this prints NOT MEASURED and exits 0 locally, but
 * exits 1 in CI (CI=true): a check that quietly skips where it matters is not a check.
 *
 * Four measurements:
 *   1. the whole script parses (the Windows build never runs in CI);
 *   2. no function is called at script level before its definition — PowerShell runs top to
 *      bottom, the parser does not catch it, and 1.6.50's first draft had exactly that;
 *   3. the functions, cut out of the script and run against temp dirs;
 *   4. the override reaches `tauri build` as `--config <file>` (1.7.2). Measuring only the JSON
 *      let 1.6.50 pass here while the real Windows run failed: the script put it in
 *      $env:TAURI_CONFIG, which the Rust build reads and the CLI's bundler does not. The path is
 *      literal text and no `npx` line holds a `$` (1.7.3): npm's npx.ps1 re-runs the caller's
 *      statement text in its own scope, where the script's variables are not set.
 */
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const SCRIPT = join(RAIZ, "build-local.ps1");

function acharPwsh() {
  for (const c of [process.env.PWSH, "pwsh"].filter(Boolean)) {
    try { execFileSync(c, ["-NoProfile", "-NoLogo", "-Command", "1"], { stdio: "ignore" }); return c; } catch {}
  }
  return null;
}
const PWSH = acharPwsh();
if (!PWSH) {
  if (process.env.CI) { console.error("🔴 pwsh not found in CI: the build-local.ps1 proof cannot run"); process.exit(1); }
  console.log("[build-local.ps1] NOT MEASURED: no pwsh (set PWSH=/path/to/pwsh). CI runs it.");
  process.exit(0);
}
const ps = (cmd, env = {}) => execFileSync(PWSH, ["-NoProfile", "-NoLogo", "-NonInteractive", "-Command", cmd], {
  encoding: "utf8", env: { ...process.env, ...env }, stdio: ["ignore", "pipe", "pipe"],
});

const falhas = [];

// 1 + 2: parse, and definition-before-use at script level.
const estrutura = ps(`
$e = $null; $ast = [System.Management.Automation.Language.Parser]::ParseFile('${SCRIPT}', [ref]$null, [ref]$e)
"PARSE $($e.Count)"
$defs = @{}
$ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $false) |
  ForEach-Object { $defs[$_.Name] = $_.Extent.StartLineNumber }
$ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.CommandAst] }, $true) | ForEach-Object {
  $nome = $_.GetCommandName()
  if (-not $nome -or -not $defs.ContainsKey($nome)) { return }
  $p = $_.Parent; $dentro = $false
  while ($p) { if ($p -is [System.Management.Automation.Language.FunctionDefinitionAst]) { $dentro = $true; break }; $p = $p.Parent }
  if (-not $dentro -and $_.Extent.StartLineNumber -lt $defs[$nome]) { "EARLY $nome $($_.Extent.StartLineNumber) $($defs[$nome])" }
}`);
const parse = Number(estrutura.match(/PARSE (\d+)/)?.[1] ?? -1);
if (parse !== 0) falhas.push(`build-local.ps1 does not parse (${parse} error(s))`);
for (const m of estrutura.matchAll(/EARLY (\S+) (\d+) (\d+)/g)) {
  falhas.push(`${m[1]} is called at line ${m[2]} but defined at line ${m[3]}: PowerShell runs top to bottom`);
}

// 3: the two functions, cut out of the script.
const fonte = readFileSync(SCRIPT, "utf8").replace(/^﻿/, "");
function funcao(nome) {
  const i = fonte.indexOf(`function ${nome} {`);
  if (i < 0) { falhas.push(`${nome} moved out of build-local.ps1`); return ""; }
  return fonte.slice(i, fonte.indexOf("\n}\n", i) + 3);
}
const FUNCS = ["Resolve-UpdaterKey", "Get-TauriConfigOverride", "Write-TauriConfigFile"].map(funcao).join("\n");

const TRIPLE = "x86_64-pc-windows-msvc";
function override({ sidecar, key, noSign, updater = true }) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-ps1-"));
  try {
    mkdirSync(join(dir, "src-tauri", "binaries"), { recursive: true });
    if (sidecar) writeFileSync(join(dir, "src-tauri", "binaries", `anna-${TRIPLE}.exe`), "");
    const out = ps(`${FUNCS}
try { $r = Get-TauriConfigOverride -Root '${dir}' -Triple '${TRIPLE}' -HasKey $${key} -NoSign $${noSign} -UpdaterArtifacts $${updater}
  if ($null -eq $r) { 'RESULT NULL' } else { "RESULT $r" } } catch { "THROW $($_.Exception.Message)" }`);
    return out.trim().split("\n").pop();
  } finally { rmSync(dir, { recursive: true, force: true }); }
}
function chave({ comArquivos }) {
  const home = mkdtempSync(join(tmpdir(), "shvia-ps1-home-"));
  try {
    if (comArquivos) {
      mkdirSync(join(home, ".shvia"));
      writeFileSync(join(home, ".shvia", "updater.key"), "dW50cnVzdGVkIGtleQ==\n");
      writeFileSync(join(home, ".shvia", "updater.pass"), "s3nha com espaço \nsegunda linha\n");
    }
    const out = ps(`${FUNCS}
$r = Resolve-UpdaterKey -HomeDir '${home}'
"RESULT $r [$env:TAURI_SIGNING_PRIVATE_KEY] [$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD]"`,
      { TAURI_SIGNING_PRIVATE_KEY: "", TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "" });
    return out.trim().split("\n").pop();
  } finally { rmSync(home, { recursive: true, force: true }); }
}

const ARQ_CONFIG = "src-tauri/target/build-local.tauri-config.json";
function arquivoDeConfig() {
  const dir = mkdtempSync(join(tmpdir(), "shvia-ps1-cfg-"));
  const json = '{"bundle":{"externalBin":[]}}';
  try {
    const out = ps(`${FUNCS}
$f = Write-TauriConfigFile -Root '${dir}' -Json '${json}'
"RESULT $f"`);
    const f = out.trim().split("\n").pop().replace(/^RESULT /, "");
    if (f !== join(dir, ...ARQ_CONFIG.split("/"))) return `written to ${f}, not ${ARQ_CONFIG}`;
    const bytes = readFileSync(f);
    return bytes.equals(Buffer.from(json)) ? "RESULT exact, no BOM" : `RESULT bytes ${bytes.toString("hex")}`;
  } finally { rmSync(dir, { recursive: true, force: true }); }
}
// The call itself, read from the source: the bundler only sees `--config`, and npx.ps1 only sees text.
function chamadaDoBuild() {
  if (/\$env:TAURI_CONFIG\s*=/.test(fonte)) return "the script assigns $env:TAURI_CONFIG";
  if (!fonte.includes(`npx tauri build --config ${ARQ_CONFIG} }`)) return `tauri build has no --config ${ARQ_CONFIG}`;
  const comVariavel = fonte.split("\n").filter((l) => !/^\s*#/.test(l) && /\bnpx\b.*\$/.test(l));
  if (comVariavel.length) return `npx line with a variable: ${comVariavel[0].trim()}`;
  return "RESULT --config";
}

const CASOS = [
  ["sidecar and key present: no override", () => override({ sidecar: true, key: true, noSign: false }), "RESULT NULL"],
  ["🔴 no anna staged: externalBin is emptied", () => override({ sidecar: false, key: true, noSign: false }), 'RESULT {"bundle":{"externalBin":[]}}'],
  ["no key, -NoSign: no updater artifacts", () => override({ sidecar: true, key: false, noSign: true }), 'RESULT {"bundle":{"createUpdaterArtifacts":false}}'],
  ["no anna, no key, -NoSign: both", () => override({ sidecar: false, key: false, noSign: true }), 'RESULT {"bundle":{"externalBin":[],"createUpdaterArtifacts":false}}'],
  ["🔴 no key without -NoSign: refused before building", () => override({ sidecar: true, key: false, noSign: false }), /^THROW falta a chave do updater/],
  ["updater artifacts off in the config: no key needed", () => override({ sidecar: true, key: false, noSign: false, updater: false }), "RESULT NULL"],
  ["the key files are read like build-local.sh reads them", () => chave({ comArquivos: true }), "RESULT True [dW50cnVzdGVkIGtleQ==] [s3nha com espaço ]"],
  ["no key anywhere: false", () => chave({ comArquivos: false }), "RESULT False [] []"],
  ["the override file is the exact JSON, without a BOM", () => arquivoDeConfig(), "RESULT exact, no BOM"],
  ["🔴 the override reaches the bundler as --config, as literal text npx.ps1 can re-run", () => chamadaDoBuild(), "RESULT --config"],
];
for (const [nome, rodar, esperado] of CASOS) {
  const obtido = rodar();
  const ok = esperado instanceof RegExp ? esperado.test(obtido) : obtido === esperado;
  if (!ok) falhas.push(`${nome}: expected ${esperado}, got ${obtido}`);
}

if (falhas.length) {
  for (const f of falhas) console.error(`🔴 ${f}`);
  console.error(`\n${falhas.length} problem(s): the Windows build can fail at its end again, or not run at all.`);
  process.exit(1);
}
console.log(`[build-local.ps1] parses · functions defined before use · ${CASOS.length} cases (pwsh ${PWSH})`);
