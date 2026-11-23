// atualizar.mjs — bring the installed Codex CLI up to date through the manager that installed it (1.44.0).
//
// The app updates itself and its runners, and since 1.13.0 installs a missing runner; the Codex CLI
// the runner starts is the person's own install and nothing updated it. Measured on the owner's Mac on
// 23/11/2026: Desktop 1.43.2, runner 1.7.0, `codex-cli 0.163.0` from Homebrew, while npm and the Homebrew
// cask already had 0.171.0. The desktop now asks `codex-runner --atualizar-codex` once a day.
//
// The runner, not the desktop, does it, because only the runner knows which `codex` it starts
// (`resolverCodex`: `SHVIA_CODEX_BIN`, install dirs outside the app's PATH, the npm shim on Windows).
// The update goes through the SAME manager that installed it, read from where the binary really lives:
//
//   - Homebrew cask (`…/Caskroom/codex/…`)   → `brew upgrade --cask codex`
//   - Homebrew formula (`…/Cellar/codex/…`)  → `brew upgrade codex`
//   - npm global (`…/node_modules/@openai/codex/…`) → `npm install -g @openai/codex@<latest>`
//   - anything else (a standalone install, a checkout) → nothing is run; the desktop says a newer
//     version exists and how it was found.
//
// Pure functions with the machine passed in, like `plataforma.mjs`, so the tests run every case on any OS.

import path from "node:path";

/** `0.171.0` > `0.163.0`; non-numeric parts count as 0. */
export function maisNova(a, b) {
  const x = String(a).split(".").map((n) => parseInt(n, 10) || 0);
  const y = String(b).split(".").map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0);
  }
  return false;
}

/**
 * How the `codex` at `caminhoReal` (symlinks resolved) was installed, and the tool that updates it.
 * `existe` answers "is there a file here"; it picks the npm that owns the global prefix.
 */
export function comoFoiInstalado({ caminhoReal, platform = process.platform, existe = () => false }) {
  const p = String(caminhoReal ?? "");
  const sep = platform === "win32" ? path.win32 : path.posix;
  const normal = p.replaceAll("\\", "/");
  for (const [marca, metodo] of [["/Caskroom/codex/", "brew-cask"], ["/Cellar/codex/", "brew"]]) {
    const i = normal.indexOf(marca);
    if (i > 0) {
      const brew = `${normal.slice(0, i)}/bin/brew`;
      return { metodo, ferramenta: existe(brew) ? brew : "brew" };
    }
  }
  const npmMarca = "/node_modules/@openai/codex/";
  const j = normal.indexOf(npmMarca);
  if (j > 0) {
    // The global prefix: `<prefix>/lib/node_modules` on macOS and Linux, `<prefix>\node_modules` on Windows.
    const modulos = normal.slice(0, j);
    const prefixo = platform === "win32" ? modulos : modulos.replace(/\/lib$/, "");
    const candidato = platform === "win32" ? sep.join(prefixo, "npm.cmd") : `${prefixo}/bin/npm`;
    return { metodo: "npm", ferramenta: existe(candidato) ? candidato : platform === "win32" ? "npm.cmd" : "npm" };
  }
  return { metodo: "desconhecido", ferramenta: null };
}

/** The command that updates a `codex` installed by `metodo`, or null when this module runs nothing for it. */
export function comandoDeAtualizacao({ metodo, ferramenta, ultima }) {
  if (metodo === "brew-cask") return [ferramenta, ["upgrade", "--cask", "codex"]];
  if (metodo === "brew") return [ferramenta, ["upgrade", "codex"]];
  if (metodo === "npm" && ultima) return [ferramenta, ["install", "-g", `@openai/codex@${ultima}`]];
  return null;
}

/**
 * One pass. Every effect comes in through `m`, so the tests drive it with a fake machine:
 *   m.versao()            → the version of the `codex` the runner starts, or "" when none answers
 *   m.caminhoReal()       → its real path, or "" when it cannot be found
 *   m.ultimaNoNpm()       → the latest `@openai/codex` on the npm registry, or "" when it cannot ask
 *   m.rodar(cmd, args)    → { ok, saida } after running an update command
 *   m.existe(caminho)     → whether a file exists
 * Returns the one line the desktop reads: { resultado, atual, ultima?, novo?, metodo, erro? }.
 * `resultado`: em_dia | atualizado | sem_metodo | falhou | sem_codex.
 */
export function atualizarCodex(m, platform = process.platform) {
  const atual = m.versao();
  if (!atual) return { resultado: "sem_codex" };
  const { metodo, ferramenta } = comoFoiInstalado({ caminhoReal: m.caminhoReal(), platform, existe: m.existe });

  // Homebrew knows its own latest: `brew upgrade` is a no-op when the cask or formula is current, and the npm
  // registry can be ahead of the cask for a few hours, which must not read as "failed".
  if (metodo === "brew-cask" || metodo === "brew") {
    const [cmd, args] = comandoDeAtualizacao({ metodo, ferramenta });
    const r = m.rodar(cmd, args);
    const novo = m.versao();
    if (novo && maisNova(novo, atual)) return { resultado: "atualizado", atual, novo, metodo };
    if (!r.ok) return { resultado: "falhou", atual, metodo, erro: resumo(r.saida) };
    return { resultado: "em_dia", atual, metodo };
  }

  const ultima = m.ultimaNoNpm();
  if (!ultima || !maisNova(ultima, atual)) return { resultado: "em_dia", atual, ultima: ultima || undefined, metodo };
  if (metodo !== "npm") return { resultado: "sem_metodo", atual, ultima, metodo };

  const [cmd, args] = comandoDeAtualizacao({ metodo, ferramenta, ultima });
  const r = m.rodar(cmd, args);
  const novo = m.versao();
  if (novo && maisNova(novo, atual)) return { resultado: "atualizado", atual, ultima, novo, metodo };
  return { resultado: "falhou", atual, ultima, metodo, erro: resumo(r.saida) || "a versão não mudou" };
}

/** The last meaningful line of a tool's output, short enough for a notification. */
function resumo(saida) {
  const linhas = String(saida ?? "").split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
  const ultima = linhas.reverse().find((l) => /error|erro|denied|EACCES|EPERM|failed|falhou/i.test(l)) ?? linhas[0] ?? "";
  return ultima.slice(0, 200);
}
