// esquema.mjs — validates every outgoing payload against Codex's own generated
// schema, BEFORE it goes on the wire.
//
// 🔴 This exists because of five defects in one afternoon (09/09/2026), all with the
// same signature: **the arithmetic was right and the SHAPE was invented.** A turn that
// ended on the ack, a usage counter reading a field its notification does not have, an
// error text read one level too shallow, a retry treated as a death, and a decision
// dropped because the id was `0`. Unit tests stayed green through all of them — they
// asserted the shape their author made up, using data made up in that same shape.
//
// A schema written by the other side cannot be fooled that way: it is the one document
// in this runner that this author did not write.
//
// The strictness is deliberate and is NOT what JSON Schema does by default: an unknown
// property is an ERROR here, even where the schema omits `additionalProperties: false`.
// For payloads we SEND, a field the server does not know is never a harmless extra —
// it is a typo, a renamed field, or a guess, and every one of the five started as
// exactly that. Erring toward the alarm costs a fixed test; erring toward silence cost
// the afternoon.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = dirname(fileURLToPath(import.meta.url));
const BUNDLE = "schemas/codex_app_server_protocol.v2.schemas.json";

let defs = null;
/** Loaded once, lazily: a runner that cannot find its schema must say so, not crash. */
function definicoes() {
  if (defs) return defs;
  const bruto = JSON.parse(readFileSync(resolve(RAIZ, BUNDLE), "utf8"));
  defs = bruto.definitions || bruto.$defs || {};
  return defs;
}

/** The method → params-definition map. Only what this runner actually sends. */
export const ESQUEMA_DO_METODO = Object.freeze({
  "initialize": "InitializeParams",
  "model/list": "ModelListParams",
  "thread/start": "ThreadStartParams",
  "turn/start": "TurnStartParams",
  "command/exec": "CommandExecParams",
});

function resolver(no, d, profundidade = 0) {
  if (!no || typeof no !== "object" || profundidade > 8) return no;
  if (typeof no.$ref === "string") {
    const nome = no.$ref.replace("#/definitions/", "").replace("#/$defs/", "");
    return resolver(d[nome], d, profundidade + 1);
  }
  return no;
}

function tipoBate(valor, esperado) {
  switch (esperado) {
    case "string": return typeof valor === "string";
    case "boolean": return typeof valor === "boolean";
    case "integer": return Number.isInteger(valor);
    case "number": return typeof valor === "number";
    case "array": return Array.isArray(valor);
    case "object": return valor !== null && typeof valor === "object" && !Array.isArray(valor);
    case "null": return valor === null;
    default: return true; // tipo que não modelamos: não inventa reprovação
  }
}

/**
 * Returns an array of problems — empty means valid.
 *
 * Deliberately shallow: it checks the TOP level of the payload (required present,
 * no unknown keys, primitive types). That is where all five defects lived, and a
 * full recursive validator would be a second implementation of JSON Schema that
 * nobody here would test — the exact "instrument nobody audits" this house warns
 * about. Depth can be added the day a defect asks for it.
 */
export function validarPayload(method, params) {
  const nome = ESQUEMA_DO_METODO[method];
  if (!nome) return []; // método que não mandamos: nada a afirmar
  const d = definicoes();
  const esquema = d[nome];
  if (!esquema) return [`schema \`${nome}\` não existe no bundle — o Codex mudou o protocolo`];

  const problemas = [];
  const props = esquema.properties || {};
  const obrigatorios = esquema.required || [];
  const p = params || {};

  for (const chave of obrigatorios) {
    if (p[chave] === undefined) problemas.push(`falta o campo obrigatório \`${chave}\``);
  }
  for (const chave of Object.keys(p)) {
    if (!props[chave]) {
      problemas.push(`campo \`${chave}\` não existe em ${nome} — inventado, renomeado ou com typo`);
      continue;
    }
    const alvo = resolver(props[chave], d);
    const tipo = alvo?.type;
    if (typeof tipo === "string" && p[chave] !== undefined && p[chave] !== null && !tipoBate(p[chave], tipo)) {
      problemas.push(`campo \`${chave}\` devia ser ${tipo}, veio ${Array.isArray(p[chave]) ? "array" : typeof p[chave]}`);
    }
  }
  return problemas;
}
