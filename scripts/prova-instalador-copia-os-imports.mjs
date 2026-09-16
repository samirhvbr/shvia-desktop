#!/usr/bin/env node
/**
 * O `install.sh` de cada runner copia TODO módulo local que o runner importa?
 *
 * ## Por que esta régua existe: é a TERCEIRA vez na mesma classe
 *
 * O `claude-runner.mjs` importa módulos locais irmãos (`./politica.mjs`,
 * `./parada.mjs`), e o instalador os copia por uma LISTA ESCRITA À MÃO. Módulo novo
 * entra no import e não entra na lista, e a instalação nasce quebrada.
 *
 *  - **03/09/2026**: `politica.mjs` ficou de fora desde a 1.4.7. O ✓ dizia "instalado"
 *    com o conjunto incompleto, e quem descobria era o usuário — com uma mensagem que
 *    culpava o binário.
 *  - **1.4.22**: consertado, e o instalador ganhou uma guarda: importa o módulo
 *    instalado antes de declarar ✓, então a cadeia inteira de imports é exercitada.
 *  - **16/09/2026**: `parada.mjs` (a Run, 1.5.0) ficou de fora do mesmo `cp`.
 *
 * ## O que a guarda de 1.4.22 pega, e o que ela NÃO pega
 *
 * Ela é boa e não bastou. Ela só roda **durante uma instalação**, e o CI não instala —
 * então o defeito viajou seis dias e catorze versões sem nada acusar. Quem paga é quem
 * instala do zero, que é justamente quem não tem como saber que o defeito não é dele.
 *
 * Esta régua lê TEXTO: os `import ... from "./x.mjs"` do runner contra o `cp` do
 * instalador. Não instala nada, não precisa de `node_modules`, roda em qualquer máquina —
 * e por isso pode viver no CI, que é onde a de 1.4.22 não alcança.
 *
 * Arquivo de teste (`*.test.mjs`) não conta: ele não viaja para a instalação de propósito.
 *
 * Run: `node scripts/prova-instalador-copia-os-imports.mjs`
 */
import { readFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const RUNNERS = ['claude-runner', 'codex-runner'];

let falhas = 0;
for (const runner of RUNNERS) {
  const entrada = join(ROOT, runner, `${runner}.mjs`);
  const instalador = join(ROOT, runner, 'install.sh');
  if (!existsSync(entrada) || !existsSync(instalador)) {
    console.log(`  – ${runner} (sem runner ou sem instalador)`);
    continue;
  }

  /* A cadeia INTEIRA, não só o primeiro nível: um módulo copiado pode importar outro que
   * não é. O `politica.mjs` de 03/09 era primeiro nível; nada garante que o próximo seja. */
  const vistos = new Set();
  const fila = [entrada];
  const precisa = new Set();
  while (fila.length) {
    const arq = fila.pop();
    if (vistos.has(arq)) continue;
    vistos.add(arq);
    let txt;
    try { txt = readFileSync(arq, 'utf8'); } catch { continue; }
    for (const m of txt.matchAll(/from\s+["']\.\/([A-Za-z0-9_.-]+\.mjs)["']/g)) {
      const nome = m[1];
      if (nome.endsWith('.test.mjs')) continue;
      precisa.add(nome);
      fila.push(join(ROOT, runner, nome));
    }
  }

  const sh = readFileSync(instalador, 'utf8');
  // Só a linha do `cp`: o nome do módulo aparece em comentário (o de 1.4.22 explica o
  // defeito pelo nome), e casar no arquivo inteiro daria verde por causa da explicação.
  const linhaCp = (sh.split('\n').find((l) => /^\s*cp\s/.test(l)) || '');
  const faltando = [...precisa].filter((n) => !linhaCp.includes(n)).sort();

  const ok = faltando.length === 0;
  if (!ok) falhas++;
  console.log(`  ${ok ? '✓' : '✗'} ${runner.padEnd(16)} importa ${precisa.size} módulo(s) local(is)`
    + (ok ? '' : ` · FORA do cp: ${faltando.join(', ')}`));
}

if (falhas) {
  console.error(
    `\n🔴 ${falhas} instalador(es) não copiam um módulo que o runner importa.\n`
      + '   A instalação nasce quebrada, e a guarda do próprio instalador só acusa\n'
      + '   durante uma instalação — o CI não instala. Acrescente o arquivo ao `cp`.\n',
  );
  process.exit(1);
}
console.log('\nOK: todo módulo local importado viaja na instalação.\n');
