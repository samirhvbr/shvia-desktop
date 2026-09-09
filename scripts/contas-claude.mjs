#!/usr/bin/env node
// Registers the Claude Code account profiles this machine already has, by asking the shell.
//
// ── Why this exists ───────────────────────────────────────────────────────────
// The picker only ever offered profiles whose directory matched two hardcoded names, and a
// machine that arranges its accounts differently got one entry: the system default. Since
// 1.4.28 a profile also records WHICH variable it switches accounts with, which is what makes
// the other arrangement expressible — but nothing in the UI writes one yet. This script is
// that step, and it is meant to be temporary: when the Settings screen lands, it goes.
//
// ── What it does, and what it refuses to do ───────────────────────────────────
// It asks the SHELL which of its functions set a Claude account variable — never parses
// `.zshrc` by hand — and writes the pairs into `contas-claude.json`. It reads no credential,
// creates no directory, and merges instead of replacing: an entry you wrote by hand survives.
//
// 🔴 It REFUSES on an app older than 1.4.28. Before that version the `var` field does not
// exist, so a credential profile would be applied as `CLAUDE_CONFIG_DIR` — handing the client
// a blank configuration home while the screen keeps naming the account you picked. Writing
// the file would look like it worked and break the next turn, which is the failure mode this
// whole area exists to avoid.
//
//   node scripts/contas-claude.mjs             # shows what it found; writes nothing
//   node scripts/contas-claude.mjs --aplicar   # writes
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const APLICAR = process.argv.includes('--aplicar');
const HOME = os.homedir();
const MINIMA = [1, 4, 28];

const ZSH = `for f in \${(k)functions}; do b=\${functions[$f]}; case $b in (*CLAUDE_*CONFIG_DIR*) print -r -- "$f"$'\\x1f'"\${b//$'\\n'/ }";; esac; done`;
const BASH = `for f in $(declare -F | awk '{print $3}'); do b=$(declare -f "$f"); case $b in *CLAUDE_*CONFIG_DIR*) printf '%s\\x1f%s\\n' "$f" "\${b//$'\\n'/ }";; esac; done`;

const sair = (msg, codigo = 1) => { console.error(msg); process.exit(codigo); };

/** Where the app keeps the registry, per OS. Mirrors Tauri's `app_config_dir`. */
function caminhoDoRegistro() {
  if (process.platform === 'darwin') {
    return path.join(HOME, 'Library/Application Support/cloud.blue3.shvia/contas-claude.json');
  }
  const base = process.env.XDG_CONFIG_HOME || path.join(HOME, '.config');
  return path.join(base, 'cloud.blue3.shvia/contas-claude.json');
}

/** The installed app's version, or null when it is not installed where we look. */
function versaoInstalada() {
  if (process.platform !== 'darwin') return null;
  const plist = '/Applications/ShvIA.app/Contents/Info.plist';
  if (!fs.existsSync(plist)) return null;
  try {
    return execFileSync('defaults', ['read', plist.replace(/\.plist$/, ''), 'CFBundleShortVersionString'],
      { encoding: 'utf8' }).trim();
  } catch { return null; }
}

const menorQue = (v, min) => {
  const p = String(v).split('.').map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < min.length; i++) {
    if ((p[i] ?? 0) !== min[i]) return (p[i] ?? 0) < min[i];
  }
  return false;
};

/** Asks the shell. Interactive on purpose: a function only exists once the config was read. */
function perguntarAoShell() {
  const shell = process.env.SHELL || '';
  const bash = shell.endsWith('bash');
  try {
    return execFileSync(bash ? 'bash' : 'zsh', ['-ic', bash ? BASH : ZSH],
      { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
  } catch { return ''; }
}

/* The same narrow rule the native side applies: one literal assignment, quoted or not,
 * expanding inside $HOME with nothing but $HOME or ~. Anything computed is DROPPED, never
 * guessed — evaluating someone's shell is the line this does not cross. */
function candidatos(saida) {
  const out = [];
  for (const linha of saida.split('\n')) {
    const i = linha.indexOf('\x1f');
    if (i < 0) continue;
    const alias = linha.slice(0, i).trim();
    const corpo = linha.slice(i + 1);
    if (!alias || out.some((c) => c.alias === alias)) continue;
    // Order matters: the long name CONTAINS the short one.
    let variavel = 'CLAUDE_SECURESTORAGE_CONFIG_DIR';
    let pos = corpo.indexOf(`${variavel}=`);
    if (pos < 0) { variavel = 'CLAUDE_CONFIG_DIR'; pos = corpo.indexOf(`${variavel}=`); }
    if (pos < 0) continue;
    const bruto = corpo.slice(pos + variavel.length + 1);
    let valor;
    if (bruto[0] === '"' || bruto[0] === "'") {
      const fim = bruto.indexOf(bruto[0], 1);
      if (fim < 0) continue;
      valor = bruto.slice(1, fim);
    } else {
      valor = bruto.split(/\s/)[0] || '';
    }
    let dir = valor.trim();
    if (dir.startsWith('$HOME')) dir = HOME + dir.slice(5);
    else if (dir.startsWith('${HOME}')) dir = HOME + dir.slice(7);
    else if (dir === '~') dir = HOME;
    else if (dir.startsWith('~/')) dir = path.join(HOME, dir.slice(2));
    if (!dir || dir.includes('$') || !path.isAbsolute(dir) || !dir.startsWith(HOME)) continue;
    out.push({ alias, var: variavel, dir, disponivel: fs.existsSync(dir) });
  }
  return out.sort((a, b) => a.alias.localeCompare(b.alias));
}

const idDe = (alias) => alias.toLowerCase().replace(/[^a-z0-9-]/g, '-').slice(0, 32);

// ── main ─────────────────────────────────────────────────────────────────────
if (process.platform === 'win32') sair('Só macOS e Linux: no Windows não há função de shell para perguntar.');

const instalada = versaoInstalada();
if (instalada && menorQue(instalada, MINIMA)) {
  sair(`🔴 O ShvIA instalado é ${instalada}, e o campo "var" só existe a partir da ${MINIMA.join('.')}.\n`
     + '   Escrever agora faria um perfil de credencial ser aplicado como CLAUDE_CONFIG_DIR:\n'
     + '   o cliente receberia uma casa de configuração em branco com o nome da sua conta na tela.\n'
     + '   Rode ./build-local.sh, instale, e rode este script de novo.');
}

const rodando = (() => {
  try { execFileSync('pgrep', ['-f', 'shvia-desktop'], { stdio: 'ignore' }); return true; } catch { return false; }
})();
if (rodando && APLICAR) {
  sair('🔴 O ShvIA está aberto, e ele reescreve o registro ao trocar de conta — feche antes de aplicar.');
}

const achados = candidatos(perguntarAoShell());
if (!achados.length) {
  sair('Nenhuma função de shell troca de conta do Claude Code nesta máquina.\n'
     + '   Um alias como `claude-b3` costuma ser: CLAUDE_SECURESTORAGE_CONFIG_DIR="$HOME/.claude-cred-blue3" exec claude "$@"');
}

console.log(`Encontrados no shell (${process.env.SHELL || 'zsh'}):\n`);
for (const c of achados) {
  console.log(`  ${c.alias.padEnd(14)} ${c.var.padEnd(32)} ${c.dir}${c.disponivel ? '' : '   (pasta ausente)'}`);
}

const arquivo = caminhoDoRegistro();
let registro = { contas: [], selecionada: 'padrao' };
if (fs.existsSync(arquivo)) {
  try { registro = JSON.parse(fs.readFileSync(arquivo, 'utf8')); } catch {
    sair(`\n🔴 ${arquivo} não é JSON válido. Não vou reescrever por cima do que não consigo ler.`);
  }
}
if (!Array.isArray(registro.contas)) registro.contas = [];

const antes = JSON.stringify(registro.contas);
for (const c of achados) {
  const id = idDe(c.alias);
  const i = registro.contas.findIndex((x) => x && x.id === id);
  const entrada = { id, rotulo: registro.contas[i]?.rotulo || c.alias, dir: c.dir, var: c.var };
  if (i >= 0) registro.contas[i] = entrada; else registro.contas.push(entrada);
}
const mudou = JSON.stringify(registro.contas) !== antes;

console.log(`\nRegistro: ${arquivo}`);
if (!mudou) { console.log('Nada a fazer — já está como precisa ficar.'); process.exit(0); }
if (!APLICAR) {
  console.log('\nFicaria assim (rode com --aplicar para gravar):\n');
  console.log(JSON.stringify(registro, null, 2));
  process.exit(0);
}
fs.mkdirSync(path.dirname(arquivo), { recursive: true });
fs.writeFileSync(arquivo, `${JSON.stringify(registro, null, 2)}\n`);
console.log(`✓ ${registro.contas.length} perfil(is) gravado(s). Abra o ShvIA: o seletor ACCOUNT passa a listá-los.`);
console.log('  O rótulo são as suas palavras — o app nunca verifica identidade, só que a pasta existe.');
