// protocolo.mjs — the translation between Codex's app-server and the SHVIA NDJSON.
//
// This module is PURE on purpose, exactly like `politica.mjs` is for the Claude
// runner: `codex-runner.mjs` runs on import (it spawns a child and owns stdio), so
// anything that lives there cannot be tested. Every decision that can be made from
// data alone lives here, and `protocolo.test.mjs` exercises it.
//
// ## What each side is
//
// - Codex speaks **newline-delimited JSON-RPC** over the stdio of
//   `codex app-server --stdio`. Measured on 09/09/2026: no `Content-Length`
//   framing, one JSON object per line, notifications arrive as `{method, params}`
//   with no `id`, and server-to-client REQUESTS arrive with an `id` that must be
//   answered.
// - SHVIA speaks the NDJSON of `anna` (SHVIA-CODE/docs/embedding.md), which the
//   Claude runner already emits. The bridge and the Code-mode cards consume that
//   and nothing else, so this runner is drop-in for the same reason the Claude one
//   was: only the producer changes.
//
// ## 🔴 The inversion that decides this whole design
//
// The Claude runner DECIDES what to gate — the SDK does not ask, so `politica.mjs`
// answers "allow or card?" for every tool call. **Codex is the other way around:
// its app-server asks us**, with `item/commandExecution/requestApproval` and
// `item/fileChange/requestApproval`, and blocks until the answer comes back.
//
// So here the runner FORWARDS instead of deciding, and the SHVIA card stays the
// single source of permission truth. What we still own is the *policy we ask
// Codex to run under* — `politicaDoNivel` below — because a permissive policy
// would mean Codex never asks, and a card that is never shown is the same as no
// card at all.

/**
 * The ONE policy this engine runs under. There is no level picker for Codex, and
 * that is the honest shape rather than a reduced one.
 *
 * 🔴 Measured on 09/09/2026, three configurations, one answer: **Codex has no
 * "ask about everything" mode.** With `untrusted` + `read-only`, `echo hello` RAN
 * with no card. With `askForApproval.granular` (every flag true, behind the
 * `experimentalApi` capability), it ran with no card again. Codex's own execpolicy
 * decides what is trivially safe, and an integrator cannot turn that off.
 *
 * So Manual/Edit/Auto would be three labels over one behaviour — the approval pill
 * lying three different ways. What Codex DOES guarantee, and what measurement M
 * proved live, is the BOUNDARY: asked to write outside the workspace root, it
 * emitted `execCommandApproval` with the reason *"I need to write to the requested
 * file path outside the current sandbox root"*, and the rejection held — the file
 * was never created.
 *
 * That is the whole promise, and the doc states it with what it does not cover:
 * **inside the project, writes do not ask.**
 *
 * `on-request` + `workspace-write` is what M measured. `granular` is deliberately
 * NOT used: it needs `experimentalApi` on top of an already experimental
 * app-server, and it bought nothing — paying two experimental dependencies for a
 * guarantee neither delivers is the worst trade available.
 */
export const POLITICA = Object.freeze({
  askForApproval: "on-request",
  sandboxMode: "workspace-write",
});

/**
 * `never` must never reach the wire. Kept as a function so the ruler has something
 * to bite: if someone later reintroduces a level picker, this is the invariant that
 * has to survive it.
 */
export function politicaValida(p) {
  return !!p && p.askForApproval !== "never" && p.sandboxMode !== "danger-full-access";
}

/** The SHVIA decisions, as the host sends them on stdin. */
const APROVA = new Set(["approve", "always"]);

/**
 * Translate a SHVIA card decision into the value the app-server expects.
 *
 * The vocabularies differ per request, which is why this takes the method: v1
 * `execCommandApproval` answers `ReviewDecision`
 * (`approved` · `approved_for_session` · `denied` · `timed_out` · `abort`), while
 * the v2 `item/*` requests answer `accept` · `acceptForSession` · `decline` ·
 * `cancel`. Sending the wrong vocabulary is rejected by the server, and a
 * rejected answer leaves the turn hanging — the failure looks like a hang, not
 * like an error, so it is worth a table instead of a guess.
 */
export function decisaoParaResposta(method, decision) {
  const aprovou = APROVA.has(decision);
  const paraSempre = decision === "always";

  if (method === "execCommandApproval" || method === "applyPatchApproval") {
    if (!aprovou) return { decision: "denied" };
    return { decision: paraSempre ? "approved_for_session" : "approved" };
  }
  // v2: item/commandExecution/requestApproval · item/fileChange/requestApproval
  if (!aprovou) return { decision: "decline" };
  return { decision: paraSempre ? "acceptForSession" : "accept" };
}

/** Methods that BLOCK the turn until we answer. */
export const PEDIDOS_QUE_BLOQUEIAM = new Set([
  "execCommandApproval",
  "applyPatchApproval",
  "item/commandExecution/requestApproval",
  "item/fileChange/requestApproval",
]);

/**
 * Turn a blocking server request into the `gate_request` the Code-mode card
 * renders. `id` is the JSON-RPC id — the same value the answer has to carry, and
 * the same value the host echoes back in `{"id","decision"}`.
 *
 * The preview shapes (`command` | `diff`) are the ones the card already knows how
 * to draw, so no client change is needed to show a Codex gate.
 */
export function pedidoParaGate(msg) {
  const { id, method, params } = msg || {};
  if (!PEDIDOS_QUE_BLOQUEIAM.has(method)) return null;
  const p = params || {};
  const why = String(p.reason ?? "");

  if (method === "execCommandApproval" || method === "item/commandExecution/requestApproval") {
    const cmd = Array.isArray(p.command) ? p.command.join(" ") : String(p.command ?? "");
    return {
      type: "gate_request",
      id,
      scope: "Bash",
      policy: "confirm",
      preview: { kind: "command", command: cmd, why },
    };
  }
  // File change. The v1 `applyPatchApproval` carries the patch itself; the v2
  // request does NOT (its params are threadId/turnId/itemId/grantRoot/reason) —
  // the diff arrived earlier as `item/fileChange/*` notifications. Inventing a
  // diff here would be worse than saying which file: a card that shows a patch
  // the agent did not propose is a lie with a diff attached.
  const path = String(p.grantRoot ?? p.path ?? "");
  const diff = typeof p.patch === "string" ? p.patch : "";
  return {
    type: "gate_request",
    id,
    scope: "Edit",
    policy: "confirm",
    preview: diff
      ? { kind: "diff", path, diff }
      : { kind: "command", command: `apply file changes${path ? " in " + path : ""}`, why },
  };
}

/**
 * Translate one app-server NOTIFICATION into a SHVIA NDJSON event, or `null` when
 * it carries nothing the Code mode shows.
 *
 * Returning `null` for the unknown is deliberate. The app-server publishes 68
 * notification methods and most are about surfaces SHVIA does not have (realtime
 * audio, marketplace, Windows sandbox setup). Forwarding them as text would fill
 * the timeline with noise; failing on them would make every Codex release a
 * breaking change here.
 */
export function traduzirNotificacao(msg) {
  const { method, params } = msg || {};
  const p = params || {};

  switch (method) {
    case "item/agentMessage/delta":
      return { type: "text", delta: String(p.delta ?? "") };

    // Reasoning is shown as text so the user sees the engine working. The Code
    // timeline has no separate "thinking" channel, and silence during a long
    // reasoning block reads as a freeze.
    case "item/reasoning/textDelta":
    case "item/reasoning/summaryTextDelta":
      return { type: "text", delta: String(p.delta ?? "") };

    case "item/started": {
      const item = p.item || {};
      if (item.type !== "commandExecution" && item.type !== "fileChange") return null;
      return {
        type: "tool_call",
        id: String(item.id ?? ""),
        name: item.type === "commandExecution" ? "Bash" : "Edit",
        arguments: item,
      };
    }

    case "item/completed": {
      const item = p.item || {};
      if (item.type === "error") {
        return { type: "warn", message: String(item.message ?? "") };
      }
      if (item.type === "commandExecution" || item.type === "fileChange") {
        const content = String(item.aggregatedOutput ?? item.output ?? "");
        return {
          type: "tool_result",
          id: String(item.id ?? ""),
          name: item.type === "commandExecution" ? "Bash" : "Edit",
          bytes: Buffer.byteLength(content, "utf8"),
          content,
        };
      }
      // `agentMessage` also arrives completed, after its deltas. Emitting it
      // would print the whole answer a second time, under the streamed one.
      return null;
    }

    // 🔴 Usage does NOT come with `turn/completed`. Its params are `{threadId,
    // turn}` and nothing else — reading `params.usage` there reported `tokens: 0`
    // on every turn, which the first live smoke printed and no unit test could
    // have: the shape was wrong, not the arithmetic.
    //
    // It arrives here instead, and in camelCase. Worth stating because
    // `codex exec --json` — the interface measured first — reports the same
    // numbers as `input_tokens`/`output_tokens`, in snake_case. Two interfaces of
    // the same Codex, two vocabularies; the one that counts is the one in use.
    //
    // `last` is this turn; `total` is the whole thread and would climb forever in
    // a per-turn counter.
    case "thread/tokenUsage/updated": {
      const u = p.tokenUsage?.last || {};
      const tokens = Number(
        u.totalTokens ?? (Number(u.inputTokens ?? 0) + Number(u.outputTokens ?? 0)),
      );
      // `estimated: true` is the honest label: this is the engine's own count, and
      // it never went through the SHVIA gateway, so there is no audited cost to
      // state. Claiming a cost we did not measure would be worse than showing none.
      return { type: "usage", tokens, cost: null, estimated: true };
    }

    // The turn's end is handled by method name in `codex-runner.mjs` (it releases
    // the wait and emits `turn_done`); there is no payload here worth forwarding.
    case "turn/completed":
      return null;

    // 🔴 The payload is `{error:{message, codexErrorInfo, additionalDetails}, willRetry,
    // threadId, turnId}` — the text is NESTED. Reading `params.message` returned
    // undefined and the runner printed `unknown error`, throwing away the only copy
    // of what the server said. The real message that day was
    // *"The model `gpt-5.5` does not exist or you do not have access to it"* — an
    // answer, replaced by a shrug.
    //
    // `additionalDetails` carries the useful half (`message` was just
    // "Reconnecting... 2/5"), so both are shown when they differ.
    case "error": {
      const e = p.error || p;
      const msg = String(e.message ?? p.summary ?? "");
      const det = String(e.additionalDetails ?? "");
      const texto = [msg, det && det !== msg ? det : ""].filter(Boolean).join(" — ");
      return {
        // A retry is not a failure yet: it is reported as a warning so the
        // timeline says something is happening, without claiming the turn died.
        type: p.willRetry ? "warn" : "error",
        message: texto || "erro sem descrição vindo do app-server",
      };
    }

    case "warning":
    case "configWarning":
    case "guardianWarning":
    case "deprecationNotice":
      return { type: "warn", message: String(p.summary ?? p.message ?? "") };

    default:
      return null;
  }
}

/**
 * Is this host line a card decision (and not a turn, an exit, or noise)?
 *
 * 🔴 It exists as a function because of the shape it replaced: `msg.id && msg.decision`.
 * The app-server numbers ITS requests from ZERO, so the first approval card of every
 * session carries `id: 0` — falsy — and the decision was dropped on the floor. The
 * runner never answered, the server kept waiting, and the turn hung forever. On screen
 * that is a freeze with no error, which is the failure mode that costs the most to
 * diagnose, and it would have hit the FIRST card a person ever saw in this engine.
 *
 * Measured on 09/09/2026, in the run that proved the sandbox boundary gates.
 */
export function ehDecisao(msg) {
  return !!msg && msg.id !== undefined && msg.id !== null && typeof msg.decision === "string";
}
