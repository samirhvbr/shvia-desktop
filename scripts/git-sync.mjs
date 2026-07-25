#!/usr/bin/env node
// git-sync.mjs — sincroniza com o remoto SEM o "del Cargo.toml" manual.
//
// PROBLEMA (recorrente no Windows): os manifests que carregam versão
// (Cargo.toml, Cargo.lock, package.json, package-lock.json, tauri.conf.json)
// são reescritos pelo build (sync-version.mjs grava a versão; cargo/npm mexem
// nos locks). Se um build bumpa a versão e falha ANTES do commit, esses
// arquivos ficam "sujos" com a versão nova não-commitada. Aí um `git pull` que
// traz a mesma versão (vinda de outra máquina/CI) bate em "your local changes
// would be overwritten" — o que o usuário vê como conflito no Cargo.toml.
//
// SOLUÇÃO: antes do pull, restaurar ao HEAD os manifests cuja ÚNICA diferença é
// a linha de versão (é lixo regenerável — o build reaplica pelo version:sync).
// Se um manifest tem QUALQUER outra mudança (ex.: uma dependência nova que você
// adicionou no Cargo.toml e ainda não commitou), ele NÃO é tocado — restaurar
// apagaria trabalho real. Nesse caso o pull para e avisa, que é o certo.
//
// Por que Node (e não repetir a lógica no .ps1 e no .sh): uma implementação só,
// sem drift entre as duas cascas, e reaproveitável como `npm run pull` para o
// pull manual (o caso que os build scripts não cobriam).
//
// Uso:
//   node scripts/git-sync.mjs            # restaura o que for versão + git pull --ff-only
//   node scripts/git-sync.mjs --no-pull  # só a restauração (não puxa)
//   npm run pull                         # atalho (use no lugar do `git pull` manual)
//
// NUNCA derruba o build: erro de rede/divergência só avisa e sai 0.

import { execFileSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const NO_PULL = process.argv.includes("--no-pull");

// Manifests derivados de version.md — a restauração cirúrgica só age nestes.
const GENERATED = [
  "src-tauri/Cargo.toml",
  "src-tauri/Cargo.lock",
  "package.json",
  "package-lock.json",
  "src-tauri/tauri.conf.json",
];

// Uma linha de diff (+/-) que é SÓ uma atribuição de versão semântica.
// Cobre TOML  (version = "1.2.3")  e JSON  ("version": "1.2.3",).
const VERSION_LINE = /^[+-]\s*"?version"?\s*[:=]\s*"\d+\.\d+\.\d+",?\s*$/;

const say = (m) => console.log(`    ${m}`);
const warn = (m) => console.warn(`    ${m}`);

// git(): roda git e devolve stdout (''), ou null se o comando falhar. NUNCA
// lança — o chamador decide o que fazer com o null.
function git(args, { allowFail = true } = {}) {
  try {
    return execFileSync("git", args, {
      cwd: ROOT,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
  } catch (e) {
    if (allowFail) return null;
    throw e;
  }
}

function have(cmd) {
  try {
    execFileSync(cmd, ["--version"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

// Guardas: sem git / sem repo / sem origin → não há o que sincronizar.
if (!have("git")) {
  warn("(git não encontrado — pulando)");
  process.exit(0);
}
if (git(["rev-parse", "--is-inside-work-tree"]) === null) {
  warn("(não é um clone git — pulando)");
  process.exit(0);
}
if (git(["remote", "get-url", "origin"]) === null) {
  warn("(sem remote 'origin' — pulando)");
  process.exit(0);
}

// ── Restauração cirúrgica ────────────────────────────────────────────────────
// vs HEAD (pega staged E unstaged — o `git diff` sem HEAD perdia o staged, um
// dos furos da versão anterior). Restaura com `git checkout HEAD -- <arquivo>`
// (do HEAD, não do index — o outro furo: `git checkout -- <arquivo>` restaurava
// a versão suja se ela tivesse sido staged).
const restored = [];
const kept = [];
for (const file of GENERATED) {
  const diff = git(["diff", "HEAD", "--", file]);
  if (!diff) continue; // limpo (ou não existe em HEAD) — nada a fazer

  const changed = diff
    .split("\n")
    .filter((l) => /^[+-]/.test(l) && !/^[+-]{3}/.test(l)); // linhas +/- , fora o cabeçalho +++/---

  const onlyVersion = changed.length > 0 && changed.every((l) => VERSION_LINE.test(l));

  if (onlyVersion) {
    if (git(["checkout", "HEAD", "--", file]) !== null) restored.push(file);
  } else {
    kept.push(file); // tem mudança REAL (ex.: dep nova) — não mexer
  }
}

if (restored.length) {
  say("manifests com só a versão suja — restaurados ao HEAD (o build regera):");
  restored.forEach((f) => say(`  ${f}`));
}
if (kept.length) {
  warn("manifests com mudanças REAIS (não-versão) — preservados; o pull pode parar:");
  kept.forEach((f) => warn(`  ${f}`));
  warn("  → se são suas, commite; se são lixo, descarte com git checkout HEAD -- <arquivo>");
}

// ── Pull ─────────────────────────────────────────────────────────────────────
if (NO_PULL) {
  say("(--no-pull: só restauração, sem puxar)");
  process.exit(0);
}

const branch = (git(["rev-parse", "--abbrev-ref", "HEAD"]) || "?").trim();
say(`branch: ${branch} — git pull --ff-only`);

let out = "";
let ok = true;
try {
  out = execFileSync("git", ["pull", "--ff-only"], {
    cwd: ROOT,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
} catch (e) {
  ok = false;
  out = `${e.stdout || ""}${e.stderr || ""}`;
}
out
  .split("\n")
  .filter(Boolean)
  .forEach((l) => say(l));

if (!ok) {
  warn("[aviso] git pull não aplicou (offline, mudanças locais reais, ou branch divergente).");
  warn("        Seguindo com o código LOCAL atual.");
}
// Sempre 0: sincronizar é best-effort, nunca motivo pra abortar o build.
process.exit(0);
