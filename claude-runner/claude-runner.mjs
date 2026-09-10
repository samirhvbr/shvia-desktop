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
//     {"type":"user","text":"...","images":[{mime,dataBase64}]}  inicia um turno
//        `images` é OPCIONAL e só o motor claude a consome hoje: o SDK aceita
//        `MessageParam` com blocos (text|image|document), então a imagem vai
//        estruturada, nunca em base64 no meio do texto.
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

import * as readline from "node:readline";
// A política de permissão (cerca de leitura, rede e destrutivo) mora em módulo próprio —
// é a única forma de ela ter teste, já que este arquivo roda ao ser importado (F-13/F-29).
import { EDICAO, LEITURA, PATH_ARG, decidir } from "./politica.mjs";

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
// `--version` responde e SAI. Sem isto o `engine_status()` do desktop rodava o
// runner com essa flag, ele ignorava, subia em modo host, recebia EOF no stdin e
// saía — devolvendo NDJSON de arranque que a ponte leria como se fosse o número
// da versão. Uma sonda que responde qualquer coisa é pior que uma que não
// responde: a de cima o desktop já sabe classificar ("encontrado, mas não
// respondeu"); a outra vira lixo exibido como fato.
if (process.argv.includes("--version")) {
  const { readFileSync } = await import("node:fs");
  const { dirname, resolve } = await import("node:path");
  const { fileURLToPath } = await import("node:url");
  let v = "desconhecida";
  try {
    const raiz = dirname(fileURLToPath(import.meta.url));
    v = JSON.parse(readFileSync(resolve(raiz, "package.json"), "utf8")).version ?? v;
  } catch { /* instalação sem o package.json ao lado: o desktop mostra o estado, não adivinha */ }
  process.stdout.write(`claude-runner ${v}\n`);
  process.exit(0);
}

// ------------------------------------------------------- subscription auth, normalized
//
// This block used to sit BELOW the `--modelos` branch, which exits the process — so model
// discovery ran through a `query()` with `ANTHROPIC_API_KEY` still in the environment
// while turns ran without it. Two authentications in one runner, and the divergence was
// invisible: both answer, and the catalog an API key returns is plausible.
//
// It now runs before EVERY SDK entry point. `--version` stays above it because it answers
// without the SDK at all (that is the whole point of the dynamic import below), and
// stripping an env var to print a version number would be work with no reader.
//
// The removal is from THIS process only — the parent's environment is untouched. We never
// SET the key: the SDK gives it precedence over the subscription, which would silently
// turn a Pro/Max session into pay-per-token billing. The login belongs to the official
// client; this runner never embeds it.
if (process.env.ANTHROPIC_API_KEY) {
  emit({
    type: "warn",
    message:
      "ANTHROPIC_API_KEY presente no ambiente — removida DESTE processo para forçar auth por assinatura (claude login).",
  });
  delete process.env.ANTHROPIC_API_KEY;
}

// O SDK entra por import DINÂMICO, e só depois do `--version` acima. Com o
// `import` estático de antes, a resolução do pacote acontecia ANTES de qualquer
// linha nossa rodar — então numa instalação sem `npm install` o runner morria com
// ERR_MODULE_NOT_FOUND e o desktop lia isso como "binário corrompido", quando o
// diagnóstico certo é "está lá, faltam as dependências". Agora `--version`
// responde mesmo sem o SDK, que é exatamente quando o diagnóstico é mais útil.
const { query } = await import("@anthropic-ai/claude-agent-sdk");

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

// ------------------------------------------------- gates pendentes (tool_use_id → resolve)
const pendingGates = new Map();

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

  // Um único ponto de decisão, e ele é testável: `politica.mjs`. A ordem lá é cerca ANTES
  // de atalho — leitura só é automática dentro da pasta e fora da denylist de segredos;
  // saída para a rede e comando destrutivo sempre pedem cartão, em qualquer nível.
  //
  // ⚠️ A frase que estava aqui — "o que NENHUM nível libera é o que sai da pasta do
  // projeto" — descrevia uma cerca que **não existia**: `Read` de caminho absoluto e
  // `WebFetch` de qualquer URL eram automáticos, e os dois juntos são a cadeia de
  // exfiltração inteira (F-13). Agora a frase é verdade, e tem teste.
  const { acao, motivo } = decidir({
    projectDir: PROJECT_DIR,
    toolName,
    toolInput,
    nivel: APROVACAO,
    leitura: LEITURA,
    edicao: EDICAO,
  });
  if (acao === "allow") {
    return allowDecision(motivo);
  }
  return gate(id, toolName, toolInput, motivo);
}

/** Emite o cartão e BLOQUEIA até o usuário decidir. `motivo` aparece no log do runner. */
async function gate(id, toolName, toolInput, motivo) {
  if (motivo) process.stderr.write(`[gate] ${toolName}: ${motivo}\n`);
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

/**
 * Monta o `prompt` do `query()`.
 *
 * Sem imagem, continua sendo **string** — o caminho que rodou até aqui, e trocá-lo
 * por iterável "para uniformizar" mudaria o comportamento de todo turno de texto
 * por causa de um caso que ainda não aconteceu.
 *
 * Com imagem, vira um `AsyncIterable<SDKUserMessage>` de **um item só**. Cabe
 * porque o runner já cria um `query()` POR TURNO com `resume: sessionId` — não é
 * uma sessão de streaming, é um turno que por acaso aceita iterável.
 *
 * Ordem dos blocos: imagens ANTES do texto. É a mesma escolha do anexo de arquivo
 * no Modo Code — a última coisa que o modelo lê é o que se está pedindo.
 */
function montarPrompt(text, images) {
  if (!images || !images.length) return text;

  const blocos = images.map((im) => ({
    type: "image",
    source: { type: "base64", media_type: String(im.mime || "image/png"), data: String(im.dataBase64 || "") },
  }));
  // Texto vazio não vira bloco vazio: o SDK rejeita `text: ""`, e uma imagem colada
  // sem pedido é um pedido legítimo ("olha isto").
  if (text && text.trim()) blocos.push({ type: "text", text });

  return (async function* () {
    yield {
      type: "user",
      message: { role: "user", content: blocos },
      parent_tool_use_id: null,
      session_id: sessionId ?? "",
    };
  })();
}

/**
 * Traduz UMA mensagem do Agent SDK nos eventos do protocolo desta casa
 * (docs/embedding.md). Pura: não emite nada, não lê nem escreve o mundo —
 * devolve `{eventos, estado}` e quem chama despacha.
 *
 * ⚠️ POR QUE ELA FOI EXTRAÍDA. Este era o trecho mais perigoso do runner e o
 * único sem prova: quando a forma de um evento do SDK muda, nenhum `case` casa,
 * **nada é emitido e nada falha** — a tela do Modo Code simplesmente emudece, e
 * o turno "termina" sem uma linha. Erro que não erra é a família de defeito que
 * esta casa passou a semana consertando, e aqui ela estava sem nenhuma régua.
 *
 * O estado que atravessa as mensagens é explícito de propósito:
 *  - `sessionId`  → vai no `resume` do turno seguinte; perdê-lo reinicia a
 *                   conversa em silêncio, com o modelo respondendo do zero;
 *  - `sawTextDelta` → o bloco `assistant` traz o texto COMPLETO. Emiti-lo depois
 *                   dos deltas duplicaria a resposta na tela; é fallback, não
 *                   caminho normal.
 *
 * @param {object} message   mensagem do SDK
 * @param {{sessionId: string|undefined, sawTextDelta: boolean}} estado
 * @param {string|undefined} modelo  o `--model` da linha de comando (fallback do rótulo)
 * @returns {{eventos: object[], estado: {sessionId: string|undefined, sawTextDelta: boolean}}}
 */
function traduzirMensagem(message, estado, modelo) {
  const eventos = [];
  let sessionId = estado.sessionId;
  let sawTextDelta = estado.sawTextDelta;

  switch (message?.type) {
    case "system":
      if (message.subtype === "init") {
        sessionId = message.session_id ?? sessionId;
        eventos.push({
          type: "model",
          model: message.model ?? message.data?.model ?? modelo ?? "claude",
          server: "anthropic (assinatura)",
        });
      }
      break;

    case "stream_event": {
      const ev = message.event;
      if (ev?.type === "content_block_delta" && ev.delta?.type === "text_delta") {
        sawTextDelta = true;
        eventos.push({ type: "text", delta: ev.delta.text });
      }
      break;
    }

    case "assistant": {
      for (const block of message.message?.content ?? []) {
        // fallback: se os deltas não vierem, emite o texto completo do bloco
        if (block.type === "text" && !sawTextDelta) {
          eventos.push({ type: "text", delta: block.text });
        }
        if (block.type === "tool_use") {
          eventos.push({ type: "tool_call", id: block.id, name: block.name, arguments: block.input });
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
          eventos.push({
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
      eventos.push({
        type: "usage",
        tokens: tin + tout,
        cost: message.total_cost_usd ?? 0,
        estimated: true, // sob assinatura o custo USD é indicativo, não faturado por token
      });
      // ⚠️ O SDK NUNCA manda `subtype: "error"` num `result`. Até a 1.4.38 era o
      // único subtipo que este `case` reconhecia, e ele não existe: medido nos tipos
      // da 0.3.258 (10/09/2026), um turno que morre num erro de API chega como
      // `success` com `is_error: true` e o texto em `result`; os outros desfechos
      // chegam como `error_during_execution`, `error_max_turns`,
      // `error_max_budget_usd` ou `error_max_structured_output_retries`, com a
      // lista em `errors`. Nenhum casava — e um 401 fechava a timeline como
      // `usage` + `turn_done`, sem linha de erro. É a família de defeito que a
      // prova deste arquivo existe para pegar: erro que não erra.
      //
      // Teto estourado (`maxTurns`, `maxBudgetUsd` — os tetos da run, B2 do
      // RUN-20260910) NÃO é erro: é o runner parando onde mandaram parar. Sai
      // como `warn`, a forma que o `anna` usa para o teto dele, e o turno fecha
      // normalmente — quem decide se continua é a página, e a linha diz por quê.
      const sub = String(message.subtype ?? "success");
      if (sub === "error_max_turns") {
        const n = message.num_turns != null ? ` (${message.num_turns})` : "";
        eventos.push({ type: "warn", message: `teto de iterações do turno atingido${n}: o agente parou aqui; mande outra mensagem para seguir` });
      } else if (sub === "error_max_budget_usd") {
        eventos.push({ type: "warn", message: "teto de custo do turno atingido: o agente parou aqui; mande outra mensagem para seguir" });
      } else if (sub !== "success" || message.is_error) {
        const lista = Array.isArray(message.errors) ? message.errors.filter(Boolean).map(String) : [];
        const texto = lista.length ? lista.join(" · ") : (message.result ? String(message.result) : `erro no turno (${sub})`);
        eventos.push({ type: "error", message: texto });
      }
      eventos.push({ type: "turn_done" });
      break;
    }
  }

  return { eventos, estado: { sessionId, sawTextDelta } };
}

async function runTurn(text, images) {
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

  // O estado atravessa as mensagens do turno: `sessionId` para o `resume` do
  // turno seguinte, `sawTextDelta` para o fallback de texto do bloco assistant.
  let estado = { sessionId, sawTextDelta: false };

  for await (const message of query({ prompt: montarPrompt(text, images), options })) {
    const r = traduzirMensagem(message, estado, MODEL);
    estado = r.estado;
    sessionId = estado.sessionId;
    for (const ev of r.eventos) emit(ev);
  }
}

async function pump() {
  if (busy) return;
  const item = queue.shift();
  if (item === undefined) return;
  busy = true;
  try {
    await runTurn(item.text, item.images);
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
    // A fila guardava STRING. Com imagem isso a perderia: o turno enfileirado
    // sairia depois sem os blocos, bem-formado e sem a figura — silêncio com cara
    // de sucesso. Guarda o PAR, e a imagem viaja presa ao pedido que a trouxe.
    queue.push({ text: String(msg.text ?? ""), images: Array.isArray(msg.images) ? msg.images : [] });
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
