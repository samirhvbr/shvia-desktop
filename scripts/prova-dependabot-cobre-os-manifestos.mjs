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
if (falhas.length) {
  for (const f of falhas) console.error(`🔴 ${f}`);
  console.error(`\n${falhas.length} problem(s): Dependabot would skip a manifest, or fail on a stale entry.`);
  process.exit(1);
}
console.log(`[dependabot] ${vigiados.size} entries · every tracked manifest and the workflows are watched`);
