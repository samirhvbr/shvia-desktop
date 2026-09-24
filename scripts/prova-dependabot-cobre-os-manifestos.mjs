#!/usr/bin/env node
/**
 * Every manifest in the repository is watched by Dependabot (1.6.35).
 *
 * The owner chose "complete": every ecosystem. What makes that true over time is not the file
 * written today but a check that fails the day a new directory with a package.json or a
 * Cargo.toml appears without an entry, since Dependabot says nothing about a manifest it was
 * never told about. It also fails on an entry whose directory lost its manifest, which Dependabot
 * reports only on its own page, where nobody looks.
 *
 * A source ruler: it reads .github/dependabot.yml as text (no YAML parser in the standard
 * library), one `- package-ecosystem:` item at a time.
 */
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const CONFIG = join(RAIZ, ".github/dependabot.yml");
if (!existsSync(CONFIG)) { console.error("🔴 .github/dependabot.yml does not exist."); process.exit(1); }

// What the config watches: "ecosystem dir" pairs.
const vigiados = new Set();
const itens = readFileSync(CONFIG, "utf8").split(/^\s*-\s+package-ecosystem:\s*/m).slice(1);
for (const item of itens) {
  const eco = item.match(/^["']?([\w-]+)/)?.[1];
  const dir = item.match(/^\s*directory:\s*["']?([^"'\s#]+)/m)?.[1];
  if (!eco || !dir) { console.error(`🔴 unreadable item in dependabot.yml: ${item.slice(0, 60)}`); process.exit(1); }
  vigiados.add(`${eco} ${dir.replace(/\/+$/, "") || "/"}`);
}

// What the repository has: tracked manifests, plus workflows for github-actions.
const MANIFESTO = { "package.json": "npm", "Cargo.toml": "cargo" };
const exigidos = new Map();
for (const f of execFileSync("git", ["ls-files"], { cwd: RAIZ, encoding: "utf8" }).split("\n")) {
  const nome = f.split("/").pop();
  if (MANIFESTO[nome]) {
    const d = dirname(f);
    exigidos.set(`${MANIFESTO[nome]} ${d === "." ? "/" : `/${d}`}`, f);
  }
  if (/^\.github\/workflows\/[^/]+\.ya?ml$/.test(f)) exigidos.set("github-actions /", f);
}

const falhas = [];
for (const [par, f] of exigidos) {
  if (!vigiados.has(par)) falhas.push(`${f} is not watched: add "package-ecosystem: ${par.split(" ")[0]}" with "directory: ${par.split(" ")[1]}"`);
}
for (const par of vigiados) {
  if (!exigidos.has(par)) falhas.push(`the entry "${par}" points at a directory with no tracked manifest for that ecosystem`);
}
// 1.6.49: the per-OS dependency sections of src-tauri/Cargo.toml hold the WebView crates that
// are matched to wry's versions; each one must be in the cargo entry's `ignore` list, or a
// Dependabot proposal would move one alone and break that platform's build (#51, #52).
const cargoToml = readFileSync(join(RAIZ, "src-tauri/Cargo.toml"), "utf8");
const casados = [];
for (const sec of cargoToml.split(/^\[/m)) {
  if (!/^target\.'cfg\(target_os = "(linux|macos|windows)"\)'\.dependencies\]/.test(sec)) continue;
  for (const linha of sec.split("\n").slice(1)) {
    const m = linha.match(/^([A-Za-z0-9_-]+)\s*=/);
    if (m) casados.push(m[1]);
  }
}
const itemCargo = itens.find((it) => /^["']?cargo\b/.test(it)) ?? "";
const ignorados = new Set([...itemCargo.matchAll(/-\s*dependency-name:\s*["']?([A-Za-z0-9_-]+)/g)].map((m) => m[1]));
if (casados.length === 0) falhas.push("found no per-OS dependency section in src-tauri/Cargo.toml: the matched-crate check measures nothing");
for (const c of casados) {
  if (!ignorados.has(c)) falhas.push(`${c} is matched to wry (a per-OS section of Cargo.toml) but Dependabot does not ignore it`);
}

if (falhas.length) {
  for (const f of falhas) console.error(`🔴 ${f}`);
  console.error(`\n${falhas.length} problem(s): Dependabot would skip a manifest, fail on a stale entry, or move a wry-matched crate alone.`);
  process.exit(1);
}
console.log(`[dependabot] ${vigiados.size} entries · every tracked manifest and the workflows are watched · ${casados.length} crates matched to wry, all ignored`);
