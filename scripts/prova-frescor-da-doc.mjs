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
 * ## Extending it to the BODY was tried and measured false (16/09/2026)
 *
 * Twice now the header was current while the body was not: on 09/09 the header said 1.4.3 and
 * the body 1.1.19, and on 16/09 a paragraph still called three SHVIA-WEB PRs "open" five days
 * after they merged. The obvious repair is to point the `maior-citada` mode — the one that
 * works on `funcionalidades.md` — at `estado-atual.md` too.
 *
 * It does not work, and the reason is worth keeping so nobody spends the afternoon again.
 * `funcionalidades.md` is a log OF VERSIONS: every number in it is a release of this
 * repository, so the highest one is a claim. `estado-atual.md` is PROSE, and prose cites
 * other people's versions. Measured on the real file, the highest own-looking version is
 * **`7.1.0` — pacman's**, named in a note about building on Arch. Six majors ahead of this
 * repository. The comment above already says what a number nobody believes does to a check:
 * it teaches people to ignore it.
 *
 * The other rot is worse for a script: "PR #123 is open" is a fact about ANOTHER repository,
 * and this clone cannot check it offline. Reading a sibling checkout is exactly what 1.5.7
 * removed from this file.
 *
 * So the scope stated above is not an oversight to be closed later — it is the measured
 * boundary. This class of rot is repaired by re-reading, not by a ruler.
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
/**
 * A version that belongs to ANOTHER repository is not a claim about this one.
 *
 * Added 11/09/2026, when `funcionalidades.md` gained the Run. That feature spans four
 * repositories, and saying which of them shipped which half is the whole point of the
 * section — the SHVIA-WEB versions it names (`2.110.254` … `2.110.258`) are higher than
 * anything this repository has ever had, so "the highest version mentioned" read them as
 * this document's own claim and reported 251 patches behind.
 *
 * The check was not wrong to look at the highest number; it had no way to tell WHOSE number
 * it was. Now it does: a match introduced by another repository's name, inside the window
 * below, is skipped. Two things keep this from becoming a hole. The names are the fleet's,
 * so the list is closed by construction rather than guessed. And a version with **no**
 * repository named still counts — which is exactly the F-22 case this file exists for, so
 * the original defect stays caught.
 *
 * The window is a clause, not a paragraph, on purpose: "SHVIA-WEB 2.110.254" is an
 * attribution, while a repo name three sentences earlier is not.
 */
const OUTRO_REPO = /\b(?:SHVIA|shvia)[-\s](?:WEB|CODE|MOBILE|ROTA|SITE|WORKSPACE|web|code|mobile|rota|site|workspace)\b/;
const JANELA_DE_ATRIBUICAO = 60;

const DOCS = [
  // "versão" or "version": the header moves to English when the note is next touched (the
  // repository language rule), and 1.6.7 measured what a Portuguese-only pattern does then —
  // the check fails with "declares no version" on a note that is up to date.
  { file: '.continue/estado-atual.md', modo: 'declarada', re: /(?:vers[ãa]o|version)\s+(\d+\.\d+\.\d+)\)/i },
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
    const todas = [...txt.matchAll(re)]
      .filter((x) => !OUTRO_REPO.test(txt.slice(Math.max(0, x.index - JANELA_DE_ATRIBUICAO), x.index)))
      .map((x) => x[1]);
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
