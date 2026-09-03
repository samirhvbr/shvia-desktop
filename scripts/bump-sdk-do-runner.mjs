#!/usr/bin/env node
/**
 * Refreshes the Claude Agent SDK pinned by `claude-runner`, and shows what changed in the
 * MODEL CATALOGUE as a result.
 *
 * ## Why this exists (finding F-18, September 2026 review)
 *
 * The Modo Code catalogue comes from the SDK (`--modelos` → `supportedModels()`), so the SDK
 * version *is* the catalogue. Until 02/09 `install.sh` deleted the lock before installing,
 * precisely so the catalogue would not freeze — a decision taken on 21/08 after the selector
 * kept offering "Opus" = Opus 4.8 weeks after Opus 5 shipped.
 *
 * It did not work. Measured on 02/09: this machine ran SDK **0.3.239** while a fresh resolve
 * of the same range gave **0.3.258**. Nineteen releases apart. Deleting the lock did not make
 * the SDK fresh; it made the installed version depend on *when you last reinstalled*, which
 * is the same staleness wearing a different hat — and unreviewable on top of it, in a
 * component that executes tools on the developer's machine.
 *
 * So the lock is versioned and `install.sh` uses `npm ci`, and refreshing becomes this: one
 * command, in the repository, visible in a diff.
 *
 * Usage:
 *   npm run runner:sdk-bump              # latest inside the current major
 *   npm run runner:sdk-bump -- 0.3.258   # a specific version
 */
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const RUNNER = join(ROOT, 'claude-runner');
const PKG = '@anthropic-ai/claude-agent-sdk';
const alvo = process.argv[2];

const npm = (args) =>
  execFileSync('npm', args, { cwd: RUNNER, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] });

const travado = () => {
  const lock = JSON.parse(readFileSync(join(RUNNER, 'package-lock.json'), 'utf8'));
  return lock.packages?.[`node_modules/${PKG}`]?.version ?? '?';
};

const antes = travado();
console.log(`SDK travado hoje: ${antes}`);

npm(['install', '--package-lock-only', '--omit=dev', '--no-audit', '--no-fund',
     alvo ? `${PKG}@${alvo}` : `${PKG}@latest`]);

const depois = travado();
if (antes === depois) {
  console.log(`Nada a fazer: já está em ${depois}.`);
  process.exit(0);
}
console.log(`SDK: ${antes} → ${depois}`);

// ── The point of the whole exercise: what did the catalogue become? ───────
//
// Printing the model list is not decoration. The reason the lock was deleted in the first
// place was a catalogue nobody could see going stale; a bump that does not show its effect
// would leave that problem exactly where it was.
console.log('\nCatálogo de modelos que este SDK oferece:');
try {
  npm(['ci', '--omit=dev', '--no-audit', '--no-fund']);
  const saida = execFileSync('node', [join(RUNNER, 'claude-runner.mjs'), '--modelos'],
    { encoding: 'utf8', timeout: 60_000 });
  const modelos = JSON.parse(saida);
  for (const m of (Array.isArray(modelos) ? modelos : modelos.modelos ?? [])) {
    console.log(`  · ${m.value ?? m.id ?? '?'}${m.displayName ? ` — ${m.displayName}` : ''}`);
  }
} catch (e) {
  // Never fatal: the bump itself succeeded, and the lock diff is the deliverable. Failing
  // here would make the command look broken when only the preview did not run.
  console.log(`  (não consegui listar: ${String(e.message).split('\n')[0]})`);
  console.log('  Rode `node claude-runner/claude-runner.mjs --modelos` depois de instalar.');
}
console.log('\nO diff do package-lock.json é a revisão. Commite com o motivo do bump.');
