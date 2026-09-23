import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

/**
 * A host line that is valid JSON but not an object — `null` — must not kill a runner (1.6.18).
 *
 * SOURCE check, and it says so: both runners act on import (the Claude one also imports the SDK,
 * which CI does not install, and the Codex one wires the host reader only after its sandbox
 * probe), so a unit test cannot feed them a line. It guards the one decision: in each host
 * reader the object check sits between `JSON.parse` and the first use of `msg`.
 */
const LEITORES = [
  ["claude-runner/claude-runner.mjs", 'rl.on("line"', "msg.type"],
  ["codex-runner/codex-runner.mjs", "function ligarEntradaDoHost", "ehDecisao(msg)"],
];

test("each host reader rejects a non-object line before touching it", () => {
  const guarda = ["typeof msg !== ", '"object"'].join("");
  for (const [arquivo, inicio, uso] of LEITORES) {
    const fonte = readFileSync(new URL(`../${arquivo}`, import.meta.url), "utf8");
    const a = fonte.indexOf(inicio);
    assert.ok(a >= 0, `${arquivo}: the host reader moved — update this ruler`);
    const parse = fonte.indexOf("JSON.parse(", a);
    const g = fonte.indexOf(guarda, parse);
    const u = fonte.indexOf(uso, parse);
    assert.ok(parse > a && g > parse && g < u, `${arquivo}: \`${uso}\` is reached before the object check`);
  }
});
