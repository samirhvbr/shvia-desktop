import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

/**
 * PLAN turns (1.9.0) in the Claude runner — a SOURCE check, and it says so.
 *
 * The runner acts on import and imports the Agent SDK, which CI does not install, so a unit
 * test cannot feed it a line (see `entrada.test.mjs`). The DECISION is unit-tested in
 * `politica.test.mjs` (`decidirNoPlano`). This guards the three wires that make it reach a
 * real turn: the host reader keeps `plano` only when it is literally `true`, the pump hands it
 * to `runTurn`, and the hook consults the plan rule BEFORE `decidir`, where a level or a card
 * could otherwise release the write.
 */
const fonte = readFileSync(new URL("./claude-runner.mjs", import.meta.url), "utf8");

test("the host reader keeps `plano` only when it is literally true", () => {
  const leitor = fonte.slice(fonte.indexOf('rl.on("line"'));
  assert.match(leitor, /plano:\s*msg\.plano === true/);
});

test("the pump hands `plano` to runTurn, and runTurn sets the turn's mode", () => {
  assert.match(fonte, /await runTurn\(item\.text, item\.images, item\.plano\)/);
  assert.match(fonte, /async function runTurn\(text, images, plano = false\) \{\s*turnoPlano = plano === true;/);
});

test("the hook asks the plan rule before decidir", () => {
  // `julgarChamada` is the hook's body; `preToolUse` wraps it so that it fails closed (below).
  const a = fonte.indexOf("async function julgarChamada(");
  const fim = fonte.indexOf("\nasync function ", a + 10);
  const hook = fonte.slice(a, fim > a ? fim : undefined);
  // The call is UNCONDITIONAL and carries the turn's mode: a condition around it (the reversion
  // `if (false && turnoPlano)`) or a constant in place of `turnoPlano` is what this refuses.
  const chamada = /const noPlano = decidirNoPlano\(toolName, turnoPlano\);\s*if \(noPlano\) \{[\s\S]{0,120}?return denyDecision\(noPlano\.motivo\);/;
  const m = chamada.exec(hook);
  const decide = hook.indexOf("decidirSeguro({");
  assert.ok(a >= 0 && decide > 0, "the hook moved or lost decidirSeguro: update this ruler");
  assert.ok(m, "the hook must call decidirNoPlano(toolName, turnoPlano) unconditionally and DENY on a hit");
  assert.ok(m.index < decide, "the plan rule must run before decidir");
  assert.equal((hook.slice(0, m.index).match(/\bif \(/g) || []).length, 0, "no condition may sit before the plan rule");
});

// 🔴 1.42.6. The hook had no try/catch, and what the SDK does with a hook that throws was never measured:
// `allow` is one reading, and a line nested deep enough made `decidir` throw. The wrapper turns any error
// into a DENY, and the verdict comes from `decidirSeguro`, which turns a policy error into a card.
test("the hook fails closed: whatever throws inside it is a deny, and the verdict is decidirSeguro's", () => {
  const a = fonte.indexOf("async function preToolUse(");
  const fim = fonte.indexOf("\nasync function ", a + 10);
  const hook = fonte.slice(a, fim > a ? fim : undefined);
  assert.match(hook, /try \{\s*return await julgarChamada\(input\);\s*\} catch \(e\) \{[\s\S]*return denyDecision\(/);
  assert.ok(!/\bdecidir\(/.test(fonte.replace(/decidirSeguro\(/g, "").replace(/decidirNoPlano\(/g, "")), "the runner must not call decidir directly");
});
