#!/usr/bin/env node
/**
 * A failed read of the published manifest never becomes "nothing was published" (1.6.20).
 *
 * `build-local.sh --publish` downloads the server's `release.json` and merges this platform on
 * top of it; the merged file then replaces the server's. Until 1.6.20 the download was
 * fail-open: a timeout, a 5xx or a refused connection read as "no manifest", and the upload
 * replaced the server's manifest with one holding only this platform — every other platform's
 * users stopped getting updates, with no error anywhere.
 *
 * This proof runs the REAL `fetch_remote_manifest` (cut out of build-local.sh, like
 * prova-visto-da-chave does) against a local HTTP server, under `set -euo pipefail` as in the
 * script: the curl wiring is where this kind of defect hides, so it is exercised, not assumed.
 */
import { execFile } from "node:child_process";
import { mkdtempSync, readFileSync, existsSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const FONTE = readFileSync(join(RAIZ, "build-local.sh"), "utf8");
const i = FONTE.indexOf("veredito_do_manifesto_remoto() {");
const j = FONTE.indexOf("fetch_remote_manifest() {");
if (i < 0 || j < 0) {
  console.error("🔴 veredito_do_manifesto_remoto / fetch_remote_manifest moved out of build-local.sh.");
  process.exit(1);
}
const BLOCO = FONTE.slice(i, FONTE.indexOf("\n}\n", j) + 3);

const MANIFESTO = JSON.stringify({ version: "9.9.9", platforms: { linux: {}, macos: {} } });
const servidor = createServer((req, res) => {
  const rota = req.url.split("/")[1];
  if (rota === "ok") { res.writeHead(200); res.end(MANIFESTO); }
  else if (rota === "lixo") { res.writeHead(200); res.end("<html>proxy error</html>"); }
  else if (rota === "falha") { res.writeHead(502); res.end("bad gateway"); }
  else { res.writeHead(404); res.end("not found"); }
});
await new Promise((r) => servidor.listen(0, "127.0.0.1", r));
const porta = servidor.address().port;

// ASYNC on purpose: the server lives in this process, and a synchronous child would block the
// event loop that answers it — curl would connect and wait out its --max-time. (The first
// version of this proof did exactly that, and "measured" the fix as broken.)
function rodar(base, env = {}) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-manifesto-"));
  const script = `set -euo pipefail\n${BLOCO}\nPUBLIC_BASE=${JSON.stringify(base)}\nfetch_remote_manifest`;
  return new Promise((resolve) => {
    execFile("bash", ["-c", script], { cwd: dir, env: { ...process.env, ...env } }, (err) => {
      const r = { status: err ? (err.code ?? 1) : 0, mesclou: existsSync(join(dir, "release.json")) };
      rmSync(dir, { recursive: true, force: true });
      resolve(r);
    });
  });
}

const url = (rota) => `http://127.0.0.1:${porta}/${rota}`;
const CASOS = [
  ["200 com JSON válido", url("ok"), {}, 0, true],
  ["404: nunca publicado", url("nada"), {}, 0, false],
  ["🔴 502: o servidor falhou", url("falha"), {}, 1, false],
  ["🔴 200 com lixo (proxy)", url("lixo"), {}, 1, false],
  ["🔴 conexão recusada", "http://127.0.0.1:9", {}, 1, false],
  ["recusada, mas o operador mandou descartar", "http://127.0.0.1:9", { SHVIA_PUBLISH_SEM_MESCLAR: "1" }, 0, false],
];

let falhas = 0;
for (const [nome, base, env, status, mesclou] of CASOS) {
  const r = await rodar(base, env);
  if ((status === 0) !== (r.status === 0) || r.mesclou !== mesclou) {
    falhas++;
    console.error(`🔴 ${nome}: esperado ${status === 0 ? "segue" : "ABORTA"}${mesclou ? " e mescla" : ""}, obtido status=${r.status} mesclou=${r.mesclou}`);
  }
}
servidor.close();

if (falhas) {
  console.error(`\n${falhas} de ${CASOS.length} casos falharam: uma leitura que falhou voltou a valer como`);
  console.error('"nada publicado" — e o upload apagaria as outras plataformas do servidor.');
  process.exit(1);
}
console.log(`[manifesto-remoto] ${CASOS.length} casos · só 200 mescla, só 404 recomeça, o resto aborta o publish`);
