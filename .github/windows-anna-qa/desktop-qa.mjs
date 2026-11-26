import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import runAnnaUiSteps from "./anna-ui.mjs";

const origin = "https://ai.shvia.org";
const debuggerOrigin = "http://127.0.0.1:19383";
const { QA_EMAIL: email, QA_PASSWORD: password, QA_FIXTURE: folder, QA_MARKER: marker } = process.env;
delete process.env.QA_EMAIL;
delete process.env.QA_PASSWORD;
assert(email && password && folder && marker && /^\d+$/.test(process.env.QA_DESKTOP_PID), "Missing test configuration");
const exec = promisify(execFile);

async function waitFor(label, test, timeout = 50_000) {
  const deadline = Date.now() + timeout;
  do {
    const result = await test();
    if (result) return result;
    await delay(500);
  } while (Date.now() < deadline);
  throw new Error("Timed out waiting for " + label);
}

async function target() {
  try {
    const response = await fetch(debuggerOrigin + "/json/list", { signal: AbortSignal.timeout(2_000) });
    const pages = await response.json();
    return pages.find((page) => page.type === "page" && page.url.startsWith(origin + "/"));
  } catch {
    return undefined;
  }
}

const page = await waitFor("the public ShvIA page in the installed WebView2", target);
const endpoint = new URL(page.webSocketDebuggerUrl);
assert.equal(endpoint.origin, "ws://127.0.0.1:19383");
const socket = new WebSocket(endpoint);
const open = new Promise((resolve, reject) => {
  const timeout = setTimeout(() => reject(new Error("WebView2 diagnostic socket did not open")), 8_000);
  socket.addEventListener("open", () => { clearTimeout(timeout); resolve(); }, { once: true });
  socket.addEventListener("error", () => { clearTimeout(timeout); reject(new Error("WebView2 connection failed")); }, { once: true });
});
await open;

let id = 0;
const pending = new Map();
socket.addEventListener("message", ({ data }) => {
  const response = JSON.parse(data);
  const callback = pending.get(response.id);
  if (callback) callback(response);
});

function evaluate(expression) {
  return new Promise((resolve, reject) => {
    const currentId = ++id;
    const timeout = setTimeout(() => {
      pending.delete(currentId);
      reject(new Error("WebView2 page operation timed out"));
    }, 9_000);
    pending.set(currentId, (reply) => {
      clearTimeout(timeout);
      pending.delete(currentId);
      if (reply.error || reply.result?.exceptionDetails) {
        reject(new Error("WebView2 could not complete a page operation"));
      } else {
        resolve(reply.result?.result?.value);
      }
    });
    socket.send(JSON.stringify({ id: currentId, method: "Runtime.evaluate", params: {
      expression, returnByValue: true,
    } }));
  });
}
const pageCall = (fn, ...args) => evaluate("(" + fn.toString() + ")(..." + JSON.stringify(args) + ")");

try {
  await waitFor("the login form on the public origin", () => pageCall((expected) =>
    location.origin === expected && !!document.querySelector("form.auth-form [name=password]"),
  origin));
  assert(await pageCall((expected, user, secret) => {
    if (location.origin !== expected) return false;
    const form = document.querySelector("form.auth-form");
    if (!form || new URL(form.action).origin !== expected) return false;
    form.elements.email.value = user;
    form.elements.password.value = secret;
    form.requestSubmit();
    return true;
  }, origin, email, password), "Could not submit the login form");
  await waitFor("an authenticated view with Code", async () => {
    const current = await target();
    if (!current || current.id !== page.id) return false;
    try {
      return await pageCall((expected) => location.origin === expected &&
        !!document.querySelector(".workspace-shell.app-shell button[data-m=code]"), origin);
    } catch {
      return false;
    }
  });
  console.log("Signed in through the public ShvIA login page in the installed Desktop.");

  const chooseFolder = async (path) => {
    const picker = fileURLToPath(new URL("./choose-folder.ps1", import.meta.url));
    const { stdout } = await exec("powershell.exe",
      ["-NoProfile", "-STA", "-File", picker, "-AppId", process.env.QA_DESKTOP_PID, "-ProjectFolder", path],
      { timeout: 35_000 });
    console.log(stdout.trim());
  };
  const proof = await runAnnaUiSteps(evaluate, chooseFolder, { folder, marker });
  assert(proof.matched && proof.observedReadTool);
  console.log("Completed Code mode with the synthetic account and a read-only local task.");
} finally {
  socket.close();
}
