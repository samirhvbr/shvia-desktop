// plataforma.mjs — what the Codex runner does differently per OS (1.6.58). Pure functions with
// the machine passed in, so `plataforma.test.mjs` runs the Windows cases on Linux: CI never runs
// the runner on Windows, and these are the only place the Windows branches execute before the
// owner's runbook.

import path from "node:path";

/**
 * How to start `codex`: `{ comando, prefixo }`, spawned as `comando ...prefixo app-server --stdio`.
 *
 * On Linux and macOS it is `codex`, as before. On Windows the npm install of the Codex CLI is a
 * `codex.cmd` shim (`%APPDATA%\npm`), and Node does not run a `.cmd` without a shell: ENOENT,
 * and EINVAL since Node 20.12.2. So the shim is resolved to what it runs — `node` with
 * `node_modules\@openai\codex\bin\codex.js` next to it — and a native `codex.exe` on PATH wins.
 * `SHVIA_CODEX_BIN` still overrides everything, as it did.
 */
export function resolverCodex({ platform = process.platform, env = process.env, existe, node = process.execPath } = {}) {
  if (env.SHVIA_CODEX_BIN) return { comando: env.SHVIA_CODEX_BIN, prefixo: [] };
  if (platform !== "win32") return { comando: "codex", prefixo: [] };
  const dirs = String(env.Path ?? env.PATH ?? "").split(";").filter(Boolean);
  for (const d of dirs) {
    const exe = path.win32.join(d, "codex.exe");
    if (existe(exe)) return { comando: exe, prefixo: [] };
    const js = path.win32.join(d, "node_modules", "@openai", "codex", "bin", "codex.js");
    if (existe(path.win32.join(d, "codex.cmd")) && existe(js)) return { comando: node, prefixo: [js] };
  }
  return { comando: "codex", prefixo: [] };
}

/**
 * The two commands of the startup sandbox proof, both run under `workspaceWrite`:
 *
 *   - `controle` writes INSIDE the project, which the sandbox allows. It must exit 0.
 *   - `prova` writes OUTSIDE it, in the user's home. The sandbox must refuse it.
 *
 * 🔴 The control is what makes a refusal mean something. Until 1.6.58 there was only the probe,
 * and any non-zero exit read as "the sandbox held". Measured on 24/09/2026 with the real
 * `codex` app-server and the probe pointed at a program that does not exist — which is what
 * `/bin/sh` is on Windows: `exitCode: 101`, "Failed to execvp … No such file or directory", and
 * the runner started, sandbox "confirmed". A command that never ran proved the sandbox.
 */
export function comandosDaProva({ platform = process.platform, fora, dentro }) {
  if (platform === "win32") {
    // cmd.exe is on every Windows; `del` removes the file so nothing is left behind.
    const cmd = (alvo) => ["cmd.exe", "/d", "/c", `echo x>"${alvo}" && del /q "${alvo}"`];
    return { controle: cmd(dentro), prova: cmd(fora) };
  }
  const sh = (alvo) => ["/bin/sh", "-c", `printf x > '${alvo}' && rm -f '${alvo}'`];
  return { controle: sh(dentro), prova: sh(fora) };
}
