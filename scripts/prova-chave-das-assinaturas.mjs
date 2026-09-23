#!/usr/bin/env node
/**
 * No release ships signed by a key the clients do not trust, and "deferred" is not "skipped"
 * (1.6.21).
 *
 * Two holes in the 1.6.3 guard, both measured in build-local.sh:
 *   1. On a fresh clone the key proof ran BEFORE `npm ci` — no Tauri CLI yet — printed
 *      "prova de assinatura adiada" and returned 0; nothing ran it again, so the abort of an
 *      unmeasured publish was skipped entirely.
 *   2. The reuse path never runs the proof at all.
 * Fixes: the build re-runs the proof after `npm ci`, and before the upload the keyid of EVERY
 * signature in release.json is compared with the pubkey compiled into the clients.
 *
 * This runs the real functions, cut out of the script, in a temp dir with a crafted
 * tauri.conf.json and release.json. No real key is involved: a keyid is bytes 2..10 of the
 * decoded key or signature, and that is all the check reads.
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const FONTE = readFileSync(join(RAIZ, "build-local.sh"), "utf8");
function funcao(nome) {
  const i = FONTE.indexOf(`${nome}() {`);
  if (i < 0) { console.error(`🔴 ${nome} moved out of build-local.sh`); process.exit(1); }
  return FONTE.slice(i, FONTE.indexOf("\n}\n", i) + 3);
}
const BLOCO = ["updater_pubkey_id", "veredito_das_assinaturas", "confere_chaves_do_manifesto"].map(funcao).join("\n");
const VERIFY = funcao("verify_updater_key");

const b64 = (b) => Buffer.from(b).toString("base64");
const id = (hex) => Buffer.from(hex, "hex");
const pubkey = (hex) => b64(`untrusted comment: minisign public key\n${b64(Buffer.concat([Buffer.from("Ed"), id(hex), Buffer.alloc(32, 7)]))}\n`);
const assinatura = (hex) => b64(`untrusted comment: signature\n${b64(Buffer.concat([Buffer.from("ED"), id(hex), Buffer.alloc(64, 9)]))}\ntrusted comment: t\n${b64(Buffer.alloc(64, 1))}\n`);
const plataforma = { darwin: "macos", linux: "linux", win32: "windows" }[process.platform];

function conferir(pub, artefatos) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-chaves-"));
  try {
    mkdirSync(join(dir, "src-tauri"));
    writeFileSync(join(dir, "src-tauri", "tauri.conf.json"), JSON.stringify({ plugins: { updater: { pubkey: pub } } }));
    writeFileSync(join(dir, "release.json"), JSON.stringify({ version: "9.9.9", platforms: { [plataforma]: { artifacts: artefatos } } }));
    execFileSync("bash", ["-c", `set -euo pipefail\n${BLOCO}\nconfere_chaves_do_manifesto`], { cwd: dir, stdio: "ignore" });
    return 0;
  } catch (e) { return e.status ?? 1; } finally { rmSync(dir, { recursive: true, force: true }); }
}

function verificar(args, publish) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-adiada-"));
  try {
    const script = `set -euo pipefail\nPUBLISH=${publish}\n${VERIFY}\nverify_updater_key ${args}\necho "ADIADA=\${PROVA_DA_CHAVE_ADIADA:-0}"`;
    const out = execFileSync("bash", ["-c", script], { cwd: dir, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
    return { status: 0, adiada: out.includes("ADIADA=1") };
  } catch (e) { return { status: e.status ?? 1, adiada: false }; } finally { rmSync(dir, { recursive: true, force: true }); }
}

const K = "0123456789ABCDEF", OUTRA = "FEDCBA9876543210";
const CASOS = [
  ["todas as assinaturas da chave publicada", () => conferir(pubkey(K), [{ file: "a", signature: assinatura(K) }, { file: "b", signature: assinatura(K) }]), 0],
  ["🔴 uma assinatura de OUTRO par", () => conferir(pubkey(K), [{ file: "a", signature: assinatura(K) }, { file: "b", signature: assinatura(OUTRA) }]), 1],
  ["🔴 assinatura ilegível", () => conferir(pubkey(K), [{ file: "a", signature: "lixo" }]), 1],
  ["🔴 pubkey ilegível com assinatura presente", () => conferir("lixo", [{ file: "a", signature: assinatura(K) }]), 1],
  ["nada assinado nesta plataforma (outro aviso cuida)", () => conferir(pubkey(K), [{ file: "a" }]), 0],
  ["sem CLI antes do npm ci: adia e marca", () => { const r = verificar("", 1); return r.status === 0 && r.adiada ? 0 : 1; }, 0],
  ["🔴 sem CLI DEPOIS do npm ci, publicando: aborta", () => verificar("depois-do-npm-ci", 1).status, 1],
  ["sem CLI depois do npm ci, build local: avisa e segue", () => verificar("depois-do-npm-ci", 0).status, 0],
];

let falhas = 0;
for (const [nome, rodar, esperado] of CASOS) {
  const obtido = rodar();
  if ((obtido === 0) !== (esperado === 0)) {
    falhas++;
    console.error(`🔴 ${nome}: esperado ${esperado === 0 ? "segue" : "ABORTA"}, obtido status=${obtido}`);
  }
}
if (falhas) {
  console.error(`\n${falhas} de ${CASOS.length} casos falharam: um release pode sair assinado por uma chave que`);
  console.error("nenhum cliente aceita, ou a prova da chave voltou a ser pulada.");
  process.exit(1);
}
console.log(`[chave-das-assinaturas] ${CASOS.length} casos · toda assinatura que sobe é da chave publicada, e "adiada" não é "pulada"`);
