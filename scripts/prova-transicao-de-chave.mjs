#!/usr/bin/env node
/**
 * Rotating the updater key takes one transition release, declared on purpose (1.6.40).
 *
 * The pubkey is compiled into every installed client, so the release that rotates it must be
 * signed with the OLD key (the one installed clients check) while it carries the NEW pubkey
 * (the one the clients it installs will check). `--transicao-de-chave` declares that release;
 * the key proofs then compare with the previous release's pubkey instead of tauri.conf.json.
 *
 * This runs the real functions, cut out of build-local.sh, in temp git repositories: a tagged
 * release with the old pubkey, then the transition commit with the new one. The Tauri CLI is a
 * fake that writes a signature carrying the keyid each case chooses; a keyid is bytes 2..10 of
 * the decoded key or signature, which is all these checks read. No real key is involved.
 */
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
const BLOCO = [
  "keyid_da_conf", "updater_pubkey_id", "updater_pubkey_id_anterior", "keyid_que_os_clientes_conferem",
  "confere_transicao_de_chave", "veredito_da_chave", "updater_sig_id", "verify_updater_key",
  "veredito_das_assinaturas", "confere_chaves_do_manifesto",
].map(funcao).join("\n");

const b64 = (b) => Buffer.from(b).toString("base64");
const id = (hex) => Buffer.from(hex, "hex");
const pubkey = (hex) => b64(`untrusted comment: minisign public key\n${b64(Buffer.concat([Buffer.from("Ed"), id(hex), Buffer.alloc(32, 7)]))}\n`);
const assinatura = (hex) => b64(`untrusted comment: signature\n${b64(Buffer.concat([Buffer.from("ED"), id(hex), Buffer.alloc(64, 9)]))}\ntrusted comment: t\n${b64(Buffer.alloc(64, 1))}\n`);
const plataforma = { darwin: "macos", linux: "linux", win32: "windows" }[process.platform];

const VELHA = "0123456789ABCDEF", NOVA = "FEDCBA9876543210";

/** A repo whose history is `versoes` ([version, pubkey keyid, tag?]) and whose CLI signs with `assina`. */
function repo(versoes, assina) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-rotacao-"));
  const git = (...a) => execFileSync("git", a, { cwd: dir, stdio: "ignore" });
  git("init", "-q"); git("config", "user.email", "t@t.tld"); git("config", "user.name", "t");
  mkdirSync(join(dir, "src-tauri"));
  for (const [versao, chave, tag] of versoes) {
    writeFileSync(join(dir, "version.md"), `${versao}\n`);
    writeFileSync(join(dir, "src-tauri/tauri.conf.json"), JSON.stringify({ version: versao, plugins: { updater: { pubkey: pubkey(chave) } } }));
    git("add", "-A"); git("-c", "core.hooksPath=/dev/null", "commit", "-qm", versao);
    if (tag) git("tag", versao);
  }
  mkdirSync(join(dir, "node_modules/.bin"), { recursive: true });
  writeFileSync(join(dir, "node_modules/.bin/tauri"), `#!/bin/sh\nprintf '%s' '${assinatura(assina)}' > "$3.sig"\n`, { mode: 0o755 });
  return dir;
}

function roda(dir, transicao, comando, manifesto) {
  if (manifesto) {
    writeFileSync(join(dir, "release.json"), JSON.stringify({ version: "x", platforms: { [plataforma]: { artifacts: manifesto.map((k, i) => ({ file: `a${i}`, signature: assinatura(k) })) } } }));
  }
  const script = [
    "set -euo pipefail",
    `PUBLISH=1; _BUILD_OS=Linux; UPDATER_PASS_FILE=/nonexistent; TRANSICAO_DE_CHAVE=${transicao}`,
    BLOCO,
    comando,
  ].join("\n");
  try {
    const saida = execFileSync("bash", ["-c", script], { cwd: dir, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
    return { status: 0, saida };
  } catch (e) {
    return { status: e.status ?? 1, saida: `${e.stdout ?? ""}${e.stderr ?? ""}` };
  }
}

// The transition commit: 1.0.0 released with the OLD pubkey, 1.0.1 carries the NEW one.
const TRANSICAO = [["1.0.0", VELHA, true], ["1.0.1", NOVA, false]];

const CASOS = [
  ["the transition is real: previous release OLD, tauri.conf.json NEW",
    () => roda(repo(TRANSICAO, VELHA), 1, "confere_transicao_de_chave"), (r) => r.status === 0],
  ["🔴 --transicao-de-chave with the pubkey unchanged is refused",
    () => roda(repo([["1.0.0", VELHA, true], ["1.0.1", VELHA, false]], VELHA), 1, "confere_transicao_de_chave"), (r) => r.status === 1],
  ["🔴 --transicao-de-chave after the transition already shipped is refused",
    () => roda(repo([["1.0.0", VELHA, true], ["1.0.1", NOVA, true], ["1.0.2", NOVA, false]], NOVA), 1, "confere_transicao_de_chave"),
    (r) => r.status === 1],
  ["🔴 no version tag to read the previous pubkey from: refused, not guessed",
    () => roda(repo([["1.0.0", VELHA, false], ["1.0.1", NOVA, false]], VELHA), 1, "confere_transicao_de_chave"), (r) => r.status === 1],
  ["this build's own tag is not 'the previous release'",
    () => roda(repo([["1.0.0", VELHA, true], ["1.0.1", NOVA, true]], VELHA), 1, "confere_transicao_de_chave"), (r) => r.status === 0],
  ["transition: the OLD key signs, and the proof accepts it",
    () => roda(repo(TRANSICAO, VELHA), 1, "verify_updater_key"), (r) => r.status === 0 && /transição/.test(r.saida)],
  ["🔴 transition: the NEW key would sign a release no installed client accepts",
    () => roda(repo(TRANSICAO, NOVA), 1, "verify_updater_key"), (r) => r.status === 1],
  ["🔴 no transition declared: the OLD key after a pubkey change is refused, and pointed to the flag",
    () => roda(repo(TRANSICAO, VELHA), 0, "verify_updater_key"), (r) => r.status === 1 && /--transicao-de-chave/.test(r.saida)],
  ["after the swap, the NEW key signs normally",
    () => roda(repo([["1.0.0", VELHA, true], ["1.0.1", NOVA, true], ["1.0.2", NOVA, false]], NOVA), 0, "verify_updater_key"),
    (r) => r.status === 0],
  ["transition manifest signed with the OLD key goes up",
    () => roda(repo(TRANSICAO, VELHA), 1, "confere_chaves_do_manifesto", [VELHA, VELHA]), (r) => r.status === 0],
  ["🔴 transition manifest with a NEW-key signature is stopped",
    () => roda(repo(TRANSICAO, VELHA), 1, "confere_chaves_do_manifesto", [VELHA, NOVA]), (r) => r.status === 1],
  ["🔴 no transition declared: an OLD-key manifest after the pubkey change is stopped",
    () => roda(repo(TRANSICAO, VELHA), 0, "confere_chaves_do_manifesto", [VELHA]), (r) => r.status === 1],
];

let falhas = 0;
for (const [nome, rodar, ok] of CASOS) {
  const r = rodar();
  if (!ok(r)) {
    falhas++;
    console.error(`🔴 ${nome}: status ${r.status}\n${r.saida.split("\n").slice(0, 6).map((l) => `      ${l}`).join("\n")}`);
  }
}
if (falhas) {
  console.error(`\n${falhas} of ${CASOS.length} cases failed: a rotation could ship a release no installed client accepts.`);
  process.exit(1);
}
console.log(`[transicao-de-chave] ${CASOS.length} cases · the transition signs with the old key, is declared, and happens once`);
