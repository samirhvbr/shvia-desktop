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
