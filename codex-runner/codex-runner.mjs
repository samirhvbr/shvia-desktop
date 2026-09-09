#!/usr/bin/env node
// codex-runner.mjs — the "Codex (ChatGPT subscription)" engine for SHVIA's Code mode.
//
// Drives `codex app-server --stdio` and speaks EXACTLY the same NDJSON protocol as
// `anna` (SHVIA-CODE/docs/embedding.md) and `claude-runner`. That is what makes it
// drop-in behind `code_bridge.rs`: the bridge and the Code-mode cards do not change,
// only who produces the NDJSON.
//
// AUTH: the credentials `codex login` stored (ChatGPT Go/Plus/Pro subscription).
// This runner never embeds a login and never sets `OPENAI_API_KEY` — the same
// stance the Claude runner takes, and for the same reason: an API key would
// silently move the user from their subscription to pay-per-token.
//
// ## Why app-server and not `codex exec --json`
//
// Measured on 09/09/2026. `codex exec` has no per-action approval: it takes one
// sandbox policy up front (`read-only` | `workspace-write` | `danger-full-access`)
// and the only mention of confirmation in its help is the flag that SKIPS it. An
// engine built on it would emit no `gate_request`, the approval pill would be
// decorative, and "nothing runs or writes without the dev seeing it" would quietly
// stop being true for one engine. A control that looks like it works is the worst
// outcome available.
//
// `codex app-server` carries the blocking approval requests instead
// (`item/commandExecution/requestApproval`, `item/fileChange/requestApproval`), so
// the SHVIA card stays the single source of permission truth. The cost is that the
// interface is marked `[experimental]` by OpenAI — accepted deliberately, because
// the alternative is an engine that lies.
//
// ## Contract (embedding.md)
//   Host → runner (stdin, one JSON line per message):
//     {"type":"user","text":"..."}                        starts a turn
//     {"type":"exit"}                                     shuts down
//     {"id":"...","decision":"approve"|"always"|"reject"} answers a gate_request
//   runner → Host (stdout):
//     {"type":"model","model","server"}
//     {"type":"text","delta"}
//     {"type":"tool_call","id","name","arguments"}
//     {"type":"tool_result","id","name","bytes","content"}
//     {"type":"gate_request","id","scope","policy","preview"}   (BLOCKS)
//     {"type":"usage","tokens","cost","estimated"}
//     {"type":"turn_done"} | {"type":"error"|"warn","message"}

import { spawn } from "node:child_process";
import * as readline from "node:readline";
import { validarPayload } from "./esquema.mjs";
// 🔴 IMPORTADO do motor vizinho, nunca copiado. A lista de segredos é a mesma nos dois
// motores porque é a MESMA lista — duas cópias divergem no dia em que alguém acrescenta
// um padrão a uma delas, e a que fica para trás segue verde sem proteger nada.
import { caminhoProibido } from "../claude-runner/politica.mjs";
import {
  PEDIDOS_QUE_BLOQUEIAM,
  decisaoParaResposta,
  ehDecisao,
  pedidoParaGate,
  POLITICA,
  traduzirNotificacao,
} from "./protocolo.mjs";

// ---------------------------------------------------------------- NDJSON output
function emit(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

// ------------------------------------------------------------------- arguments
function argOf(flag) {
  const i = process.argv.indexOf(flag);
  return i >= 0 && i + 1 < process.argv.length ? process.argv[i + 1] : undefined;
}

// `--version` answers and EXITS. Without it `engine_status()` on the desktop runs
// the runner with that flag, it comes up in host mode, gets EOF on stdin and
// leaves — handing the bridge start-up NDJSON that would be displayed as if it
// were the version. The Claude runner learned this the hard way; a probe that
// answers anything is worse than one that does not answer, because "found but
// silent" is a state the desktop already knows how to label.
if (process.argv.includes("--version")) {
  const { readFileSync } = await import("node:fs");
  const { dirname, resolve } = await import("node:path");
  const { fileURLToPath } = await import("node:url");
  let v = "0.0.0";
  try {
    const raiz = dirname(fileURLToPath(import.meta.url));
    v = JSON.parse(readFileSync(resolve(raiz, "package.json"), "utf8")).version ?? v;
  } catch { /* keeps the default: the version is a nicety, the probe is not */ }
  process.stdout.write(v + "\n");
  process.exit(0);
}

const PROJECT_DIR = argOf("--cwd") || process.cwd();
const MODEL = argOf("--model");
// `--aprovacao` is accepted and IGNORED, on purpose: the bridge passes it to every
// engine, and refusing to start over a flag that cannot change anything would break
// the spawn for no gain. The engine has one policy; see POLITICA for why.
const { askForApproval, sandboxMode } = POLITICA;

// ------------------------------------------------------- the app-server child
const codexBin = process.env.SHVIA_CODEX_BIN || "codex";
const child = spawn(codexBin, ["app-server", "--stdio"], {
  cwd: PROJECT_DIR,
  stdio: ["pipe", "pipe", "pipe"],
  env: process.env,
});

child.on("error", (e) => {
  // `não encontrado` in the message is what the bridge already matches on to fall
  // back to the gateway engine — same wording as the Claude runner, on purpose.
  emit({ type: "error", message: `codex não encontrado no PATH (${e.code ?? e.message}).` });
  process.exit(1);
});

// The app-server logs to stderr. It is diagnostics, not protocol — forwarding it
// as `warn` would put Rust log lines in the user's timeline.
child.stderr.on("data", (d) => process.stderr.write(d));

// ------------------------------------------------------- JSON-RPC over stdio
//
// Newline-delimited, no `Content-Length` framing. Measured on 09/09/2026 by
// sending one `initialize` line and reading the answer back — not assumed from
// the LSP family, which frames with headers and would have hung here forever.
let proximoId = 1;
const pendentes = new Map(); // our request id → resolve

function pedir(method, params) {
  // 🔴 The payload is checked against Codex's OWN schema before it leaves. On its
  // first run this caught `sandboxMode` in `thread/start` — a field that does not
  // exist (it is `sandbox`), which the server had been ignoring in silence since the
  // first line of this runner. The policy I believed I was setting was never set;
  // what I measured was Codex's default. A wrong field does not fail, it is
  // DISCARDED, and that is exactly the failure this guard exists for.
  const problemas = validarPayload(method, params);
  if (problemas.length) {
    emit({ type: "error", message: `payload inválido em ${method}: ${problemas.join("; ")}` });
    throw new Error(`payload inválido em ${method}`);
  }
  const id = proximoId++;
  const linha = JSON.stringify({ jsonrpc: "2.0", id, method, params });
  if (process.env.SHVIA_CODEX_DEBUG) process.stderr.write("[->] " + linha + "\n");
  child.stdin.write(linha + "\n");
  return new Promise((resolve) => pendentes.set(id, resolve));
}
function responder(id, result) {
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, result }) + "\n");
}

// ------------------------------------------------------------- pending gates
// JSON-RPC id of a blocking request → the function that answers it. Same shape as
// the Claude runner's `pendingGates`, so the host side of the contract is identical.
const gatesPendentes = new Map();

// ------------------------------------------------------------------ the turn
let threadId = null;
let ocupado = false;
// Resolves when `turn/completed` arrives. 🔴 It exists because `turn/start`
// ANSWERS IMMEDIATELY — it is an ack that the turn was accepted, not the end of
// it. The first smoke run proved it: the runner emitted `model`, awaited
// `turn/start`, saw it resolve, considered the turn finished and exited before a
// single token streamed. In pipe mode (stdin already at EOF) that means one line
// of output and a clean exit code — a failure that looks like success.
let fimDoTurno = null;
let stdinFechado = false;
const fila = [];

async function garantirThread() {
  if (threadId) return threadId;
  const r = await pedir("thread/start", {
    cwd: PROJECT_DIR,
    ...(MODEL ? { model: MODEL } : {}),
    approvalPolicy: askForApproval,
    sandbox: sandboxMode,
  });
  threadId = r?.threadId ?? r?.thread?.id ?? null;
  if (!threadId) {
    emit({ type: "error", message: "app-server did not return a threadId on thread/start." });
  }
  return threadId;
}

async function rodarTurno(texto) {
  ocupado = true;
  try {
    const tid = await garantirThread();
    if (!tid) return;
    emit({ type: "model", model: MODEL || "codex", server: "chatgpt" });
    const acabou = new Promise((resolve) => { fimDoTurno = resolve; });
    await pedir("turn/start", { threadId: tid, input: [{ type: "text", text: texto }] });
    // The ack came back; now WAIT for the turn itself.
    await acabou;
  } catch (e) {
    emit({ type: "error", message: String(e?.message ?? e) });
  } finally {
    fimDoTurno = null;
    ocupado = false;
    drenar();
  }
}

function drenar() {
  if (ocupado) return;
  const proximo = fila.shift();
  if (proximo !== undefined) { rodarTurno(proximo); return; }
  if (stdinFechado) encerrar(0);
}

// ------------------------------------------------- reading from the app-server
// `SHVIA_CODEX_DEBUG=1` dumps the raw wire to stderr. It exists because the first
// gate run reported `error: unknown error` and the NDJSON had thrown away the only
// copy of what the server actually said — a translation layer that cannot show its
// input is a layer nobody can debug.
const DEBUG = !!process.env.SHVIA_CODEX_DEBUG;

readline.createInterface({ input: child.stdout }).on("line", async (linha) => {
  const s = linha.trim();
  if (DEBUG) process.stderr.write("[<-] " + s.slice(0, 2000) + "\n");
  if (!s.startsWith("{")) return;
  let msg;
  try { msg = JSON.parse(s); } catch { return; }

  // 1. an answer to something we asked
  if (msg.id !== undefined && (msg.result !== undefined || msg.error !== undefined)) {
    const resolve = pendentes.get(msg.id);
    if (resolve) {
      pendentes.delete(msg.id);
      if (msg.error) emit({ type: "error", message: String(msg.error?.message ?? "app-server error") });
      resolve(msg.result ?? null);
    }
    return;
  }

  // 2. a request FROM the server that blocks until we answer — the gate
  if (msg.id !== undefined && PEDIDOS_QUE_BLOQUEIAM.has(msg.method)) {
    const cartao = pedidoParaGate(msg);
    if (!cartao) { responder(msg.id, decisaoParaResposta(msg.method, "reject")); return; }
    emit(cartao);
    const decisao = await new Promise((resolve) => gatesPendentes.set(String(msg.id), resolve));
    responder(msg.id, decisaoParaResposta(msg.method, decisao));
    return;
  }

  // 3. any other server request: answer so the turn does not hang. Silence here
  //    reads as a freeze on screen, which is the failure mode hardest to diagnose.
  if (msg.id !== undefined && msg.method) { responder(msg.id, {}); return; }

  // 4. a notification
  const evento = traduzirNotificacao(msg);
  if (evento) emit(evento);

  // The turn ends on `turn/completed`, and `turn/failed`/`error` has to release it
  // too — otherwise a failing turn hangs the runner forever waiting for a
  // completion that is never coming, which is the same freeze from the other side.
  if (msg.method === "turn/completed" || msg.method === "turn/failed") {
    emit({ type: "turn_done" });
    if (fimDoTurno) fimDoTurno();
  } else if (msg.method === "error" && msg.params?.willRetry !== true && fimDoTurno) {
    // 🔴 `willRetry` decides, and getting this wrong KILLED a turn that was fine.
    // The app-server reports a reconnect as `error` with `willRetry: true`
    // ("Reconnecting... 2/5"). The first version ended the turn on any error, the
    // host sent `exit`, and the runner shut down mid-retry. The turn did not fail
    // — we aborted it, and the log said the model was unreachable.
    emit({ type: "turn_done" });
    fimDoTurno();
  }
});

// ----------------------------------------------------------- reading the host
//
// 🔴 Wired only AFTER the startup ruler passes, and that ordering is the guard.
// While this ran at import time, a host line arriving before the probe finished was
// acted on immediately: `{"type":"exit"}` shut the runner down mid-probe (which is
// how the reversion proof caught it), and a `{"type":"user"}` would have run a turn
// BEFORE the sandbox was ever proven. A ruler the engine can outrun is not a ruler.
function ligarEntradaDoHost() {
readline.createInterface({ input: process.stdin })
  .on("line", (linha) => {
    const s = linha.trim();
    if (!s) return;
    let msg;
    try { msg = JSON.parse(s); } catch { return; }

    if (ehDecisao(msg)) {
      const resolve = gatesPendentes.get(String(msg.id));
      if (resolve) { gatesPendentes.delete(String(msg.id)); resolve(msg.decision); }
      return;
    }
    if (msg.type === "exit") { encerrar(0); return; }
    if (msg.type === "user") {
      const texto = String(msg.text ?? "");
      if (ocupado) fila.push(texto); else rodarTurno(texto);
    }
  })
  .on("close", () => { stdinFechado = true; drenar(); });
}

function encerrar(code) {
  // Every open card is REJECTED on the way out. Leaving them unanswered would
  // leave the app-server blocked on a decision nobody can give any more, and the
  // safe default when the person is gone is "no".
  for (const [, resolve] of gatesPendentes) resolve("reject");
  gatesPendentes.clear();
  try { child.stdin.end(); } catch { /* already gone */ }
  try { child.kill(); } catch { /* already gone */ }
  process.exit(code);
}

// Handshake first: the app-server answers `initialize` before it accepts anything
// else, and identifying the client is what makes this runner recognisable in
// Codex's own logs.
await pedir("initialize", {
  clientInfo: { name: "shvia-codex-runner", title: "ShvIA Code mode", version: "1.0.0" },
});

/**
 * 🔴 STARTUP RULER — the engine refuses to run if the sandbox does not hold.
 *
 * The whole promise of this engine is the BOUNDARY: inside the project it writes
 * freely, and to step outside it has to ask. That promise rests on Codex's sandbox
 * actually stopping an outside write — and nothing in the handshake reports whether
 * it does. `thread/start` accepts the policy and answers OK either way.
 *
 * So this measures instead of trusting: it asks the server to write a file OUTSIDE
 * the workspace, under the same policy the turns will use, and requires the attempt
 * to FAIL. If it succeeds, the sandbox is not enforcing, every later gate is
 * theatre, and the runner exits rather than serving an engine whose approval pill
 * would be a lie.
 *
 * `command/exec` is the right primitive because it runs "in the server sandbox
 * without creating a thread or turn" — no model call, so this costs no inference
 * and no turn latency, only a local process. It runs once per spawn.
 *
 * ⚠️ Calibrated toward the ALARM: an app-server that cannot run the probe at all
 * (method missing on an older Codex, transport error) also refuses to start. "I
 * could not measure" must never read as "it is clean" — the same reason
 * `prova-committer-nao-derruba.sh` exits 2 instead of 0.
 */
async function provarQueOSandboxSegura() {
  // 🔴 The target is inside $HOME, and choosing it took a FAILED reversion proof.
  //
  // The first version wrote to the workspace's PARENT, under /tmp — and passed with
  // the sandbox turned OFF, because `workspace-write` allows /tmp by design. The
  // probe was measuring a permitted write and calling it enforcement: a ruler that
  // is always green, which is worse than none because it looks like one.
  //
  // $HOME discriminates: the user owns it, so an OS permission error is impossible
  // and a failure there can only be the sandbox. Measured both ways on 09/09/2026 —
  // `workspaceWrite` → exit 2, "Read-only file system"; `dangerFullAccess` → exit 0
  // and the file appears.
  const alvo = `${process.env.HOME || "/root"}/.shvia-sandbox-probe-${process.pid}`;
  let r;
  try {
    r = await pedir("command/exec", {
      command: ["/bin/sh", "-c", `printf x > '${alvo}' && rm -f '${alvo}'`],
      cwd: PROJECT_DIR,
      sandboxPolicy: { type: "workspaceWrite", networkAccess: false },
      timeoutMs: 15000,
    });
  } catch (e) {
    emit({ type: "error", message: `não consegui provar o sandbox: ${e?.message ?? e}` });
    return false;
  }
  if (!r || typeof r.exitCode !== "number") {
    emit({
      type: "error",
      message: "o app-server não respondeu à prova de sandbox — este Codex é velho demais para este motor.",
    });
    return false;
  }
  if (r.exitCode === 0) {
    emit({
      type: "error",
      message:
        "o sandbox do Codex NÃO segurou uma escrita fora do projeto — a garantia deste motor "
        + "(fronteira com cartão) não vale nesta máquina, então ele não sobe.",
    });
    return false;
  }
  return true;
}

/**
 * Avisa que este projeto tem arquivo com cara de segredo — ANTES do primeiro turno.
 *
 * 🔴 Por que existe: entre os motores, o Codex tem exatamente UM buraco medido, e é este.
 * `claude-runner` recusa ler `.env`, `.git/`, chave e credencial em QUALQUER nível, pelo
 * `caminhoProibido` que este arquivo importa. O Codex lê e devolve o valor sem cartão
 * nenhum — medido em 09/09/2026, `cat .env` respondeu com o conteúdo e nenhum
 * `gate_request` foi emitido. Não há configuração que mude isso (não existe superfície de
 * execpolicy para o integrador) e o runner também não alcança: ele fica sabendo do comando
 * pelo `item/started`, que chega **depois** de o comando começar.
 *
 * Então o que sobra é dizer. Antes, não depois.
 *
 * ⚠️ `warn`, NUNCA `exit`. A distinção é de natureza, não de severidade: sandbox que não
 * segura QUEBRA a garantia do motor e por isso derruba o arranque (código
 * `SAIDA_SANDBOX_NAO_CONFIRMADO`); um `.env` na pasta é **condição normal de projeto** —
 * quase todo projeto tem um. Recusar subir por isso tornaria o motor inutilizável e
 * ensinaria a ignorar o aviso, que é o desfecho pior dos dois.
 *
 * Varre só o primeiro nível e para em 200 entradas: é aviso, não auditoria, e roda a cada
 * spawn. Uma varredura recursiva num monorepo seguraria o arranque para achar o que a
 * primeira dúzia já teria mostrado.
 */
async function avisarSobreSegredosNaPasta() {
  const { readdir } = await import("node:fs/promises");
  let entradas;
  try {
    entradas = await readdir(PROJECT_DIR, { withFileTypes: true });
  } catch {
    return; // pasta ilegível é problema de outro caminho; não é aqui que se descobre
  }
  const achados = [];
  for (const e of entradas.slice(0, 200)) {
    if (achados.length >= 5) break;
    if (caminhoProibido(e.name)) achados.push(e.name);
  }
  if (!achados.length) return;
  emit({
    type: "warn",
    message:
      `este projeto tem ${achados.length === 1 ? "um arquivo" : "arquivos"} com cara de segredo `
      + `(${achados.join(", ")}). O motor Codex LÊ sem perguntar — ele não tem cartão para `
      + `leitura. Troque de motor ou tire o arquivo da pasta antes de começar.`,
  });
}

if (!(await provarQueOSandboxSegura())) {
  try { child.kill(); } catch { /* já foi */ }
  process.exit(3);
}

// O sandbox segurou. O aviso vem ANTES de o host poder mandar turno: um alerta que
// chega depois do primeiro `cat .env` não é alerta, é registro.
await avisarSobreSegredosNaPasta();

// Só agora o host tem voz.
ligarEntradaDoHost();
