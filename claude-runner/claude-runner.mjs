#!/usr/bin/env node
// claude-runner.mjs — motor "Claude Code (assinatura)" para o Modo Code do SHVIA.
//
// Dirige o Claude Agent SDK (@anthropic-ai/claude-agent-sdk) e fala EXATAMENTE o
// mesmo protocolo NDJSON do `anna` (SHVIA-CODE/docs/embedding.md). Por isso é
// drop-in atrás do code_bridge.rs: o bridge e a UI de cards do Modo Code não
// mudam — só troca quem produz o NDJSON.
//
// AUTH: usa as credenciais guardadas por `claude login` (assinatura Pro/Max).
// NUNCA setamos ANTHROPIC_API_KEY; ao contrário, a REMOVEMOS do processo para
// garantir o fallback à assinatura (o SDK daria precedência à API key = pay-per-token).
// O login é do usuário, pelo cliente oficial — este runner nunca o embute.
//
// GATES — AUTORIDADE ÚNICA (Fase 1): a política de permissão é um PreToolUse
// hook + `settingSources: []`. O hook é avaliado ANTES das allow-rules e as
// bypassa, e o settings-vazio impede que o ~/.claude/settings.json PESSOAL do
// usuário auto-aprove escrita/bash sem card. Assim "nada roda/escreve sem o dev
// ver" (regra do produto) fica garantido — os cards do Modo Code são a única
// fonte de verdade de permissão.
//
// Contrato (embedding.md):
//   Host → runner (stdin, 1 linha JSON/msg):
//     {"type":"user","text":"..."}                       inicia um turno
//     {"type":"exit"}                                     encerra
//     {"id":"...","decision":"approve"|"always"|"reject"} resposta a um gate_request
//   runner → Host (stdout):
//     {"type":"model","model","server"}
//     {"type":"text","delta"}
//     {"type":"tool_call","id","name","arguments"}
//     {"type":"tool_result","id","name","bytes","content"}
//     {"type":"gate_request","id","scope","policy","preview"}   (BLOQUEIA)
//     {"type":"usage","tokens","cost","estimated"}
//     {"type":"turn_done"} | {"type":"error"|"warn","message"}

import { query } from "@anthropic-ai/claude-agent-sdk";
import * as readline from "node:readline";

// ---------------------------------------------------------------- saída NDJSON
function emit(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

// ------------------------------------------------------------------ argumentos
// --model <perfil>  --cwd <dir>   (tudo opcional; espelha o spawn do anna)
function argOf(flag) {
  const i = process.argv.indexOf(flag);
  return i >= 0 && i + 1 < process.argv.length ? process.argv[i + 1] : undefined;
}
const MODEL = argOf("--model"); // 'opus'|'sonnet'|'haiku'|'fable'|id completo
const PROJECT_DIR = argOf("--cwd") || process.cwd();

// ------------------------------------------------------- forçar auth de assinatura
if (process.env.ANTHROPIC_API_KEY) {
  emit({
    type: "warn",
    message:
      "ANTHROPIC_API_KEY presente no ambiente — removida DESTE processo para forçar auth por assinatura (claude login).",
  });
  delete process.env.ANTHROPIC_API_KEY;
}

// ------------------------------------------------- gates pendentes (tool_use_id → resolve)
const pendingGates = new Map();

// Leitura = auto (espelha a política do Modo Code: leitura não pede aprovação).
const READ_ONLY_TOOLS = new Set([
  "Read", "Glob", "Grep", "LS", "NotebookRead", "WebFetch", "WebSearch", "TodoWrite",
]);

// (toolName, input) → preview do gate_request (diff | command | commit)
function toPreview(toolName, input) {
  const inp = input || {};
  if (toolName === "Bash") {
    return {
      kind: "command",
      command: String(inp.command ?? ""),
      why: String(inp.description ?? ""),
    };
  }
  if (toolName === "Write") {
    const path = String(inp.file_path ?? inp.path ?? "");
    const body = String(inp.content ?? "");
    const diff = body.split("\n").map((l) => "+ " + l).join("\n");
    return { kind: "diff", path, diff };
  }
  if (toolName === "Edit" || toolName === "MultiEdit") {
    const path = String(inp.file_path ?? "");
    const edits =
      toolName === "MultiEdit"
        ? inp.edits ?? []
        : [{ old_string: inp.old_string, new_string: inp.new_string }];
    const diff = edits
      .map((e) => {
        const oldL = String(e?.old_string ?? "").split("\n").map((l) => "- " + l).join("\n");
        const newL = String(e?.new_string ?? "").split("\n").map((l) => "+ " + l).join("\n");
        return [oldL, newL].filter(Boolean).join("\n");
      })
      .join("\n");
    return { kind: "diff", path, diff };
  }
  // fallback: descreve a chamada como comando (visível no card)
  return { kind: "command", command: `${toolName} ${JSON.stringify(inp)}`, why: "" };
}

function allowDecision(reason) {
  return { hookSpecificOutput: { hookEventName: "PreToolUse", permissionDecision: "allow", permissionDecisionReason: reason } };
}
function denyDecision(reason) {
  return { hookSpecificOutput: { hookEventName: "PreToolUse", permissionDecision: "deny", permissionDecisionReason: reason } };
}

// PreToolUse hook — a POLÍTICA inteira do Modo Code. Roda antes de CADA tool,
// bypassa allow-rules. Leitura = allow; o resto vira gate_request e BLOQUEIA
// até a decisão do card. Usa o tool_use_id REAL (casa com o tool_result).
async function preToolUse(input /* PreToolUseHookInput */) {
  const toolName = input?.tool_name ?? "";
  const toolInput = input?.tool_input ?? {};
  const id = input?.tool_use_id ?? "";

  if (READ_ONLY_TOOLS.has(toolName)) {
    return allowDecision("leitura (auto)");
  }

  emit({
    type: "gate_request",
    id,
    scope: toolName,
    policy: "confirm",
    preview: toPreview(toolName, toolInput),
  });
  const decision = await new Promise((resolve) => pendingGates.set(id, resolve));
  if (decision === "approve" || decision === "always") {
    return allowDecision("Aprovado pelo usuário.");
  }
  return denyDecision("Rejeitado pelo usuário.");
}

// ------------------------------------------------------------------ loop de turnos
let sessionId; // mantém contexto entre turnos (resume)
let busy = false;
let stdinClosed = false; // no modo pipe (one-shot), sair após esvaziar a fila
const queue = [];

async function runTurn(text) {
  const options = {
    cwd: PROJECT_DIR,
    includePartialMessages: true, // deltas de texto via stream_event
    permissionMode: "default",
    settingSources: [],           // NÃO herda ~/.claude/settings.json do usuário
    hooks: {
      PreToolUse: [{ hooks: [preToolUse] }], // política única de permissão
    },
    ...(MODEL ? { model: MODEL } : {}),
    ...(sessionId ? { resume: sessionId } : {}),
  };

  let sawTextDelta = false;

  for await (const message of query({ prompt: text, options })) {
    switch (message.type) {
      case "system":
        if (message.subtype === "init") {
          sessionId = message.session_id ?? sessionId;
          emit({
            type: "model",
            model: message.model ?? message.data?.model ?? MODEL ?? "claude",
            server: "anthropic (assinatura)",
          });
        }
        break;

      case "stream_event": {
        const ev = message.event;
        if (ev?.type === "content_block_delta" && ev.delta?.type === "text_delta") {
          sawTextDelta = true;
          emit({ type: "text", delta: ev.delta.text });
        }
        break;
      }

      case "assistant": {
        for (const block of message.message?.content ?? []) {
          // fallback: se os deltas não vierem, emite o texto completo do bloco
          if (block.type === "text" && !sawTextDelta) {
            emit({ type: "text", delta: block.text });
          }
          if (block.type === "tool_use") {
            emit({ type: "tool_call", id: block.id, name: block.name, arguments: block.input });
          }
        }
        break;
      }

      case "user": {
        // resultados de ferramenta voltam como content do papel user
        for (const block of message.message?.content ?? []) {
          if (block.type === "tool_result") {
            const content =
              typeof block.content === "string" ? block.content : JSON.stringify(block.content);
            emit({
              type: "tool_result",
              id: block.tool_use_id,
              name: "",
              bytes: Buffer.byteLength(content),
              content,
            });
          }
        }
        break;
      }

      case "result": {
        const tin = message.usage?.input_tokens ?? 0;
        const tout = message.usage?.output_tokens ?? 0;
        emit({
          type: "usage",
          tokens: tin + tout,
          cost: message.total_cost_usd ?? 0,
          estimated: true, // sob assinatura o custo USD é indicativo, não faturado por token
        });
        if (message.subtype === "error") {
          emit({ type: "error", message: String(message.result ?? "erro no turno") });
        }
        emit({ type: "turn_done" });
        break;
      }
    }
  }
}

async function pump() {
  if (busy) return;
  const text = queue.shift();
  if (text === undefined) return;
  busy = true;
  try {
    await runTurn(text);
  } catch (e) {
    emit({ type: "error", message: String(e?.message ?? e) });
    emit({ type: "turn_done" });
  } finally {
    busy = false;
    if (queue.length) pump();
    else if (stdinClosed) process.exit(0); // one-shot: turno acabou, stdin em EOF
  }
}

// --------------------------------------------------------------- leitor de stdin
const rl = readline.createInterface({ input: process.stdin });

rl.on("line", (raw) => {
  const line = raw.trim();
  if (!line) return;
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    emit({ type: "warn", message: "linha stdin não-JSON ignorada" });
    return;
  }
  if (msg.type === "exit") {
    rl.close();
    process.exit(0);
  }
  if (msg.type === "user") {
    queue.push(String(msg.text ?? ""));
    pump();
    return;
  }
  if (msg.id && msg.decision) {
    const resolve = pendingGates.get(msg.id);
    if (resolve) {
      pendingGates.delete(msg.id);
      resolve(msg.decision);
    } else {
      emit({ type: "warn", message: `decisão para gate desconhecido: ${msg.id}` });
    }
    return;
  }
  emit({ type: "warn", message: "mensagem stdin não reconhecida" });
});

// stdin fechou com gate pendente → nega (seguro), igual ao anna
rl.on("close", () => {
  for (const [, resolve] of pendingGates) resolve("reject");
  pendingGates.clear();
  stdinClosed = true;
  if (!busy && queue.length === 0) process.exit(0); // one-shot já ocioso
});
