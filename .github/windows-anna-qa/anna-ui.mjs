import { setTimeout as sleep } from "node:timers/promises";
import assert from "node:assert/strict";

// evaluate(expression) returns the value of a page-side expression via CDP.
// chooseFolder(path) selects the directory in the native Windows picker.
export async function runAnnaUiSteps(evaluate, chooseFolder, { folder, marker, timeoutMs = 180_000 }) {
  assert.match(folder, /^[a-z]:[\\/]/i, "Use a disposable absolute Windows directory");
  assert.match(marker, /^[A-Za-z0-9_-]{12,128}$/, "Use a unique synthetic marker");
  const run = (fn, ...args) => evaluate("(" + fn.toString() + ")(..." + JSON.stringify(args) + ")");
  const wait = async (label, fn, timeout, ...args) => {
    const stop = Date.now() + timeout;
    do {
      const value = await run(fn, ...args);
      if (value) return value;
      await sleep(350);
    } while (Date.now() < stop);
    throw new Error("Timed out waiting for " + label);
  };
  const click = (selector) => run((selector) => {
    const el = document.querySelector(selector);
    if (!el || el.disabled || !el.getClientRects().length) return false;
    el.click();
    return true;
  }, selector);

  await wait("the signed-in ShvIA Desktop dashboard", () =>
    location.origin === "https://ai.shvia.org" &&
    !!document.querySelector(".workspace-shell.app-shell button[data-m='code']"), 45_000);
  assert(await click("button[data-m='code']"), "The Code mode button is unavailable");

  await wait("the local project folder button", () => {
    if (document.querySelector(".workspace-shell.app-shell").dataset.mode !== "code" ||
        window.shviaProjectScope?.() !== "code") return false;
    return [...document.querySelectorAll("#code-folder button.cf-btn")].some((b) =>
      b.getClientRects().length && /^(Link the project folder|Vincular pasta do projeto)$/i.test(b.textContent.trim()));
  }, 30_000);
  assert(await run(() => {
    // The folder choice would rebind a current Code project, so require an unused test account.
    if (window.shviaActiveProject?.()?.type === "code") return false;
    const buttons = [...document.querySelectorAll("#code-folder button.cf-btn")];
    const button = buttons.find((b) => b.getClientRects().length &&
      /^(Link the project folder|Vincular pasta do projeto)$/i.test(b.textContent.trim()));
    if (!button) return false;
    button.click();
    return true;
  }), "The isolated account already has an active Code project or has no folder picker");
  await chooseFolder(folder);

  const projectId = await wait("the selected local folder to be the active Code project", (expected) => {
    const normalize = (path) => String(path ?? "").replaceAll("/", "\\").replace(/\\+$/, "").toLowerCase();
    const project = window.shviaActiveProject?.();
    if (project?.type === "code" && project.execution_target === "local" &&
        normalize(project.path) === normalize(expected)) return project.id;
    return false;
  }, 40_000, folder);
  console.log("Code mode linked the disposable directory through the native picker.");

  assert(await click("#code-engine-seg button[data-value='gateway']"), "The SHVIA engine button is unavailable");
  await wait("the SHVIA gateway engine", () =>
    document.querySelector("#code-engine-seg")?.dataset.engine === "gateway", 15_000);

  const changedInfra = await run(() => {
    const selector = document.querySelector("#execution-mode-select");
    if (!selector || selector.disabled) return false;
    const local = [...selector.options].find((option) => option.value && !option.disabled && !option.hidden &&
      /^⚡/.test(option.textContent.trim()) && !/offline|maintenance|manutenção/i.test(option.textContent));
    if (!local || local.value === selector.value) return false;
    selector.value = local.value;
    selector.dispatchEvent(new Event("change", { bubbles: true }));
    return true;
  });
  if (changedInfra) await sleep(1_500);

  const model = await wait("an available tool-capable model in Code mode", () => {
    const infra = document.querySelector("#execution-mode-select");
    const model = document.querySelector("#profile-select");
    if (!infra || !model || infra.selectedOptions[0]?.disabled) return false;
    const optionIsUsable = (option) => option?.value && !option.disabled && !option.hidden &&
      !option.closest("optgroup")?.disabled && !/^(?:combo:|__shvia_)/.test(option.value) &&
      !/^(?:loading|carregando|select|selecione|no models|sem modelos)\b/i.test(option.value);
    if (!optionIsUsable(infra.selectedOptions[0])) return false;
    const chosen = [...model.options].filter(optionIsUsable);
    if (!chosen.length) return false;
    if (!optionIsUsable(model.selectedOptions[0])) {
      if (model.disabled) return false;
      model.value = chosen[0].value;
      model.dispatchEvent(new Event("change", { bubbles: true }));
    }
    return optionIsUsable(model.selectedOptions[0]) && {
      model: model.value,
      infra: infra.value,
    };
  }, 40_000);
  console.log("Code mode selected an available SHVIA model and infrastructure.");

  // Keep Manual, which never automatically approves a shell command or write in a new project.
  assert(await run(() => {
    const approval = document.querySelector("#code-approval-select");
    if (!approval || approval.disabled) return false;
    if (approval.value !== "manual") {
      approval.value = "manual";
      approval.dispatchEvent(new Event("change", { bubbles: true }));
    }
    return approval.value === "manual";
  }), "Could not confirm Manual approval for the read-only task");

  const baseline = await run(() => {
    const timeline = document.querySelector("#code-timeline");
    return {
      answers: timeline.querySelectorAll(".ct-assistant").length,
      tools: timeline.querySelectorAll(".ct-tool").length,
      errors: timeline.querySelectorAll(".ct-error").length,
    };
  });
  const prompt = "Read probe.txt in this project using the read_file tool. " +
    "Reply with exactly the one line it contains. Do not edit anything, run shell commands, or use the network.";
  assert(await run((text) => {
    const textbox = document.querySelector("#code-composer textarea");
    const send = document.querySelector("#code-composer .cc-send");
    if (!textbox || !textbox.getClientRects().length || !send || send.disabled) return false;
    textbox.focus();
    textbox.value = text;
    textbox.dispatchEvent(new Event("input", { bubbles: true }));
    send.click();
    return true;
  }, prompt), "Could not submit the read-only Anna task");
  console.log("Submitted the read-only task through the Code composer.");

  const outcome = await wait("Anna's completed reply", (before, expected) => {
    const timeline = document.querySelector("#code-timeline");
    const error = [...timeline.querySelectorAll(".ct-error")][before.errors];
    if (error) {
      const text = error.textContent;
      const category = /401|auth|credential|credencial/i.test(text) ? "authentication" :
        /403|permission|permissão|denied/i.test(text) ? "permission" :
        /429|quota|cota|rate.limit/i.test(text) ? "quota or rate limit" :
        /model|modelo|infra/i.test(text) ? "model or infrastructure" :
        /anna|engine|motor|agent|agente/i.test(text) ? "engine" : "unclassified";
      const detail = text.replace(/(?:Bearer\s+|shvia_[a-z]+_)[A-Za-z0-9_.-]+/gi, "[credential]")
        .replace(/\b[A-Za-z0-9_.+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g, "[email]")
        .replace(/\b[A-Za-z0-9_-]{24,}\b/g, "[opaque value]").trim().slice(0, 400);
      return { error: category, detail };
    }
    if (timeline.querySelector(".ct-card__foot .ct-btn--primary")) return { error: "unexpected approval request" };
    const answers = [...timeline.querySelectorAll(".ct-assistant")].slice(before.answers);
    const answer = answers.at(-1);
    const tools = [...timeline.querySelectorAll(".ct-tool")].slice(before.tools);
    const read = tools.some((tool) => /^read_file\b/.test(tool.textContent.trim()) && tool.textContent.includes("probe.txt"));
    const completed = !document.querySelector("#code-composer .cc-send")?.disabled;
    if (!answer || !completed) return false;
    const raw = String(answer._raw ?? answer.textContent ?? "").trim();
    const content = raw.replace(/^`([\s\S]+)`$/, "$1").trim();
    return { matched: content === expected, observedReadTool: read };
  }, timeoutMs, baseline, marker);
  assert(!outcome.error, "Anna Code mode reported " + outcome.error + ": " + outcome.detail);
  assert(outcome.observedReadTool, "The Code timeline did not show a read_file call for probe.txt");
  assert(outcome.matched, "Anna's final assistant answer did not exactly match the synthetic marker");
  console.log("Anna completed read_file and returned the unique marker from probe.txt.");
  return { projectId, model: model.model, infra: model.infra, matched: true, observedReadTool: true };
}

export default runAnnaUiSteps;
