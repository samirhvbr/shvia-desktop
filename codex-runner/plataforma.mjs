// plataforma.mjs — what the Codex runner does differently per OS (1.6.58). Pure functions with
// the machine passed in, so `plataforma.test.mjs` runs the Windows cases on Linux: CI never runs
// the runner on Windows, and these are the only place the Windows branches execute before the
// owner's runbook.

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
