#!/usr/bin/env node
/**
 * A `cd` out of the project does not outlive its own command (1.6.51).
 *
 * The page decides a Bash card by reading its text: in its Auto mode (the default) a `confirm`
 * card it judges "inside the project" is approved by nobody. A relative path reads as inside,
 * so `cat hosts` is approved on sight — which is only right if the command really runs in the
 * project. Two things of the SDK's make that true, and this proof measures both with the REAL
 * runner and the REAL SDK binary, against a local stand-in for the Messages API that scripts
 * the model (nothing leaves 127.0.0.1, no tokens are spent):
 *
 *   1. after a command that leaves the allowed directories, the SDK puts the shell back in the
 *      project and says so ("Shell cwd was reset to <project>"), so the next command, in the
 *      same turn, runs in the project;
 *   2. the runner opens each turn with `resume`, and the cwd starts from `cwd` again. SDK
 *      0.3.270s made a `cd` persist across the turns of ONE streaming session; the runner does
 *      not use one (1.6.48 read that changelog line as a gap; this measured it).
 *
 * A control keeps the first measurement honest: `cd sub`, inside the project, must persist to
 * the next command. If an SDK stopped persisting any `cd` at all, "the next command ran in the
 * project" would pass for the wrong reason.
 *
 * Needs `claude-runner/node_modules` (`npm ci` in claude-runner). Without it this prints
 * NOT MEASURED and exits 0 locally, but exits 1 in CI (CI=true).
 */
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, sep } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const RUNNER = join(RAIZ, "claude-runner", "claude-runner.mjs");
if (!existsSync(join(RAIZ, "claude-runner", "node_modules", "@anthropic-ai", "claude-agent-sdk"))) {
  if (process.env.CI) { console.error("🔴 the runner's SDK is not installed in CI: run `npm ci` in claude-runner first"); process.exit(1); }
  console.log("[cd] NOT MEASURED: claude-runner/node_modules is missing (npm ci in claude-runner). CI runs it.");
  process.exit(0);
}

const base = realpathSync(mkdtempSync(join(tmpdir(), "shvia-cd-")));
const PROJ = join(base, "proj");
const FORA = join(base, "fora");
for (const d of [join(PROJ, "sub"), FORA, join(base, "home"), join(base, "config")]) mkdirSync(d, { recursive: true });

// ── the scripted model: turn 1 = `cd <target>` then `pwd`; turn 2 = `pwd` ─────────────────────
let script = null;
let n = 0;
function reply(body, block, stop) {
  const msg = { id: `msg_${++n}`, type: "message", role: "assistant", model: body.model || "stand-in",
    content: [], stop_reason: null, stop_sequence: null, usage: { input_tokens: 5, output_tokens: 1 } };
  if (!body.stream) return { json: { ...msg, content: [block], stop_reason: stop, usage: { input_tokens: 5, output_tokens: 5 } } };
  const start = block.type === "text" ? { type: "text", text: "" } : { type: "tool_use", id: block.id, name: block.name, input: {} };
  const delta = block.type === "text"
    ? { type: "text_delta", text: block.text }
    : { type: "input_json_delta", partial_json: JSON.stringify(block.input) };
  return { sse: [
    { type: "message_start", message: msg },
    { type: "content_block_start", index: 0, content_block: start },
    { type: "content_block_delta", index: 0, delta },
    { type: "content_block_stop", index: 0 },
    { type: "message_delta", delta: { stop_reason: stop, stop_sequence: null }, usage: { output_tokens: 5 } },
    { type: "message_stop" },
  ] };
}
const textOf = (m) => typeof m.content === "string" ? m.content
  : (m.content || []).filter((b) => b.type === "text").map((b) => b.text).join("\n");

const server = http.createServer((req, res) => {
  let raw = "";
  req.on("data", (d) => (raw += d));
  req.on("end", () => {
    let body = {};
    try { body = JSON.parse(raw || "{}"); } catch { /* not JSON: answered below as a side call */ }
    if (req.method !== "POST" || !req.url.startsWith("/v1/messages") || req.url.includes("count_tokens")) {
      res.writeHead(200, { "content-type": "application/json" });
      return res.end(JSON.stringify(req.url.includes("count_tokens") ? { input_tokens: 5 } : {}));
    }
    let out;
    const agentLoop = Array.isArray(body.tools) && body.tools.some((t) => t.name === "Bash");
    if (!agentLoop) {
      out = reply(body, { type: "text", text: "ok" }, "end_turn");
    } else {
      // The turn is named by the last user message carrying a TURN-n marker; the step is how
      // many assistant messages came after it.
      const msgs = body.messages || [];
      let at = -1, turn = null;
      for (let i = msgs.length - 1; i >= 0 && !turn; i--) {
        const hit = msgs[i].role === "user" && textOf(msgs[i]).match(/TURN-(\d)/);
        if (hit) { at = i; turn = hit[1]; }
      }
      const step = msgs.slice(at + 1).filter((m) => m.role === "assistant").length;
      const cmd = turn ? script[turn]?.[step] : undefined;
      out = cmd
        ? reply(body, { type: "tool_use", id: `toolu_${turn}_${step}_${n}`, name: "Bash", input: { command: cmd, description: "proof" } }, "tool_use")
        : reply(body, { type: "text", text: "done" }, "end_turn");
    }
    if (out.json) { res.writeHead(200, { "content-type": "application/json" }); return res.end(JSON.stringify(out.json)); }
    res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache" });
    for (const e of out.sse) res.write(`event: ${e.type}\ndata: ${JSON.stringify(e)}\n\n`);
    res.end();
  });
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));

/** Runs two turns through the real runner, approving every card. Returns the Bash results in order. */
function run(target) {
  script = { 1: [`cd ${target}`, "pwd"], 2: ["pwd"] };
  const env = { ...process.env };
  delete env.ANTHROPIC_API_KEY;
  Object.assign(env, {
    ANTHROPIC_BASE_URL: `http://127.0.0.1:${server.address().port}`,
    ANTHROPIC_AUTH_TOKEN: "stand-in, not a credential",
    CLAUDE_CONFIG_DIR: join(base, "config"),
    HOME: join(base, "home"),
    CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: "1",
    DISABLE_AUTOUPDATER: "1",
    DISABLE_TELEMETRY: "1",
    DISABLE_ERROR_REPORTING: "1",
  });
  // `manual`: every Bash command becomes a `confirm` card, the case where the page decides.
  const child = spawn(process.execPath, [RUNNER, "--cwd", PROJ, "--aprovacao", "manual"], { env, stdio: ["pipe", "pipe", "pipe"] });
  const send = (o) => child.stdin.write(JSON.stringify(o) + "\n");
  const cards = [];
  const results = [];
  let turns = 0, buf = "", stderr = "";
  child.stderr.on("data", (d) => (stderr += d));
  return new Promise((resolve) => {
    const limit = setTimeout(() => child.kill("SIGKILL"), 90_000);
    child.stdout.on("data", (d) => {
      buf += d;
      for (let i; (i = buf.indexOf("\n")) >= 0;) {
        const line = buf.slice(0, i); buf = buf.slice(i + 1);
        let ev; try { ev = JSON.parse(line); } catch { continue; }
        if (ev.type === "gate_request") { cards.push(ev); send({ id: ev.id, decision: "approve" }); }
        if (ev.type === "tool_result") results.push(String(ev.content).trim());
        if (ev.type === "error") results.push(`ERROR ${ev.message}`);
        if (ev.type === "turn_done") send(++turns === 1 ? { type: "user", text: "TURN-2" } : { type: "exit" });
      }
    });
    child.on("exit", () => { clearTimeout(limit); resolve({ cards, results, turns, stderr }); });
    send({ type: "user", text: "TURN-1" });
  });
}

const falhas = [];
const inside = (p) => p === PROJ || p.startsWith(PROJ + sep);
let ctl;
try {
  const out = await run(FORA);
  ctl = await run("sub");
  const shown = (r) => JSON.stringify(r.results);
  if (out.turns !== 2 || ctl.turns !== 2) {
    falhas.push(`the runner did not finish two turns (${out.turns}, ${ctl.turns}): ${out.stderr.slice(-600)}`);
  } else {
    const [cdOut, pwdOut, pwdOut2] = out.results;
    const [, pwdCtl, pwdCtl2] = ctl.results;
    // The card for the `cd` itself must show where it goes: that is what the page judges.
    if (!out.cards[0] || out.cards[0].policy !== "confirm" || !String(out.cards[0].preview?.command).includes(FORA)) {
      falhas.push(`the \`cd\` card does not show its target to the page: ${JSON.stringify(out.cards[0])}`);
    }
    if (pwdCtl !== join(PROJ, "sub")) {
      falhas.push(`control: \`cd sub\` did not reach the next command (${shown(ctl)}). Without persistence the checks below prove nothing`);
    }
    if (pwdOut !== PROJ) {
      falhas.push(`after an approved \`cd\` out of the project the next command ran in ${pwdOut}, not the project (${shown(out)}): a relative path there is outside while the page reads it as inside`);
    }
    if (!/Shell cwd was reset to /.test(cdOut ?? "")) {
      falhas.push(`the SDK no longer says it reset the shell (${JSON.stringify(cdOut)}): check how it keeps the cwd now`);
    }
    if (pwdOut2 !== PROJ) falhas.push(`the next turn started in ${pwdOut2}, not the project`);
    if (!inside(pwdCtl2 ?? "")) falhas.push(`after \`cd sub\` the next turn started outside the project: ${pwdCtl2}`);
  }
} finally {
  server.close();
  // Cleanup is not the measurement, and must never be the verdict. In CI on 24/09/2026 (#71)
  // this rmSync threw ENOTEMPTY — an entry appeared in `base` while it was being removed — and
  // the proof died before printing anything. The cause is NOT known: 10 measured local runs
  // passed, and no process with `base` in its environment was alive after the runner exited
  // (probed at 0, 50, 200 and 1000 ms). So it retries, and a leftover directory is reported.
  try {
    rmSync(base, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  } catch (e) {
    console.warn(`[cd] temporary directory left behind: ${base} (${e.code ?? e.message})`);
  }
}
if (falhas.length) {
  for (const f of falhas) console.error(`🔴 ${f}`);
  console.error(`\n${falhas.length} problem(s): a Bash card the page approves as "inside the project" may run outside it.`);
  process.exit(1);
}
console.log(`[cd] the shell returns to the project after a \`cd\` out of it, and each turn starts there · the control persists within the turn, ${ctl.results[2] === PROJ ? "not" : "and also"} across turns`);
