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
import { chmodSync, copyFileSync, existsSync, mkdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

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

function acharAnna() {
  const doFlag = opt("--from");
  if (doFlag) return resolve(doFlag);
  if (process.env.SHVIA_ANNA_BIN) return resolve(process.env.SHVIA_ANNA_BIN);

  const cmd = process.platform === "win32" ? "where" : "which";
  try {
    const saida = execFileSync(cmd, ["anna"], { encoding: "utf8" }).trim();
    const primeira = saida.split(/\r?\n/)[0]?.trim();
    if (primeira) return primeira;
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

const origem = acharAnna();

// Ausência NÃO é erro de build. O app continua funcionando sem o `anna`
// empacotado — o `resolve_bin()` ainda procura no PATH em runtime, que é o
// comportamento de antes deste item. Derrubar o build aqui transformaria "não
// consegui melhorar a adoção" em "não consegui empacotar o app".
if (!origem || !existsSync(origem)) {
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
const ANNA_MINIMO = [0, 11, 9];

function partesDaVersao(texto) {
  const m = /(\d+)\.(\d+)\.(\d+)/.exec(texto ?? "");
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
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
  console.error("            Empacotar isto entrega um Modo Code que trava no 422 do gateway em sessão longa.");
  console.error("            Conserte instalando o anna atual e rodando de novo:");
  console.error("              cd ../SHVIA-CODE && ./install.sh   # ou: --from /caminho/para/anna");
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
