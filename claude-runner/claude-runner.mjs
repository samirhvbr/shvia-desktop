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
const EFFORT = argOf("--effort"); // 'low'|'medium'|'high'|'xhigh'|'max'
const PROJECT_DIR = argOf("--cwd") || process.cwd();

/**
 * `--modelos`: imprime o catálogo do Claude Code em JSON e sai.
 *
 * Existe porque o seletor de MODELO/ESFORÇO da UI mostrava o catálogo do
 * GATEWAY mesmo com o motor Claude ativo — dois espaços de nomes no mesmo
 * dropdown. `openai/gpt-5.6-sol` não significa nada para o Claude Code, e o
 * seletor de esforço não chegava a lugar nenhum.
 *
 * A lista vem do PRÓPRIO SDK (`supportedModels()`), não de uma cópia nossa:
 * cada linha traz `supportsEffort` e `supportedEffortLevels`, então a UI sabe
 * quais níveis oferecer por modelo e quando desabilitar o seletor — sem a casa
 * manter um catálogo paralelo que envelhece em silêncio.
 *
 * Medido em 21/08: a chamada é de canal de controle e **não consome turno** —
 * a sessão inicializa, responde o controle e encerra sem sampling.
 */
if (process.argv.includes("--modelos")) {
  const q = query({ prompt: "", options: { cwd: PROJECT_DIR, settingSources: [] } });
  try {
    emit({ type: "modelos", modelos: await q.supportedModels() });
  } catch (e) {
    // Falha aqui não é veredito sobre o catálogo — é a listagem sem resposta.
    // A UI cai no fallback dela em vez de mostrar uma lista vazia como se
    // fosse "nenhum modelo disponível".
    emit({ type: "erro", message: `não consegui listar os modelos: ${e?.message ?? e}` });
    process.exitCode = 2;
  } finally {
    await q.interrupt?.().catch(() => {});
  }
  process.exit();
}

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

/** Ferramentas que ESCREVEM arquivo — o degrau que o nível `edit` libera. */
const EDIT_TOOLS = new Set(["Write", "Edit", "MultiEdit", "NotebookEdit"]);

/**
 * Nível de aprovação, espelhando os três do `anna` — **de propósito**.
 *
 * O SDK tem `permissionMode` ('default'|'acceptEdits'|'bypassPermissions'|'plan'|
 * 'dontAsk'|'auto'), e ele **não é usado aqui**: quem decide é o hook
 * `PreToolUse` abaixo, porque é ele que emite `gate_request` e faz o cartão de
 * aprovação aparecer na tela do ShvIA. Trocar pelo `permissionMode` moveria a
 * decisão para dentro do Claude Code — que a casca não renderiza — e o usuário
 * perderia a tela de aprovação em vez de ganhar controle.
 *
 * Por isso os níveis são os mesmos dos dois motores: quem implementa aprovação
 * nos dois é código nosso, então não há dois vocabulários a conciliar.
 *
 * `bypassPermissions` e `dontAsk` ficam FORA por regra da casa — não são "auto",
 * são *sem gate*, e o `anna` recusa isso com todas as letras ("não existe modo
 * yolo"). Um motor não pode ser a porta dos fundos do outro.
 */
const APROVACAO = (() => {
  const v = argOf("--aprovacao");
  return v === "edit" || v === "auto" ? v : "manual";
})();

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

  // Níveis acima de `manual` liberam sem perguntar — e a fronteira é a MESMA do
  // `anna`: `edit` libera escrita de arquivo; `auto` libera também comando.
  //
  // ⚠️ O que NENHUM nível libera é o que sai da pasta do projeto. O `cwd` do
  // SDK confina as ferramentas de arquivo; para `Bash` o comando é livre, então
  // `auto` aqui é "não pergunta por comando", não "pode qualquer coisa". Se um
  // dia isso precisar de cerca própria, o lugar é aqui — e a cerca vem antes do
  // atalho, nunca depois.
  if (APROVACAO !== "manual" && EDIT_TOOLS.has(toolName)) {
    return allowDecision(`edição liberada pelo nível "${APROVACAO}"`);
  }
  if (APROVACAO === "auto") {
    return allowDecision('liberado pelo nível "auto"');
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
    // `effort` guia a profundidade do raciocínio ('low'…'max'). Só entra quando
    // pedido: sem a flag, vale o default do modelo (`high`) — mandar um valor
    // inventado seria escolher por quem não pediu. Quais níveis cada modelo
    // aceita vem de `supportedModels()` (ver `--modelos`), não de uma lista nossa.
    ...(EFFORT ? { effort: EFFORT } : {}),
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
