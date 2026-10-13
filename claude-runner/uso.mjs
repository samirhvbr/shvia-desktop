// The plan limits of a Claude account (`claude-runner --uso`), normalised for the Usage tab.
//
// The numbers come from the Agent SDK's own `/usage` reader, so the Claude Code client reads them
// with the credential it keeps: this runner never touches a token. The page gets one shape for every
// engine (`codex-runner/uso.mjs` emits the same), always as PERCENT USED, never as "left".

/**
 * The SDK method that answers `/usage`, found by PRESENCE.
 *
 * In 0.3.287 it is `usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET`, and the SDK says
 * the name will change when the API is stabilised. Looking it up by name would break silently on
 * that day; looking it up by prefix keeps working across the rename, and `null` lets the page say
 * "this engine version cannot read usage" instead of showing nothing.
 */
export function metodoDeUso(q) {
  if (!q) return null;
  for (const nome of ["getUsage", "usage"]) {
    if (typeof q[nome] === "function") return nome;
  }
  for (let o = q; o && o !== Object.prototype; o = Object.getPrototypeOf(o)) {
    const achado = Object.getOwnPropertyNames(o).find((n) => /^usage_/.test(n) && typeof q[n] === "function");
    if (achado) return achado;
  }
  return null;
}

/** A window as the page reads it: `usado` is 0–100, `renova` is ISO 8601 or null. */
function janela(chave, w, nome = null) {
  if (!w || typeof w !== "object") return null;
  const usado = typeof w.utilization === "number" && Number.isFinite(w.utilization) ? w.utilization : null;
  return { chave, nome, minutos: null, usado, renova: typeof w.resets_at === "string" ? w.resets_at : null };
}

/**
 * `SDKControlGetUsageResponse` → `{type:'uso', plano, janelas}`.
 *
 * Only the documented fields are read: `five_hour`, `seven_day`, `model_scoped[]` (the server's own
 * label, e.g. "Fable"), the legacy `seven_day_opus`/`seven_day_sonnet` when `model_scoped` does not
 * already name them, and `extra_usage` when it is on. The answer also carries code-named buckets and
 * a `limits[]` array (measured 13/10/2026); neither is documented, so neither reaches the screen.
 */
export function normalizarUso(r) {
  const plano = typeof r?.subscription_type === "string" ? r.subscription_type : null;
  const rl = r?.rate_limits;
  if (!r?.rate_limits_available || !rl || typeof rl !== "object") {
    // An API key, Bedrock or Vertex session: there is no plan window, and that is a fact, not a 0%.
    return { type: "uso", plano, janelas: [], semJanelas: true };
  }
  const janelas = [janela("cinco_horas", rl.five_hour), janela("semanal", rl.seven_day)];
  const porModelo = Array.isArray(rl.model_scoped) ? rl.model_scoped : [];
  const nomes = new Set();
  for (const m of porModelo) {
    const nome = typeof m?.display_name === "string" && m.display_name.trim() ? m.display_name.trim() : null;
    if (!nome) continue;
    nomes.add(nome.toLowerCase());
    janelas.push(janela("semanal_modelo", m, nome));
  }
  for (const [campo, nome] of [["seven_day_opus", "Opus"], ["seven_day_sonnet", "Sonnet"]]) {
    if (!nomes.has(nome.toLowerCase())) janelas.push(janela("semanal_modelo", rl[campo], nome));
  }
  const extra = rl.extra_usage;
  if (extra && extra.is_enabled === true && typeof extra.utilization === "number") {
    janelas.push({ chave: "credito_extra", nome: null, minutos: null, usado: extra.utilization, renova: null });
  }
  return { type: "uso", plano, janelas: janelas.filter(Boolean) };
}
