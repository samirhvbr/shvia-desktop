import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const [evidence] = process.argv.slice(2);
const endpoint = "http://127.0.0.1:19383";
const origin = "https://ai.shvia.org";
const pageState = `({
  origin: location.origin,
  path: location.pathname,
  title: document.title,
  ready: document.readyState,
  hasLogin: Boolean(document.querySelector('form.auth-form [name="password"]')),
  viewport: { width: innerWidth, height: innerHeight }
})`;

function inspect(socketUrl) {
  return new Promise((resolve, reject) => {
    const url = new URL(socketUrl);
    if (url.origin !== "ws://127.0.0.1:19383") {
      reject(new Error("Unexpected WebView2 debugger endpoint"));
      return;
    }
    const socket = new WebSocket(url);
    const timeout = setTimeout(() => finish(reject, new Error("WebView2 response timed out")), 4_000);
    function finish(done, value) {
      clearTimeout(timeout);
      socket.close();
      done(value);
    }
    socket.addEventListener("error", () => finish(reject, new Error("WebView2 connection failed")));
    socket.addEventListener("open", () => {
      socket.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: {
        expression: pageState, returnByValue: true,
      } }));
    });
    let state;
    socket.addEventListener("message", ({ data }) => {
      const response = JSON.parse(data);
      if (response.id === 1) {
        state = response.result?.result?.value;
        if (state?.origin !== origin || !state.hasLogin || state.ready !== "complete" ||
            state.viewport.width <= 0 || state.viewport.height <= 0) {
          finish(reject, new Error("ShvIA login is still loading"));
          return;
        }
        socket.send(JSON.stringify({ id: 2, method: "Page.captureScreenshot" }));
      }
      if (response.id === 2) finish(resolve, { state, screenshot: response.result?.data });
    });
  });
}

const deadline = Date.now() + 90_000;
let lastResult = "WebView2 debugger has not appeared";
while (Date.now() < deadline) {
  try {
    const response = await fetch(endpoint + "/json/list", { signal: AbortSignal.timeout(2_000) });
    const pages = (await response.json()).filter((target) => target.type === "page");
    const target = pages.find((page) => page.url.startsWith(origin + "/"));
    if (!target) throw new Error("ShvIA target unavailable. WebView2 pages: " + pages.map(({ url }) => url.split("?")[0]).join(", "));
    const { state, screenshot } = await inspect(target.webSocketDebuggerUrl);
    if (!screenshot) throw new Error("WebView2 did not return a screenshot");
    await writeFile(join(evidence, "login.json"), JSON.stringify(state, null, 2) + "\n");
    await writeFile(join(evidence, "login.png"), Buffer.from(screenshot, "base64"));
    console.log("Verified live WebView2 window:", state);
    process.exit(0);
  } catch (error) {
    lastResult = error.message;
    await delay(2_000);
  }
}
throw new Error(lastResult);
