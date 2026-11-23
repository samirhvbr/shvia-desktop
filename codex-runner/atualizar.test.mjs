// atualizar.test.mjs — the Codex update decides by where `codex` really lives, and never calls a stale run a failure.
import { test } from "node:test";
import assert from "node:assert/strict";
import { atualizarCodex, comoFoiInstalado, maisNova } from "./atualizar.mjs";

test("the manager comes from the real path of the binary", () => {
  const existe = (f) => ["/opt/homebrew/bin/brew", "/usr/local/bin/npm"].includes(f);
  assert.deepEqual(
    comoFoiInstalado({ caminhoReal: "/opt/homebrew/Caskroom/codex/0.163.0/codex-aarch64-apple-darwin", platform: "darwin", existe }),
    { metodo: "brew-cask", ferramenta: "/opt/homebrew/bin/brew" },
  );
  assert.deepEqual(
    comoFoiInstalado({ caminhoReal: "/home/linuxbrew/.linuxbrew/Cellar/codex/0.163.0/bin/codex", platform: "linux", existe }),
    { metodo: "brew", ferramenta: "brew" },
  );
  assert.deepEqual(
    comoFoiInstalado({ caminhoReal: "/usr/local/lib/node_modules/@openai/codex/bin/codex.js", platform: "darwin", existe }),
    { metodo: "npm", ferramenta: "/usr/local/bin/npm" },
  );
  assert.deepEqual(
    comoFoiInstalado({ caminhoReal: "C:\\Users\\a\\AppData\\Roaming\\npm\\node_modules\\@openai\\codex\\bin\\codex.js", platform: "win32", existe: () => false }),
    { metodo: "npm", ferramenta: "npm.cmd" },
  );
  // A standalone install (the Codex installer's own layout) is not ours to update.
  assert.deepEqual(
    comoFoiInstalado({ caminhoReal: "/home/a/.codex/packages/standalone/releases/0.164.0-x86_64-unknown-linux-musl/bin/codex", platform: "linux" }),
    { metodo: "desconhecido", ferramenta: null },
  );
});

test("versions compare by number, not by text", () => {
  assert.equal(maisNova("0.171.0", "0.163.0"), true);
  assert.equal(maisNova("0.163.10", "0.163.9"), true);
  assert.equal(maisNova("0.163.0", "0.163.0"), false);
  assert.equal(maisNova("0.99.0", "0.100.0"), false);
});

/** A fake machine: `versoes` is what `codex --version` answers before and after the update. */
function maquina({ caminho, versoes, ultima = "", ok = true, saida = "" }) {
  const rodados = [];
  let i = 0;
  return {
    rodados,
    versao: () => versoes[Math.min(i, versoes.length - 1)],
    caminhoReal: () => caminho,
    ultimaNoNpm: () => ultima,
    rodar: (cmd, args) => { rodados.push([cmd, ...args].join(" ")); i++; return { ok, saida }; },
    existe: () => false,
  };
}

test("Homebrew: upgrade through brew, report the new version", () => {
  const m = maquina({ caminho: "/opt/homebrew/Caskroom/codex/0.163.0/codex", versoes: ["0.163.0", "0.171.0"] });
  assert.deepEqual(atualizarCodex(m, "darwin"), { resultado: "atualizado", atual: "0.163.0", novo: "0.171.0", metodo: "brew-cask" });
  assert.deepEqual(m.rodados, ["brew upgrade --cask codex"]);
});

test("Homebrew with nothing newer yet is up to date, not a failure", () => {
  // The npm registry can be ahead of the cask for hours: brew decides, and the registry is not asked.
  const m = maquina({ caminho: "/opt/homebrew/Caskroom/codex/0.171.0/codex", versoes: ["0.171.0"], ultima: "0.172.0" });
  assert.deepEqual(atualizarCodex(m, "darwin"), { resultado: "em_dia", atual: "0.171.0", metodo: "brew-cask" });
});

test("Homebrew that fails says why", () => {
  const m = maquina({ caminho: "/opt/homebrew/Cellar/codex/0.163.0/bin/codex", versoes: ["0.163.0"], ok: false, saida: "==> Upgrading codex\nError: Permission denied @ rb_sysopen" });
  assert.deepEqual(atualizarCodex(m, "darwin"), { resultado: "falhou", atual: "0.163.0", metodo: "brew", erro: "Error: Permission denied @ rb_sysopen" });
});

test("npm: install the registry's latest only when it is newer", () => {
  const velho = maquina({ caminho: "/usr/lib/node_modules/@openai/codex/bin/codex.js", versoes: ["0.163.0", "0.171.0"], ultima: "0.171.0" });
  assert.equal(atualizarCodex(velho, "linux").resultado, "atualizado");
  assert.deepEqual(velho.rodados, ["npm install -g @openai/codex@0.171.0"]);

  const emDia = maquina({ caminho: "/usr/lib/node_modules/@openai/codex/bin/codex.js", versoes: ["0.171.0"], ultima: "0.171.0" });
  assert.equal(atualizarCodex(emDia, "linux").resultado, "em_dia");
  assert.deepEqual(emDia.rodados, []);
});

test("an install nobody here manages is reported, never touched", () => {
  const m = maquina({ caminho: "/home/a/.codex/packages/standalone/releases/0.164.0/bin/codex", versoes: ["0.164.0"], ultima: "0.171.0" });
  assert.deepEqual(atualizarCodex(m, "linux"), { resultado: "sem_metodo", atual: "0.164.0", ultima: "0.171.0", metodo: "desconhecido" });
  assert.deepEqual(m.rodados, []);
});

test("no codex, no registry: nothing to do and nothing run", () => {
  assert.deepEqual(atualizarCodex(maquina({ caminho: "", versoes: [""] }), "darwin"), { resultado: "sem_codex" });
  const semRede = maquina({ caminho: "/usr/lib/node_modules/@openai/codex/bin/codex.js", versoes: ["0.163.0"], ultima: "" });
  assert.equal(atualizarCodex(semRede, "linux").resultado, "em_dia");
  assert.deepEqual(semRede.rodados, []);
});
