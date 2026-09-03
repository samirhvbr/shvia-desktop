#!/usr/bin/env node
/**
 * Do the "what is implemented" documents still describe the version in `version.md`?
 *
 * ## Why this exists (finding F-22, September 2026 review)
 *
 * `.continue/estado-atual.md` was describing **1.1.34** while the repository was at 1.4.3 —
 * sixty releases apart. `docs/funcionalidades.md` had not been touched since 0.12.0.
 *
 * The telling detail is inside the file itself: it already carries a note saying
 * *"Saneado em 07/08/2026 — este arquivo estava descrevendo a 0.8.0"*. So the sanitation had
 * been done once, by hand, for exactly this reason, and the drift came straight back.
 *
 * That is the signature of a problem an instruction cannot fix. Doing a third manual
 * sanitation would buy a few weeks; a check that fails buys the habit. The finding says as
 * much — "ligar a doc ao CHANGELOG".
 *
 * ## What it checks, and what it deliberately does not
 *
 * It compares the version each document CLAIMS against `version.md`, and fails when the gap
 * passes the tolerance. It does not read the prose: a document can be current in its header
 * and wrong in its body, and no script catches that. What it removes is the case that
 * actually happened — nobody noticing that sixty releases went by.
 *
 * Run: `node scripts/prova-frescor-da-doc.mjs`
 */
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const atual = readFileSync(join(ROOT, 'version.md'), 'utf8').trim();
const [MAJ, MIN, PAT] = atual.split('.').map(Number);

/**
 * How far behind a document may fall before this fails.
 *
 * Patch-level tolerance is generous on purpose: a doc does not go stale because three bug
 * fixes shipped. What it must not miss is a MINOR — that is where features arrive, and
 * features are what these files claim to list.
 */
const TOLERANCIA_PATCH = 25;

/**
 * Two documents, two shapes — and reading them the same way gives a wrong answer.
 *
 * `estado-atual.md` DECLARES the version it describes, in a header. The right signal is that
 * declaration.
 *
 * `funcionalidades.md` is a log **by version**: each bullet says which release brought the
 * feature. Its first `0.2.0` is the oldest entry, not a claim about the document. Reading it
 * the same way reported "1002 minors behind", which is nonsense that would teach anyone to
 * ignore this check. The right signal there is the HIGHEST version mentioned.
 */
const DOCS = [
  { file: '.continue/estado-atual.md', modo: 'declarada', re: /vers[ãa]o\s+(\d+\.\d+\.\d+)\)/i },
  { file: 'docs/funcionalidades.md', modo: 'maior-citada', re: /\b(\d+\.\d+\.\d+)\b/g },
];

let falhas = 0;
for (const { file, re, modo } of DOCS) {
  let txt;
  try {
    txt = readFileSync(join(ROOT, file), 'utf8');
  } catch {
    console.log(`  – ${file} (ausente)`);
    continue;
  }
  let achada;
  if (modo === 'maior-citada') {
    const todas = [...txt.matchAll(re)].map((x) => x[1]);
    achada = todas.sort((a, b) => {
      const [A, B] = [a, b].map((v) => v.split('.').map(Number));
      return A[0] - B[0] || A[1] - B[1] || A[2] - B[2];
    }).pop();
  } else {
    achada = re.exec(txt)?.[1];
  }
  if (!achada) {
    console.error(`  ✗ ${file}: não declara versão nenhuma — não dá para saber se está em dia`);
    falhas++;
    continue;
  }
  const m = [null, achada];
  const [maj, min, pat] = achada.split('.').map(Number);
  // Say the distance in the terms the versions actually use. A single "N minors behind"
  // number across a major boundary produces things like "986", which is arithmetic nobody
  // believes — and a check nobody believes is a check nobody reads.
  const atrasMajor = MAJ - maj;
  const atrasMinor = MIN - min;
  const atrasPatch = PAT - pat;
  const ok = atrasMajor === 0 && atrasMinor <= 0 && atrasPatch <= TOLERANCIA_PATCH;
  console.log(`  ${ok ? '✓' : '✗'} ${file.padEnd(30)} diz ${achada} · repo ${atual}`);
  if (!ok) {
    falhas++;
    const distancia = atrasMajor > 0
      ? `${atrasMajor} major(es) atrás — este documento é anterior à linha ${MAJ}.x inteira`
      : atrasMinor > 0
        ? `${atrasMinor} minor(es) atrás — é aí que entram funcionalidades, e é isso que este arquivo lista`
        : `${atrasPatch} patches atrás (tolerância ${TOLERANCIA_PATCH})`;
    console.error(`      ${distancia}.`);
  }
}

if (falhas) {
  console.error(
    `\n🔴 ${falhas} documento(s) descrevendo uma versão que não é a atual.\n`
      + '   Atualize o texto E o cabeçalho. O CHANGELOG.md tem o que mudou.\n',
  );
  process.exit(1);
}
console.log(`\nOK: a doc de estado acompanha a ${atual}.\n`);
