#!/usr/bin/env node
// Sincroniza a versão do app a partir de version.md (fonte única da verdade)
// — e, desde a 1.1.21, PROVA que sincronizou: ao fim ele reconfere cada portador
// e SAI COM ERRO se algum discordar. Rode `--verificar` para conferir sem escrever
// (é o `npm run prova:bump`, para usar antes de commitar).
//
// ⚠️ POR QUE A PROVA EXISTE. Este script já sincronizava, e o bump saiu pela
// metade TRÊS vezes: 1.1.18→1.1.19 (os manifestos ficaram atrás), a 1.1.19 que
// existia para consertar isso, e a 1.1.20 (version.md e CHANGELOG na frente dos
// manifestos, travando a validação do makepkg). O motivo é que as duas saídas de
// exceção do laço abaixo eram `console.warn` + `continue`: arquivo ausente e
// padrão não encontrado viravam AVISO num log de build, e o build seguia verde.
// Portador que para de casar com o regex some da sincronia para sempre e nada
// acusa. Aviso perde para gesto — mesma regra do piso de versão do `anna` em
// `stage-anna.mjs`, e do `STANDING_ATIVO` nascer em `0`.
// para os arquivos que carregam versão: package.json, tauri.conf.json,
// Cargo.toml e os lock files. Idempotente (só escreve o que muda), sem
// dependências (Node puro). Roda no `prebuild` (npm) e à mão via
// `npm run version:sync`. Modelado no sync-version.mjs do SHVTERM.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), ".."); // raiz do repo
const read = (p) => readFileSync(resolve(ROOT, p), "utf8");
// `--verificar` não escreve: só afirma. Serve para rodar antes do commit, onde a
// deriva nasce — o build já sincroniza, e o que passava era a árvore COMMITADA.
const APENAS_VERIFICAR = process.argv.includes("--verificar");

const version = read("version.md").trim();
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(
    `[sync-version] versão inválida em version.md: ${JSON.stringify(version)}`,
  );
  process.exit(1);
}

// [arquivo, regex que captura o trecho a substituir, substituição]
const targets = [
  ["package.json", /("version"\s*:\s*")\d+\.\d+\.\d+(")/, `$1${version}$2`],
  [
    "src-tauri/tauri.conf.json",
    /("version"\s*:\s*")\d+\.\d+\.\d+(")/,
    `$1${version}$2`,
  ],
  [
    "src-tauri/Cargo.toml",
    /(^\s*version\s*=\s*")\d+\.\d+\.\d+(")/m,
    `$1${version}$2`,
  ],
  // Lock files (existem após install/build): ancorar no nome do nosso pacote
  // para não tocar nas versões das dependências.
  [
    "src-tauri/Cargo.lock",
    /(name = "shvia-desktop"\r?\nversion = ")\d+\.\d+\.\d+(")/,
    `$1${version}$2`,
  ],
  [
    "package-lock.json",
    /("name":\s*"shvia-desktop",\s*"version":\s*")\d+\.\d+\.\d+(")/g,
    `$1${version}$2`,
  ],
];

let changed = 0;
for (const [file, re, repl] of targets) {
  let text;
  try {
    text = read(file);
  } catch {
    console.warn(`[sync-version] pulei (ausente): ${file}`);
    continue;
  }
  if (!re.test(text)) {
    console.warn(`[sync-version] padrão de versão não encontrado em ${file}`);
    continue;
  }
  const next = text.replace(re, repl);
  if (next !== text && !APENAS_VERIFICAR) {
    writeFileSync(resolve(ROOT, file), next);
    console.log(`[sync-version] ${file} → ${version}`);
    changed++;
  }
}

// ─── A prova: os portadores CONCORDAM, ou o build cai ────────────────────────
// Reusa o MESMO par (regex, substituição) da sincronia: se aplicar a substituição
// não muda nada, o arquivo já está na versão certa. Sem segundo padrão para
// divergir calado — a lição da `lib/markdown-links.mjs` duplicada.
const problemas = [];
for (const [file, re, repl] of targets) {
  let text;
  try {
    text = read(file);
  } catch {
    problemas.push(`${file}: AUSENTE (era só um aviso até a 1.1.21)`);
    continue;
  }
  if (!re.test(text)) {
    problemas.push(`${file}: padrão de versão não encontrado — o arquivo saiu da sincronia sem ninguém notar`);
    continue;
  }
  if (text.replace(re, repl) !== text) {
    problemas.push(`${file}: NÃO está em ${version}`);
  }
}

if (problemas.length > 0) {
  console.error(`\n[sync-version] BUMP PELA METADE — ${problemas.length} portador(es) fora de ${version}:`);
  for (const p of problemas) console.error(`  · ${p}`);
  console.error("\nversion.md é a fonte da verdade. Conserte com `npm run version:sync`");
  console.error("e confira com `npm run prova:bump`. Terceira ocorrência disto travou a");
  console.error("validação do makepkg na 1.1.20 — o build cai aqui de propósito.");
  process.exit(1);
}
console.log(
  APENAS_VERIFICAR
    ? `[sync-version] ok: ${targets.length} portador(es) + version.md concordam em ${version}`
    : `[sync-version] ${version} (${changed} arquivo(s) atualizado(s)); ${targets.length} portador(es) conferidos`,
);
