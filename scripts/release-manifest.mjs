#!/usr/bin/env node
// Gera checksums e o release.json dos instaladores (item D9).
//
// ── Por que isto existe ──────────────────────────────────────────────────────
// Até aqui o build saía sem **um único checksum** em nenhuma plataforma: quem
// baixava o .dmg ou o .msi não tinha como verificar que recebeu o que foi
// empacotado. E o item D1 (auto-update) precisa de um **manifesto** para saber
// que versão existe e onde baixá-la — o `release.json` É esse manifesto.
//
// ── A restrição que define o desenho: um build por MÁQUINA ────────────────────
// A CI foi removida na 0.4.6 (custo) e o build é 100% local. Então cada SO é
// empacotado numa máquina diferente, e nenhuma delas vê os artefatos das outras.
//
// Consequência: este script **MESCLA**. Ele lê o release.json que já existe,
// atualiza SÓ a plataforma do build atual e preserva as outras. Sobrescrever
// faria o build do Windows apagar a entrada do macOS — e o D1 leria um manifesto
// que promete uma plataforma só.
//
// A mesma versão é o critério de mesclagem: se o release.json encontrado é de uma
// versão ANTERIOR, ele é descartado inteiro. Misturar artefatos de versões
// diferentes no mesmo manifesto é pior que recomeçar, porque o updater baixaria
// 0.15.0 no macOS e 0.14.0 no Windows achando que são a mesma release.
//
// Node puro, sem dependência — igual ao sync-version.mjs e ao git-sync.mjs.
//
// USO:
//   node scripts/release-manifest.mjs                 # lê version.md, detecta o SO
//   node scripts/release-manifest.mjs --out dist/     # copia os artefatos p/ uma pasta
//   node scripts/release-manifest.mjs --print         # só imprime, não escreve
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const BUNDLE = join(ROOT, "src-tauri/target/release/bundle");
const MANIFEST = join(ROOT, "release.json");

const args = process.argv.slice(2);
const flag = (n) => args.includes(n);
const opt = (n) => {
  const i = args.indexOf(n);
  return i >= 0 && args[i + 1] ? args[i + 1] : null;
};

// ── versão: a mesma fonte única do resto do repo ─────────────────────────────
const version = readFileSync(join(ROOT, "version.md"), "utf8").trim();
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(`[release-manifest] versão inválida em version.md: ${JSON.stringify(version)}`);
  process.exit(1);
}

// ── plataforma: derivada do SO que está rodando, não de argumento ────────────
// Passar a plataforma por flag convidaria a rodar o script do macOS declarando
// "windows" e gravar hash de artefato que não existe naquele SO.
const PLATAFORMA = { darwin: "macos", win32: "windows", linux: "linux" }[process.platform];
if (!PLATAFORMA) {
  console.error(`[release-manifest] SO não suportado: ${process.platform}`);
  process.exit(1);
}

// Extensões que interessam por plataforma. `.app.tar.gz` é o formato que o
// updater do Tauri consome no macOS; o `.dmg` é o que humano baixa.
const EXTENSOES = {
  macos: [".dmg", ".app.tar.gz"],
  windows: [".msi", "-setup.exe"],
  linux: [".deb", ".AppImage", ".rpm"],
}[PLATAFORMA];

function encontrarArtefatos(dir, achados = []) {
  if (!existsSync(dir)) return achados;
  for (const entrada of readdirSync(dir, { withFileTypes: true })) {
    const caminho = join(dir, entrada.name);
    if (entrada.isDirectory()) {
      encontrarArtefatos(caminho, achados);
    } else if (EXTENSOES.some((ext) => entrada.name.endsWith(ext))) {
      achados.push(caminho);
    }
  }
  return achados;
}

// Só o que é DESTA versão. Os build scripts limpam o bundle dir antes de
// empacotar, mas este script também roda à mão — e um artefato de outra versão
// esquecido ali entraria no manifesto e faria o updater (D1) oferecer o arquivo
// ERRADO. Pego rodando: um ShvIA_0.8.2.dmg antigo apareceu no manifesto da 0.15.0.
//
// Nome SEM versão (ex.: `ShvIA.app.tar.gz`, que é como o Tauri gera o bundle do
// updater no macOS) passa: não há como ser de outra versão se não declara nenhuma.
const OUTRA_VERSAO = /\d+\.\d+\.\d+/;
const daVersaoAtual = (nome) => nome.includes(version) || !OUTRA_VERSAO.test(nome);

const todos = encontrarArtefatos(BUNDLE);
const descartados = todos.filter((p) => !daVersaoAtual(basename(p)));
const artefatos = todos.filter((p) => daVersaoAtual(basename(p)));

for (const p of descartados) {
  console.warn(`[release-manifest] ignorado (não é da ${version}): ${basename(p)}`);
}

if (artefatos.length === 0) {
  // Aviso, não erro: o script roda no fim do build, e derrubar o build inteiro
  // porque o manifesto não achou artefato transformaria um problema de
  // empacotamento num problema de manifesto.
  console.warn(`[release-manifest] nenhum artefato ${EXTENSOES.join("/")} em ${BUNDLE} — nada a fazer.`);
  process.exit(0);
}

const sha256 = (p) => createHash("sha256").update(readFileSync(p)).digest("hex");

// ── entradas da plataforma atual ─────────────────────────────────────────────
const destino = opt("--out") ? resolve(ROOT, opt("--out")) : null;
if (destino) mkdirSync(destino, { recursive: true });

const entradas = artefatos
  .map((caminho) => {
    const nome = basename(caminho);
    const hash = sha256(caminho);

    // Um `.sha256` AO LADO do artefato, no formato que o `sha256sum -c` e o
    // `shasum -a 256 -c` leem direto. Quem baixa não precisa saber que existe um
    // release.json para verificar um arquivo.
    writeFileSync(`${caminho}.sha256`, `${hash}  ${nome}\n`);

    if (destino) {
      copyFileSync(caminho, join(destino, nome));
      copyFileSync(`${caminho}.sha256`, join(destino, `${nome}.sha256`));
    }

    return { file: nome, size: statSync(caminho).size, sha256: hash };
  })
  .sort((a, b) => a.file.localeCompare(b.file));

// ── mescla com o manifesto existente ─────────────────────────────────────────
let manifesto = { version, generated_at: null, platforms: {} };

if (existsSync(MANIFEST)) {
  try {
    const anterior = JSON.parse(readFileSync(MANIFEST, "utf8"));
    if (anterior?.version === version && anterior?.platforms) {
      manifesto = anterior;
    } else if (anterior?.version) {
      console.log(`[release-manifest] release.json era da ${anterior.version}; recomeçando para a ${version}.`);
    }
  } catch {
    console.warn("[release-manifest] release.json ilegível — recomeçando.");
  }
}

manifesto.version = version;
// A data é regravada a cada build porque o manifesto é o de "a release 0.15.0
// como ela está agora", e a última plataforma empacotada é a informação útil.
manifesto.generated_at = new Date().toISOString();
manifesto.platforms[PLATAFORMA] = { artifacts: entradas };

if (flag("--print")) {
  console.log(JSON.stringify(manifesto, null, 2));
  process.exit(0);
}

writeFileSync(MANIFEST, `${JSON.stringify(manifesto, null, 2)}\n`);

const faltando = ["macos", "windows", "linux"].filter((p) => !manifesto.platforms[p]);

console.log(`[release-manifest] ${version} · ${PLATAFORMA}: ${entradas.length} artefato(s)`);
for (const e of entradas) {
  console.log(`  ${e.sha256.slice(0, 16)}…  ${(e.size / 1048576).toFixed(1)} MB  ${e.file}`);
}
if (destino) console.log(`  copiados para ${destino}`);
// Dizer o que FALTA é o ponto do manifesto mesclado: sem este aviso, publicar a
// release com uma plataforma só é um erro silencioso.
if (faltando.length > 0) {
  console.log(`  ⚠️ ainda sem artefato: ${faltando.join(", ")} — rode o build nessas máquinas antes de publicar.`);
} else {
  console.log("  ✓ as três plataformas estão no manifesto.");
}
