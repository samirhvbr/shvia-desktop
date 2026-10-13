// The plan limits of the Codex account (`codex-runner --uso`), normalised for the Usage tab in the
// same shape as `claude-runner/uso.mjs`: `{type:'uso', plano, janelas}`, always PERCENT USED.

/** Window length → the key the page names. Anything else keeps its minutes and is named by them. */
function chaveDaDuracao(minutos) {
  if (minutos === 300) return "cinco_horas";
  if (minutos === 10080) return "semanal";
  return "janela";
}

function janela(w, nome) {
  if (!w || typeof w !== "object" || typeof w.usedPercent !== "number") return null;
  const minutos = Number.isInteger(w.windowDurationMins) ? w.windowDurationMins : null;
  return {
    chave: chaveDaDuracao(minutos),
    nome,
    minutos,
    usado: w.usedPercent,
    // Unix seconds on the wire; ISO 8601 for the page, like the Claude side.
    renova: Number.isInteger(w.resetsAt) ? new Date(w.resetsAt * 1000).toISOString() : null,
  };
}

/**
 * `account/rateLimits/read` → `{type:'uso', plano, janelas}`.
 *
 * 🔴 The window is named by its LENGTH, never by its slot. Measured 13/10/2026 on a `prolite` plan:
 * `primary` was the WEEKLY window (10080 minutes) and `secondary` was null. Calling `primary` "five
 * hours", as ChatGPT plans usually have it, would have put a weekly number under the wrong label.
 *
 * Extra buckets (`rateLimitsByLimitId` other than the main one) carry the backend's `limitName`.
 */
export function normalizarUso(r) {
  const principal = r?.rateLimits;
  const plano = typeof principal?.planType === "string" && principal.planType !== "unknown" ? principal.planType : null;
  const janelas = [janela(principal?.primary, null), janela(principal?.secondary, null)];
  const outros = r?.rateLimitsByLimitId && typeof r.rateLimitsByLimitId === "object" ? r.rateLimitsByLimitId : {};
  for (const [id, balde] of Object.entries(outros)) {
    if (!balde || id === principal?.limitId) continue;
    const nome = (typeof balde.limitName === "string" && balde.limitName.trim()) || id;
    janelas.push(janela(balde.primary, nome), janela(balde.secondary, nome));
  }
  return { type: "uso", plano, janelas: janelas.filter(Boolean) };
}
