// node --test claude-runner/uso.test.mjs
//
// The fixtures are the answers two real accounts gave on 13/10/2026 (Agent SDK 0.3.287), cut to the
// fields that matter and with the code-named buckets kept, because they are what must NOT surface.
import { test } from "node:test";
import assert from "node:assert/strict";
import { metodoDeUso, normalizarUso } from "./uso.mjs";

const MAX = {
  subscription_type: "max",
  rate_limits_available: true,
  rate_limits: {
    five_hour: { utilization: 4, resets_at: "2026-10-14T02:19:59.802291+00:00" },
    seven_day: { utilization: 6, resets_at: "2026-10-20T16:59:59.802314+00:00" },
    seven_day_opus: null,
    seven_day_sonnet: null,
    iguana_necktie: { utilization: 44.27, resets_at: "2026-11-05T07:59:00+00:00", limit_dollars: 250 },
    model_scoped: [{ display_name: "Fable", utilization: 2, resets_at: "2026-10-20T16:59:59.802455+00:00" }],
    extra_usage: { is_enabled: false, utilization: null },
    limits: [{ kind: "session", percent: 4 }],
  },
};

test("a Max account: five hours, weekly and the server's own per-model window, as percent used", () => {
  const u = normalizarUso(MAX);
  assert.equal(u.type, "uso");
  assert.equal(u.plano, "max");
  assert.deepEqual(u.janelas.map((j) => [j.chave, j.nome, j.usado]), [
    ["cinco_horas", null, 4],
    ["semanal", null, 6],
    ["semanal_modelo", "Fable", 2],
  ]);
  assert.equal(u.janelas[0].renova, "2026-10-14T02:19:59.802291+00:00");
});

test("code-named buckets, limits[] and a disabled extra usage never reach the screen", () => {
  const texto = JSON.stringify(normalizarUso(MAX));
  assert.doesNotMatch(texto, /iguana|44\.27|limit_dollars|credito_extra/);
});

test("the legacy Opus window shows when model_scoped does not name it, and only then", () => {
  const legado = structuredClone(MAX);
  legado.rate_limits.seven_day_opus = { utilization: 30, resets_at: "2026-10-20T16:59:59Z" };
  assert.ok(normalizarUso(legado).janelas.some((j) => j.nome === "Opus" && j.usado === 30));
  legado.rate_limits.model_scoped.push({ display_name: "Opus", utilization: 31, resets_at: null });
  assert.deepEqual(normalizarUso(legado).janelas.filter((j) => j.nome === "Opus").map((j) => j.usado), [31]);
});

test("an extra-usage pool that is on is a window of its own", () => {
  const extra = structuredClone(MAX);
  extra.rate_limits.extra_usage = { is_enabled: true, utilization: 12.5 };
  assert.deepEqual(normalizarUso(extra).janelas.at(-1), { chave: "credito_extra", nome: null, minutos: null, usado: 12.5, renova: null });
});

test("no plan windows (API key, Bedrock, Vertex) is said, not shown as zero", () => {
  assert.deepEqual(normalizarUso({ subscription_type: null, rate_limits_available: false, rate_limits: null }),
    { type: "uso", plano: null, janelas: [], semJanelas: true });
});

test("a window with no number keeps the window and says null, never 0", () => {
  const vazio = structuredClone(MAX);
  vazio.rate_limits.five_hour = { utilization: null, resets_at: null };
  assert.deepEqual(normalizarUso(vazio).janelas[0], { chave: "cinco_horas", nome: null, minutos: null, usado: null, renova: null });
});

test("the reader is found by presence, under today's experimental name and a stable one", () => {
  class Hoje { usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET() {} }
  assert.equal(metodoDeUso(new Hoje()), "usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET");
  assert.equal(metodoDeUso({ getUsage() {} }), "getUsage");
  assert.equal(metodoDeUso({ supportedModels() {} }), null);
});
