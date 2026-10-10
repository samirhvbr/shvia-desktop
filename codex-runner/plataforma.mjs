// plataforma.mjs — what the Codex runner does differently per OS (1.6.58). Pure functions with
// the machine passed in, so `plataforma.test.mjs` runs the Windows cases on Linux: CI never runs
// the runner on Windows, and these are the only place the Windows branches execute before the
// owner's runbook.

import path from "node:path";

/**
 * How to start `codex`: `{ comando, prefixo }`, spawned as `comando ...prefixo app-server --stdio`.
 *
 * On Linux and macOS it is `codex` when that is on PATH, as before. When it is not, the usual
 * install dirs are tried and the first one that has it gives the absolute path: on a Mac
 * (09/10/2026) `codex` sat in `/opt/homebrew/bin` and a spawn with a minimal PATH — what an app
 * launched from the Dock may get — failed with ENOENT. Why the desktop's PATH lacked it was not
 * established; this makes the runner independent of that. Nothing found stays `codex`: the
 * spawn then fails with its own, honest error.
 *
 * On Windows the npm install of the Codex CLI is a
 * `codex.cmd` shim (`%APPDATA%\npm`), and Node does not run a `.cmd` without a shell: ENOENT,
 * and EINVAL since Node 20.12.2. So the shim is resolved to what it runs — `node` with
 * `node_modules\@openai\codex\bin\codex.js` next to it — and a native `codex.exe` on PATH wins.
 * `SHVIA_CODEX_BIN` still overrides everything, as it did.
 *
 * `existe` answers "is there a file here"; `listar` lists a directory (the version managers keep
 * one `bin` per Node version). Both are passed in so the tests run every OS on any OS.
 */
export function resolverCodex({
  platform = process.platform, env = process.env, existe, listar = () => [], home = env.HOME ?? "",
  node = process.execPath,
} = {}) {
  if (env.SHVIA_CODEX_BIN) return { comando: env.SHVIA_CODEX_BIN, prefixo: [] };
  if (platform !== "win32") {
    const noPath = String(env.PATH ?? "").split(":").filter(Boolean);
    if (noPath.some((d) => existe(path.posix.join(d, "codex")))) return { comando: "codex", prefixo: [] };
    for (const d of diretoriosDeInstalacao({ home, listar })) {
      const bin = path.posix.join(d, "codex");
      if (existe(bin)) return { comando: bin, prefixo: [] };
    }
    return { comando: "codex", prefixo: [] };
  }
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
 * Where a user-level install of `codex` lands on macOS and Linux, most likely first. The same
 * list the desktop adds to the agents' PATH (`user_env.rs`), plus nvm's per-version `bin`, newest
 * first — the shell's PATH is the only thing that knows which of those is the active one.
 */
export function diretoriosDeInstalacao({ home, listar }) {
  const dirs = ["/opt/homebrew/bin", "/usr/local/bin"];
  if (home) {
    dirs.push(`${home}/.local/bin`, `${home}/.cargo/bin`, `${home}/.bun/bin`, `${home}/.volta/bin`);
    const nvm = `${home}/.nvm/versions/node`;
    let versoes = [];
    try { versoes = listar(nvm); } catch { /* no nvm here */ }
    const numero = (v) => String(v).replace(/^v/, "").split(".").map((n) => Number(n) || 0);
    const maisNovaPrimeiro = (a, b) => {
      const x = numero(a), y = numero(b);
      for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (y[i] ?? 0) - (x[i] ?? 0);
      return 0;
    };
    for (const v of [...versoes].sort(maisNovaPrimeiro)) dirs.push(`${nvm}/${v}/bin`);
  }
  return dirs;
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
