// parada.mjs — the stop handshake of the "Claude Code (subscription)" engine.
//
// Block B2 of docs/code/RUN-20260910.md (ADR-034). The protocol is in
// SHVIA-CODE/docs/embedding.md, §"The stop handshake (`stop_request`)".
//
// Why a module of its own, like `politica.mjs`: `claude-runner.mjs` runs on import, so
// anything that lives in it can only be proved by cutting its source out with a regex
// (`scripts/prova-montar-prompt.mjs`). Everything here is pure — no process, no stdin, no
// SDK — and `parada.test.mjs` proves it under `node --test`.
//
// What the handshake is, in one paragraph. When the model wants to end a turn, the SDK
// fires the `Stop` hook. With `--parada host` the runner does not decide: it emits a
// `stop_request` on stdout, BLOCKS until the host answers by `id`, and translates the
// answer into the hook's output — `continue` becomes `{decision:'block', reason}` (the
// model keeps going, in the same context, with `reason` as the message it reads), `stop`
// becomes an empty output (the turn ends as it always did). Without the flag the hook is
// not even installed, so a host that does not know the event never receives it.
//
// ⚠️ The runner never decides. The rule, the orchestrator profile and the human live on
// the other side of the pipe (the page, on behalf of the server). Two defaults are the
// only judgement here, and both err toward the alarm: no answer within TETO_MS → stop;
// stdin closed → stop. A hung host must never hang a turn.

/** No answer from the host within this → `stop`. The turn ends normally. */
export const TETO_MS = 60_000;

/** `last` is cut here: the host already has the full text from the `text` deltas. */
export const LAST_MAX = 4000;

/** What the model reads when the host says `continue` without a message of its own. */
export const MENSAGEM_PADRAO = "Continue conforme o plano. Não peça confirmação entre etapas.";

/**
 * The `stop_request` event, exactly as embedding.md shapes it.
 * `iteration` counts this turn's stop requests; `reason` says why the model stopped.
 */
export function montarStopRequest({ id, iteration, last, reason }) {
  const texto = String(last ?? "");
  return {
    type: "stop_request",
    id: String(id),
    iteration: Number.isFinite(iteration) && iteration > 0 ? Math.floor(iteration) : 0,
    last: texto.length > LAST_MAX ? texto.slice(0, LAST_MAX) : texto,
    reason: typeof reason === "string" && reason ? reason : "model_stopped",
  };
}

/**
 * The host's answer, normalised. Only an explicit `continue` continues; everything else
 * — `stop`, a gate word like `approve`, garbage, nothing — is `stop`. A message that is
 * blank falls to MENSAGEM_PADRAO: the model needs a reason to go on, and an empty one
 * reads as "stop" to some models.
 */
export function normalizarDecisao(msg) {
  if (msg && msg.decision === "continue") {
    const m = typeof msg.message === "string" && msg.message.trim() ? msg.message.trim() : MENSAGEM_PADRAO;
    return { decision: "continue", message: m };
  }
  return { decision: "stop" };
}

/** The normalised decision → the SDK `Stop` hook output. */
export function saidaDoHook(decisao) {
  if (decisao && decisao.decision === "continue") {
    return { decision: "block", reason: decisao.message };
  }
  return {};
}

/**
 * Waits for the host's answer to `id`, with the ceiling. `pendentes` is the map the
 * stdin reader resolves through (the same design as the gates' `pendingGates`).
 *
 * `timer`/`cancelar` are injectable so the ceiling can be proved without waiting 60 s.
 * The real timer is `unref`ed: a pending stop must not keep the process alive after
 * stdin closed.
 */
export function esperarDecisao(pendentes, id, { tetoMs = TETO_MS, timer = setTimeout, cancelar = clearTimeout } = {}) {
  return new Promise((resolve) => {
    // The resolver is registered BEFORE the timer is armed: a timer that fires at once
    // (the test's stand-in for 60 s) must find the request, or the promise never
    // settles — a hang in the proof, and the exact failure the ceiling exists to prevent.
    let t;
    pendentes.set(id, (msg) => {
      cancelar(t);
      pendentes.delete(id);
      resolve(normalizarDecisao(msg));
    });
    t = timer(() => {
      if (pendentes.delete(id)) resolve({ decision: "stop", motivo: "teto" });
    }, tetoMs);
    if (t && typeof t.unref === "function") t.unref();
  });
}

/** stdin closed: every pending request is answered `stop`, the map is emptied. */
export function encerrarPendentes(pendentes) {
  const resolvers = [...pendentes.values()];
  pendentes.clear();
  for (const resolver of resolvers) resolver({ decision: "stop" });
}

/**
 * Command-line flags → the run options. `--parada host` installs the hook; `--teto-iteracoes`
 * and `--teto-custo` become the SDK caps (`maxTurns`, `maxBudgetUsd`) only when they are
 * positive numbers — a cap of 0 would end every turn before it started, and a word is not
 * a cap. Absent or invalid → no cap, the behaviour before 1.5.0.
 */
export function opcoesDaRun(argOf) {
  const parada = argOf("--parada") === "host";
  const it = Number(argOf("--teto-iteracoes"));
  const custo = Number(argOf("--teto-custo"));
  return {
    parada,
    maxTurns: Number.isFinite(it) && it >= 1 ? Math.floor(it) : undefined,
    maxBudgetUsd: Number.isFinite(custo) && custo > 0 ? custo : undefined,
  };
}
