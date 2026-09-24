#!/usr/bin/env node
/**
 * The pre-commit hook refuses a half-bumped commit (1.6.33).
 *
 * The version went out half-bumped three times (1.1.19, 1.4.21, 1.6.3): version.md committed, a
 * carrier not. In 1.6.3 the carriers were bumped on disk and left out of the INDEX, so the check
 * of the working tree passed. This proof makes REAL commits in a temp repository carrying the
 * real carriers, `scripts/sync-version.mjs` and `tools/git-hooks/pre-commit`.
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const ARQUIVOS = [
  "version.md", "package.json", "package-lock.json", "claude-runner/package.json",
  "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/tauri.conf.json",
  "scripts/sync-version.mjs", "tools/git-hooks/pre-commit",
];
const PORTADORES = ARQUIVOS.slice(1, 7);

function repo() {
  const dir = mkdtempSync(join(tmpdir(), "shvia-guarda-"));
  for (const f of ARQUIVOS) {
    mkdirSync(dirname(join(dir, f)), { recursive: true });
    copyFileSync(join(RAIZ, f), join(dir, f));
  }
  const git = (...a) => execFileSync("git", a, { cwd: dir, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  git("init", "-q");
  git("config", "user.email", "t@t.tld");
  git("config", "user.name", "t");
  git("config", "core.hooksPath", "tools/git-hooks");
  git("add", "-A");
  git("-c", "core.hooksPath=/dev/null", "commit", "-qm", "base");
  return { dir, git };
}
function bump(dir) {
  const v = readFileSync(join(dir, "version.md"), "utf8").trim().split(".").map(Number);
  writeFileSync(join(dir, "version.md"), `${v[0]}.${v[1]}.${v[2] + 1}\n`);
  execFileSync(process.execPath, ["scripts/sync-version.mjs"], { cwd: dir, stdio: "ignore" });
}
function commit(r, env = {}) {
  try {
    execFileSync("git", ["commit", "-qm", "x"], { cwd: r.dir, stdio: "ignore", env: { ...process.env, ...env } });
    return 0;
  } catch (e) { return e.status ?? 1; }
}

let falhas = 0;
const confere = (nome, obtido, esperado) => {
  if ((obtido === 0) !== (esperado === 0)) { falhas++; console.error(`🔴 ${nome}: esperado ${esperado === 0 ? "aceito" : "RECUSADO"}, obtido status ${obtido}`); }
};

{ // 1.6.3, exactly: version.md staged, carriers bumped on disk and NOT staged
  const r = repo(); bump(r.dir); r.git("add", "version.md");
  confere("version.md no índice, portadores só na árvore (a 1.6.3)", commit(r), 1);
  confere("…o mesmo com REPODOCS_NO_HOOK=1", commit(r, { REPODOCS_NO_HOOK: "1" }), 0);
  rmSync(r.dir, { recursive: true, force: true });
}
{ const r = repo(); bump(r.dir); r.git("add", "version.md", ...PORTADORES);
  confere("tudo no índice", commit(r), 0); rmSync(r.dir, { recursive: true, force: true }); }
{ const r = repo(); writeFileSync(join(r.dir, "nota.txt"), "x"); r.git("add", "nota.txt");
  confere("commit sem versão nem portador", commit(r), 0); rmSync(r.dir, { recursive: true, force: true }); }
{ const r = repo(); bump(r.dir); r.git("add", "package.json");
  confere("portador novo no índice sem o version.md", commit(r), 1); rmSync(r.dir, { recursive: true, force: true }); }

if (falhas) { console.error(`\n${falhas} caso(s) falharam: um commit pela metade voltaria a passar.`); process.exit(1); }
console.log("[guarda-dos-portadores] 5 casos · o índice decide, e o commit pela metade é recusado");
