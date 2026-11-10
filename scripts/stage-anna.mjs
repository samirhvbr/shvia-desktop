#!/usr/bin/env node
// Prepara o `anna` para ser EMPACOTADO junto do app (item D5).
//
// ── O problema ────────────────────────────────────────────────────────────────
// O Modo Code é a feature mais cara de construir do produto, e o `anna` é
// **pré-requisito externo**: quem instala o ShvIA Desktop não tem Modo Code até
// resolver isso à mão (rodar um install.sh de outro repo, ou pôr um .exe no PATH).
// É o gargalo de adoção — a funcionalidade existe e a maioria nunca chega nela.
//
// ── Por que `externalBin` e NÃO `resources` ───────────────────────────────────
// Os dois copiam arquivos para o bundle, mas só o `externalBin`:
//
//  1. põe o binário **ao lado do executável do app** (`Contents/MacOS/` no macOS),
//     que é exatamente o primeiro lugar onde o `resolve_bin()` já procura. Com
//     `resources` ele iria para `Contents/Resources/` e o lookup não o acharia;
//  2. entra na **assinatura do bundle**. Isso não é conforto: um executável NÃO
//     ASSINADO dentro de um .app assinado **reprova na notarização** da Apple, e o
//     sintoma seria o app inteiro sendo recusado — não o `anna`.
//
// O preço é o nome com **target triple** (`anna-aarch64-apple-darwin`), que é
// justamente o que este script resolve.
//
// ── De onde vem o binário ─────────────────────────────────────────────────────
// Em ordem: `--from <caminho>`, `$SHVIA_ANNA_BIN`, ou o `anna` do PATH.
//
// O PATH é o último e é conveniente, não confiável: empacotar "o que estiver
// instalado" é como uma versão velha vai parar dentro de um release.
//
// ⚠️ Esse risco estava previsto AQUI desde o começo, e a mitigação escolhida foi
// "o script SEMPRE imprime a versão que está empacotando, para alguém conferir
// depois". Em 19/08 ele aconteceu assim mesmo: saiu um release com `anna 0.8.4`
// (julho) e o Modo Code travava no 422 do gateway. O número estava no log; o log
// é que não tem leitor. **Aviso perde para gesto** — por isso agora existe um
// PISO DE VERSÃO que derruba o build (ver `ANNA_MINIMO`, mais abaixo), e a
// impressão da versão virou conferência, não proteção.
import { chmodSync, copyFileSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// 0.16.2 carries subprocess environment filtering as well as the NDJSON credential frame.
const ANNA_MINIMO = [0, 16, 2];

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const DESTINO_DIR = join(ROOT, "src-tauri/binaries");

const args = process.argv.slice(2);
const opt = (n) => {
  const i = args.indexOf(n);
  return i >= 0 && args[i + 1] ? args[i + 1] : null;
};

/** Target triple do Rust — o sufixo que o Tauri exige no nome do sidecar. */
function targetTriple() {
  try {
    const saida = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
    const m = saida.match(/^host:\s*(\S+)$/m);
    if (m) return m[1];
  } catch {
    /* rustc ausente cai no erro abaixo */
  }
  return null;
}

// `pedido` names how the binary was asked for by name (`--anna`, `$SHVIA_ANNA_BIN`); null when it
// was only found on PATH. The --reuse-check mode below treats the two differently.
function acharAnna() {
  const doFlag = opt("--from");
  if (doFlag) return { caminho: resolve(doFlag), pedido: "--anna" };
  if (process.env.SHVIA_ANNA_BIN) return { caminho: resolve(process.env.SHVIA_ANNA_BIN), pedido: "$SHVIA_ANNA_BIN" };

  const cmd = process.platform === "win32" ? "where" : "which";
  try {
    const saida = execFileSync(cmd, ["anna"], { encoding: "utf8" }).trim();
    const primeira = saida.split(/\r?\n/)[0]?.trim();
    if (primeira) return { caminho: primeira, pedido: null };
  } catch {
    /* não está no PATH */
  }
  return null;
}

function versaoDo(bin) {
  try {
    return execFileSync(bin, ["--version"], { encoding: "utf8", timeout: 10_000 }).trim();
  } catch {
    return null;
  }
}

const triple = targetTriple();
if (!triple) {
  console.error("[stage-anna] não consegui descobrir o target triple (rustc ausente?).");
  process.exit(1);
}

const achado = acharAnna();
const origem = achado?.caminho ?? null;

// ── --reuse-check: would a rebuild package a different anna? (1.35.4) ─────────
// build-local.sh reuses the bundle on disk when the version and the sources match, and staging
// runs only on the rebuild path. So `--anna <newer>` against a reused bundle was ignored in
// silence: on the owner's Mac, 1.35.1 (31/10/2026) and again 1.35.3 (02/11/2026, anna 0.15.3
// built a minute before) printed "[reuso] ✅ sim" and kept the engine the bundle was built with.
// This mode answers before the reuse is taken, with the same lookup that stages. It changes
// nothing on disk. Exit 0: the reuse keeps the right engine (stdout says which). Exit 3: rebuild,
// with the reason on stdout.
//
// The rule: an anna asked for by name (--anna, $SHVIA_ANNA_BIN) must be the version in the
// bundle, in either direction. One found only on PATH displaces the bundle's only when it is
// NEWER — a `--publish` run that left out the `--anna` of the build before it must not swap an
// older engine from PATH into the release. Versions, not hashes, are compared: the staged file
// is the one Tauri bundled, and every anna release bumps its version.
if (args.includes("--reuse-check")) {
  const noBundle = join(DESTINO_DIR, `anna-${triple}${process.platform === "win32" ? ".exe" : ""}`);
  const temBundle = existsSync(noBundle);
  const vBundle = temBundle ? versaoDo(noBundle) : null;
  const pBundle = partesDaVersao(vBundle);
  const recusa = (motivo) => { console.log(motivo); process.exit(3); };

  if (temBundle && !pBundle) recusa("the staged Anna did not report a release version");
  if (pBundle && compara(pBundle, ANNA_MINIMO) < 0) recusa(`the bundle carries ${vBundle}, below Anna ${ANNA_MINIMO.join(".")}`);

  if (!achado) {
    console.log(`anna in the bundle: ${vBundle ?? "none"} (no other anna found)`);
    process.exit(0);
  }
  const de = `${achado.pedido ?? "PATH"}: ${origem}`;
  if (!existsSync(origem)) recusa(`the anna this run asks for does not exist (${de})`);
  const vAqui = versaoDo(origem);
  const pAqui = partesDaVersao(vAqui);
  if (!pAqui) recusa(`the anna this run would package did not answer --version (${de})`);
  if (!pBundle) recusa(`the bundle on disk has no anna; this run would package ${vAqui} (${de})`);
  const d = compara(pAqui, pBundle);
  if (achado.pedido ? d !== 0 : d > 0) {
    recusa(`the bundle on disk carries ${vBundle}; this run would package ${vAqui} (${de})`);
  }
  const fica = d < 0 ? ` (the one on PATH, ${vAqui}, is older and stays out)` : "";
  console.log(`anna in the bundle: ${vBundle}${fica}`);
  process.exit(0);
}

// 🔴 (1.38.13) An anna NAMED with `--from` (`./build-local.sh --anna`) or `$SHVIA_ANNA_BIN` that is not there IS an
// error. Naming one is a choice, like the floor below, and until 1.38.13 a typo in `./build-local.sh --publish
// --anna <path>` fell into the absence branch under this one: it removed the staged engine and published a
// release with no Code engine, behind one warning in a long log.
if (achado?.pedido && !existsSync(origem)) {
  console.error(`[stage-anna] ${achado.pedido} names ${origem}, which does not exist.`);
  console.error("            NOT building without the engine that was asked for: fix the path, or build with --no-anna.");
  process.exit(1);
}

// Ausência NÃO é erro de build. O app continua funcionando sem o `anna`
// empacotado — o `resolve_bin()` ainda procura no PATH em runtime, que é o
// comportamento de antes deste item. Derrubar o build aqui transformaria "não
// consegui melhorar a adoção" em "não consegui empacotar o app".
if (!origem || !existsSync(origem)) {
  // 🔴 (1.6.24) The destination is gitignored and PERSISTS between builds: an `anna` staged by
  // an earlier build stayed in src-tauri/binaries/, and `externalBin` bundled it while the line
  // below said the app shipped WITHOUT the engine — an old engine, never checked against
  // ANNA_MINIMO. It is removed; build-local.sh then tells Tauri there is no sidecar.
  const velho = join(DESTINO_DIR, `anna-${triple}${process.platform === "win32" ? ".exe" : ""}`);
  if (existsSync(velho)) {
    rmSync(velho);
    console.warn(`[stage-anna] removido o anna de um build anterior: ${velho}`);
  }
  console.warn("[stage-anna] `anna` não encontrado — o app sai SEM o motor empacotado.");
  console.warn("            Quem instalar precisará instalá-lo à mão (o Modo Code fica indisponível até lá).");
  console.warn("            Para empacotar: ./build-local.sh --anna /caminho/para/anna");
  process.exit(0);
}

const versao = versaoDo(origem);
if (!versao) {
  // Binário que não responde `--version` provavelmente é de outra arquitetura ou
  // está corrompido. Empacotar assim entregaria um Modo Code quebrado por dentro
  // de um app que parece completo — pior que não empacotar.
  console.error(`[stage-anna] ${origem} não respondeu a --version. NÃO vou empacotar um binário que não roda.`);
  process.exit(1);
}

/*
 * ─── Piso de versão do `anna` ──────────────────────────────────────────────────
 *
 * A versão era LIDA e jogada fora, e o custo disso apareceu em 19/08: o app saía
 * com o `anna` que estivesse no PATH da máquina de build — aqui, um **0.8.4 de
 * julho** —, e o Modo Code batia no 422 do gateway (`messages` tem `max:200`) em
 * toda sessão longa. O 0.10.0 (18/08) compacta o histórico antes de enviar, mas
 * quem tinha o binário velho continuava travando, com o app na última versão e a
 * mensagem de erro mandando "atualize o app" — que já estava atualizado.
 *
 * Este piso é a mesma regra do `--version` acima, aplicada a um caso a mais:
 * **empacotar um motor velho entrega um Modo Code quebrado por dentro de um app
 * que parece completo.** Por isso derruba o build em vez de avisar — aviso em log
 * de build é o que ninguém lê, e a falha some dentro de um app que instala bem.
 *
 * Ausência do `anna` continua NÃO sendo erro (acima): lá o Modo Code fica
 * declaradamente indisponível. Aqui ele ficaria disponível e quebrado, que é pior.
 *
 * Ao subir este piso, escreva o PORQUÊ — qual defeito a versão nova conserta.
 */
// 0.11.4: lê `images` do turno do usuário. O piso subiu por causa de uma PROMESSA:
// desde a 1.1.31 a ponte expõe `recursos: { imagem: true }`, e a página do Modo
// Code — que vem do servidor e atualiza sozinha — usa esse flag para decidir se
// oferece colar figura. Empacotar um `anna` anterior faria a casca prometer o que
// o motor não cumpre: a página mandaria `images`, o sidecar leria só `text` e a
// figura sumiria em SILÊNCIO, com o chip na tela dizendo que foi. Piso baixo aqui
// não entrega Modo Code quebrado — entrega Modo Code MENTINDO, que é pior.
// 0.11.9: dois defeitos que só doem AQUI, na casca. (a) Sem o teto de tempo do
// `bash` com morte do GRUPO de processos (achado F-04), uma tool que trava segura o
// sidecar para sempre — e no desktop o `anna` é filho do app: o Modo Code fica
// pendurado sem que o usuário tenha um Ctrl-C para dar, só matando o app inteiro.
// No terminal isso é um incômodo; embutido, é um travamento sem saída. (b) Até a
// 0.11.8 o gate do `bash` casava só o PREFIXO do comando, então `cat .env` saía em
// **auto** (F-02) — e é justamente a casca que aponta o `anna` para a pasta real do
// projeto do usuário, com a página do Modo Code vindo do servidor e atualizando
// sozinha. Piso em 0.11.4 empacotaria um motor que executa leitura de segredo sem
// mostrar cartão, dentro do app que deu a ele a pasta.
// 0.13.6 (raised in 1.35.2, 31/10/2026): the engine's history compaction. Up to 0.13.5,
// past 150 messages every round dropped the 2–4 oldest and only the leading `system`
// messages were protected, so a long Code turn lost the person's own request and
// showed "histórico compactado" before almost every tool call (the owner's Mac, MiniMax
// M3.1 Flash: the 100-round ceiling at 153 tools and 4.47M tokens). 0.13.6 keeps the
// first and latest requests and cuts once every ~25 rounds. The same floor carries
// 0.13.4's patched rustls, rustls-webpki and webpki-roots (RUSTSEC-2026-0346 to -0349,
// -0381, -0382). Why it had to be the floor and not a note: the `anna` staged for the
// Linux 1.35.1 build on the release machine was 0.11.15, the one on its PATH, two
// minutes before those packages were uploaded; 0.11.15 cleared the old 0.11.9 floor,
// so nothing stopped a three-month-old engine from shipping.
// 0.15.8 (raised in 1.36.1, 04/11/2026): a late approval no longer decides the next gate.
// Since 0.14.0 one reader thread queues the host's lines and a `cancel` closes a pending gate;
// an approval sent for that gate a moment too late stayed in the queue, and the NEXT gate,
// another command, took it as its own, so a command the person never saw would run. 0.15.8
// ignores a decision whose `id` is not the waiting gate's. It bites here: this app forwards
// the page's decision lines to the engine as they are (`code_bridge/sessao.rs`), with the
// `id`, and keeps open gates across reloads and the page going to the background, which is
// where a decision arrives late. The same floor carries 0.15.4 (asks before writing an
// instruction file, even with session approval) and 0.15.7 (refuses to overwrite a file
// the model has not seen as it is now). Desktop 1.36.0 was published for Linux with
// 0.15.3, the engine on that build machine's PATH, which cleared the 0.13.6 floor.
// 0.15.10 (raised in 1.36.3, 05/11/2026): a rate-limited round waits at least the backoff.
// The gateway's throttle counts per ACCOUNT, every open agent together, and answers
// `Retry-After: 0` in the last second of its window; up to 0.15.9 the engine took it as given
// and sent its four retries in the same second. It bites here: this app parks a working agent
// when the person switches project, so two agents on one account is this app's normal case —
// the owner's Run on MiniMax stopped at its first iteration that way ("nova tentativa em 0 s
// (4 de 4)"), and the Run counts a failed turn as the end.
// 0.16.0 (raised in 1.36.4, 06/11/2026): a 401 asks for a new key instead of ending the turn.
// The page hands the engine a 12-hour Code token at spawn, and this app keeps an agent alive
// across project switches and pauses: the owner's autonomous Run paused at 19:57, resumed at
// 07:51 with the token expired, got a 401 and ended with the agent's history. 0.16.0
// (shvia-code #24) emits `credential_expired`, takes `{"type":"credential"}` on stdin and repeats
// the request once; the page (shvia-web #686) answers it. An older engine ignores the line, so
// only the renewal before a turn would protect it, and a 401 in the middle of a long turn would
// still end the run.


function partesDaVersao(texto) {
  const m = /^anna (\d+)\.(\d+)\.(\d+)$/.exec(texto ?? "");
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

/** Sign of a − b, decided by the first component that differs. */
function compara(a, b) {
  const i = a.findIndex((n, k) => n !== b[k]);
  return i === -1 ? 0 : a[i] - b[i];
}

const partes = partesDaVersao(versao);
if (!partes) {
  console.error(`[stage-anna] não consegui ler a versão de "${versao}". NÃO vou empacotar sem saber o que é.`);
  process.exit(1);
}

// Compara pelo PRIMEIRO componente que difere — `0.9.9` é menor que `0.10.0`, o
// que uma comparação de string erraria (e erraria justamente neste caso).
const menor = partes.findIndex((n, i) => n !== ANNA_MINIMO[i]);
if (menor !== -1 && partes[menor] < ANNA_MINIMO[menor]) {
  console.error(`[stage-anna] ${origem} é "${versao}", abaixo do piso ${ANNA_MINIMO.join(".")}.`);
  console.error("            Packaging it ships a Code engine without the fixes the floor exists for (see ANNA_MINIMO).");
  console.error("            Conserte instalando o anna atual e rodando de novo:");
  console.error("              cd ../SHVIA-CODE (ou ../shvia-code) && ./install.sh   # ou: --from /caminho/para/anna");
  process.exit(1);
}

mkdirSync(DESTINO_DIR, { recursive: true });
const sufixo = process.platform === "win32" ? ".exe" : "";
const destino = join(DESTINO_DIR, `anna-${triple}${sufixo}`);

copyFileSync(origem, destino);
if (process.platform !== "win32") chmodSync(destino, 0o755);

console.log(`[stage-anna] empacotando ${versao}`);
console.log(`             de:    ${origem}`);
console.log(`             para:  ${destino}`);
