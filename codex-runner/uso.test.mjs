// node --test codex-runner/uso.test.mjs
//
// The first fixture is what a real `prolite` account answered on 13/10/2026 (codex-cli 0.162.1).
import { test } from "node:test";
import assert from "node:assert/strict";
import { normalizarUso } from "./uso.mjs";

const PROLITE = {
  ordinaryUsageAllowed: true,
  rateLimits: {
    limitId: "codex", limitName: null, planType: "prolite",
    primary: { usedPercent: 3, windowDurationMins: 10080, resetsAt: 1792508458 },
    secondary: null,
  },
  rateLimitsByLimitId: {
    codex: { limitId: "codex", primary: { usedPercent: 3, windowDurationMins: 10080, resetsAt: 1792508458 }, secondary: null },
  },
};

test("the window is named by its length: this plan's primary is the WEEKLY one", () => {
  assert.deepEqual(normalizarUso(PROLITE), {
    type: "uso",
    plano: "prolite",
    janelas: [{ chave: "semanal", nome: null, minutos: 10080, usado: 3, renova: "2026-10-20T15:00:58.000Z" }],
  });
});

test("five hours and weekly when the plan has both, in percent used", () => {
  const plus = { rateLimits: { limitId: "codex", planType: "plus",
    primary: { usedPercent: 9, windowDurationMins: 300, resetsAt: null },
    secondary: { usedPercent: 27, windowDurationMins: 10080, resetsAt: null } } };
  assert.deepEqual(normalizarUso(plus).janelas.map((j) => [j.chave, j.usado]), [["cinco_horas", 9], ["semanal", 27]]);
});

test("an unusual length keeps its minutes, and an extra bucket keeps the backend's name", () => {
  const r = { rateLimits: { limitId: "codex", planType: "pro", primary: { usedPercent: 1, windowDurationMins: 60 } },
    rateLimitsByLimitId: { codex_luna: { limitId: "codex_luna", limitName: "GPT-6 Luna", primary: { usedPercent: 40, windowDurationMins: 10080 } } } };
  assert.deepEqual(normalizarUso(r).janelas.map((j) => [j.chave, j.nome, j.minutos, j.usado]),
    [["janela", null, 60, 1], ["semanal", "GPT-6 Luna", 10080, 40]]);
});

test("no windows and an unknown plan are said, not invented", () => {
  assert.deepEqual(normalizarUso({ rateLimits: { planType: "unknown", primary: null, secondary: null } }),
    { type: "uso", plano: null, janelas: [] });
});
