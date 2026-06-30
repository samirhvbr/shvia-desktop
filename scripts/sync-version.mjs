#!/usr/bin/env node
// Sincroniza a versão do app a partir de version.md (fonte única da verdade)
// para os arquivos que carregam versão: package.json, tauri.conf.json,
// Cargo.toml e os lock files. Idempotente (só escreve o que muda), sem
// dependências (Node puro). Roda no `prebuild` (npm) e à mão via
// `npm run version:sync`. Modelado no sync-version.mjs do SHVTERM.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), ".."); // raiz do repo
const read = (p) => readFileSync(resolve(ROOT, p), "utf8");

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
    /(name = "shvia-desktop"\nversion = ")\d+\.\d+\.\d+(")/,
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
  if (next !== text) {
    writeFileSync(resolve(ROOT, file), next);
    console.log(`[sync-version] ${file} → ${version}`);
    changed++;
  }
}
console.log(`[sync-version] ${version} (${changed} arquivo(s) atualizado(s))`);
