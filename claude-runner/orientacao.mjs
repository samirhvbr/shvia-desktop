// orientacao.mjs — steering the turn in flight (1.11.0), the Claude engine's half.
//
// What the person types while the agent works goes INTO the turn: `streamInput` with
// `priority: "next"` on the turn's own query. Measured against the real Agent SDK on 01/10/2026
// (0.3.258, Haiku, a 4 s `sleep` tool call, the correction pushed during it): the model read it
// before its next request and the turn ended with ONE result that obeyed it — with the plain-string
// prompt text turns use, so `montarPrompt` keeps its string path.
//
// 🔴 `streamInput` resolving does NOT mean the model read it. A correction that lands while the
// model writes its LAST message is never consumed, and nothing says so. So a steer is confirmed
// (`steer_applied`) only when the main conversation starts a NEW model request after the push —
// a `message_start` stream event without `parent_tool_use_id` (a sub-agent's request does not carry
// the main thread's input). One still unconfirmed when the turn ends is handed back with
// `steer_deferred` and NOT run here: the host decides — the Code-mode page puts it at the front of
// its own queue, where its turn bookkeeping lives.
//
// Pure on purpose, like `parada.mjs`: `claude-runner.mjs` acts on import and imports the SDK, which
// CI does not install, so this is the part a test can drive (`orientacao.test.mjs`).

/**
 * @param {object} deps
 * @param {(ev: object) => void} deps.emitir  writes one protocol line to the host
 * @param {(texto: string) => void} deps.enfileirar  runs a text as the next message (a steer
 *   with no turn running, which is just a message)
 */
export function criarOrientacao({ emitir, enfileirar }) {
  let pendentes = [];

  return {
    /**
     * A steer from the host. `consulta` is the turn's Query while one runs, else null.
     * @returns {"injetada"|"enfileirada"|"vazia"}
     */
    orientar(texto, consulta, sessionId) {
      if (!String(texto).trim()) return "vazia";
      // Nothing running: a steer is just the next message.
      if (!consulta || typeof consulta.streamInput !== "function") {
        enfileirar(texto);
        return "enfileirada";
      }
      pendentes.push(texto);
      consulta.streamInput((async function* () {
        yield {
          type: "user",
          message: { role: "user", content: texto },
          parent_tool_use_id: null,
          session_id: sessionId ?? "",
          priority: "next",
        };
      })()).catch(() => { /* the turn closed under it: `encerrarTurno` defers what is left */ });
      return "injetada";
    },

    /** Every SDK message of the turn passes here, before it is translated. */
    observar(message) {
      if (!pendentes.length) return;
      if (message?.type !== "stream_event" || message.event?.type !== "message_start") return;
      if (message.parent_tool_use_id) return;
      for (const texto of pendentes) emitir({ type: "steer_applied", text: texto });
      pendentes = [];
    },

    /** The turn ended (well or not): what it never read goes back to the host, once. */
    encerrarTurno() {
      for (const texto of pendentes) emitir({ type: "steer_deferred", text: texto });
      pendentes = [];
    },

    get pendentes() { return pendentes.length; },
  };
}
