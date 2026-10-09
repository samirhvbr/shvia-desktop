import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { validarPayload } from './esquema.mjs';

for (const failure of [false, true]) test(`catalogue RPC: ${failure ? 'failure is not an empty success' : 'pagination includes future models without starting a turn'}`, () => {
  const dir = mkdtempSync(join(tmpdir(), 'shvia-catalogue-'));
  try {
    const fake = join(dir, 'codex');
    writeFileSync(fake, `#!${process.execPath}
const rl = require('node:readline').createInterface({input: process.stdin});
rl.on('line', line => {
 const q = JSON.parse(line);
 let result;
 if (q.method === 'initialize') result = {};
 else if (q.method === 'model/list') {
   if (${failure}) { console.log(JSON.stringify({id:q.id,error:{message:'offline'}})); return; }
   result = {data:[{model:q.params.cursor ? 'gpt-future' : 'gpt-6-astra',displayName:'Model',isDefault:!q.params.cursor,supportedReasoningEfforts:[{reasoningEffort:'ultra'}]}],nextCursor:q.params.cursor ? null : 'page2'};
 } else { process.exit(42); }
 console.log(JSON.stringify({id:q.id,result}));
});
`, { mode: 0o755 });
    const run = spawnSync(process.execPath, [new URL('./codex-runner.mjs', import.meta.url).pathname, '--modelos'], {
      env: { ...process.env, SHVIA_CODEX_BIN: fake }, encoding: 'utf8', timeout: 5000,
    });
    assert.equal(run.error, undefined);
    const lines = run.stdout.trim().split('\n').map(JSON.parse);
    if (failure) { assert.equal(run.status, 1); assert.ok(lines.some(r => r.erro)); }
    else {
      assert.equal(run.status, 0, run.stderr);
      assert.deepEqual(lines[0].modelos.map(m => m.value), ['gpt-6-astra', 'gpt-future']);
      assert.deepEqual(lines[0].modelos[0].supportedEffortLevels, ['ultra']);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('selected effort uses the official turn/start schema', () => {
  assert.deepEqual(validarPayload('turn/start', {threadId:'test', input:[{type:'text',text:'hello'}], effort:'ultra'}), []);
});

// ── a catalogue that cannot be read says why (a Mac, 09/10/2026) ───────────────────────────────────
// The desktop reads ONE kind of line from `--modelos`: `modelos` or `erro`. A runner that could not
// start `codex` used to write `{"type":"error"}` — a line the desktop drops — and the picker said
// "Atualize o codex-runner", for a runner that was up to date.
const rodar = (args, codex, extraEnv = {}) => spawnSync(process.execPath,
  [new URL('./codex-runner.mjs', import.meta.url).pathname, ...args], {
    env: { ...process.env, SHVIA_CODEX_BIN: codex, ...extraEnv }, encoding: 'utf8', timeout: 8000, input: '',
  });
const linhas = (saida) => saida.trim().split('\n').filter(Boolean).map(JSON.parse);

test('🔴 --modelos with no codex to start answers `erro`, not a line the desktop drops', () => {
  const run = rodar(['--modelos'], '/nao/existe/codex');
  assert.equal(run.error, undefined);
  assert.equal(run.status, 1);
  const l = linhas(run.stdout);
  assert.equal(l.length, 1);
  assert.match(l[0].erro, /codex não encontrado.*ENOENT/);
  assert.equal(l.some((x) => x.type === 'error'), false);
});

test('🔴 --modelos with a codex that dies at once answers `erro` with how it died', () => {
  const dir = mkdtempSync(join(tmpdir(), 'shvia-catalogue-'));
  try {
    const fake = join(dir, 'codex');
    writeFileSync(fake, `#!${process.execPath}\nprocess.exit(3);\n`, { mode: 0o755 });
    const run = rodar(['--modelos'], fake);
    assert.equal(run.error, undefined);
    assert.equal(run.status, 1);
    const l = linhas(run.stdout);
    assert.equal(l.length, 1);
    assert.match(l[0].erro, /encerrou sem aviso \(código 3\)/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the same failure outside --modelos is still the `type: error` line the bridge matches on', () => {
  const run = rodar(['--cwd', tmpdir()], '/nao/existe/codex');
  assert.equal(run.error, undefined);
  assert.equal(run.status, 1);
  const l = linhas(run.stdout);
  assert.equal(l[0].type, 'error');
  assert.match(l[0].message, /não encontrado/);
  assert.equal('erro' in l[0], false);
});

test('a codex found by absolute path has its own directory first on the PATH it runs with', () => {
  const dir = mkdtempSync(join(tmpdir(), 'shvia-catalogue-'));
  try {
    const fake = join(dir, 'codex');
    // Answers `initialize`, then reports the PATH it was started with through the catalogue's
    // model name — so the test reads it from the runner's own output.
    writeFileSync(fake, `#!${process.execPath}
const rl = require('node:readline').createInterface({input: process.stdin});
rl.on('line', line => {
 const q = JSON.parse(line);
 const result = q.method === 'initialize' ? {} : {data:[{model:process.env.PATH.split(':')[0],displayName:'M',isDefault:true,supportedReasoningEfforts:[]}],nextCursor:null};
 console.log(JSON.stringify({id:q.id,result}));
});
`, { mode: 0o755 });
    const run = rodar(['--modelos'], fake, { PATH: '/usr/bin:/bin' });
    assert.equal(run.status, 0, run.stderr);
    assert.equal(linhas(run.stdout)[0].modelos[0].value, dir);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
