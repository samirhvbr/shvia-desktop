#!/usr/bin/env node
/**
 * The GitHub Release of a published version carries its installers.
 *
 * ## Why (finding f121, measured 22/09/2026)
 *
 * Every Release of this repository had **0 assets** (1.6.2, 1.6.1, 1.6.0, 1.5.17, checked one by
 * one). `release.yml` creates the tag and the notes; building, signing and publishing live in
 * `build-local.sh`, on another machine, at another moment. Nothing tied "1.6.2 was released" to
 * "the three platforms were signed and published", and the only record of what went out was the
 * live `release.json` on the server, which each publish overwrites. The owner chose to attach the
 * installers to the Release (answer "Anexa os instaladores", 23/09/2026).
 *
 * ## What this ruler measures
 *
 * `anexa_na_release_do_github` alone, through real bash, with `gh` replaced by a fake that
 * records its arguments. The function must never break a publish that already reached the
 * server: the server copy is what users download, the GitHub copy is the record. So every
 * failure is a WARNING with the command to finish by hand, and the ruler checks both the verdict
 * and that no upload is attempted when the Release does not exist.
 */
import { execFileSync } from 'node:child_process'
import { mkdtempSync, readFileSync, writeFileSync, chmodSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), '..')
const FONTE = readFileSync(join(RAIZ, 'build-local.sh'), 'utf8')
const BLOCO = (() => {
  const i = FONTE.indexOf('anexa_na_release_do_github() {')
  if (i < 0) {
    console.error('🔴 `anexa_na_release_do_github` no longer exists in build-local.sh.')
    console.error('   Without it a published version leaves no installers on its GitHub Release.')
    process.exit(1)
  }
  return FONTE.slice(i, FONTE.indexOf('\n}\n', i) + 3)
})()

// A fake `gh`: `release view` answers with $VIEW_RC, `release upload` with $UPLOAD_RC, and every
// call is appended to $LOG so the ruler can see what would have been sent.
function fakeGh(dir) {
  const p = join(dir, 'gh')
  writeFileSync(p, `#!/usr/bin/env bash
echo "$*" >> "$LOG"
case "$1 $2" in
  "release view") exit "\${VIEW_RC:-0}" ;;
  "release upload") exit "\${UPLOAD_RC:-0}" ;;
esac
exit 0
`)
  chmodSync(p, 0o755)
  return p
}

function rodar({ semGh = false, viewRc = 0, uploadRc = 0 }) {
  const dir = mkdtempSync(join(tmpdir(), 'anexa-'))
  const log = join(dir, 'log')
  const gh = semGh ? join(dir, 'nao-existe') : fakeGh(dir)
  const script = `set -euo pipefail\n${BLOCO}\nanexa_na_release_do_github 1.6.9 a.AppImage a.AppImage.sha256 release.json`
  const out = execFileSync('bash', ['-c', script], {
    encoding: 'utf8',
    env: { ...process.env, GH_BIN: gh, LOG: log, VIEW_RC: String(viewRc), UPLOAD_RC: String(uploadRc) },
    stdio: ['ignore', 'pipe', 'ignore'],
  }).trim()
  return { veredito: out.split('\n').pop(), chamadas: existsSync(log) ? readFileSync(log, 'utf8') : '' }
}

const CASOS = [
  [{}, 'anexado', c => /release upload 1\.6\.9 a\.AppImage a\.AppImage\.sha256 release\.json --clobber/.test(c),
    'the normal path: every file of this platform plus release.json, replacing a previous copy'],
  [{ semGh: true }, 'sem-gh', c => c === '',
    'no gh on the machine: warn and go on — the server publish already happened'],
  [{ viewRc: 1 }, 'sem-release', c => !/release upload/.test(c),
    '🔴 no Release for the version yet: warn, and NEVER upload (upload would fail or target the wrong tag)'],
  [{ uploadRc: 1 }, 'falhou', c => /release upload/.test(c),
    'upload failed: warn with the command to finish by hand, do not abort the publish'],
]

let falhas = 0
for (const [opts, esperado, chamadasOk, porque] of CASOS) {
  let r
  try { r = rodar(opts) } catch (e) { r = { veredito: `ERROR (exit ${e.status})`, chamadas: '' } }
  if (r.veredito !== esperado || !chamadasOk(r.chamadas)) {
    falhas++
    console.error(`🔴 ${JSON.stringify(opts)}: expected ${esperado}, got ${r.veredito}`)
    console.error(`   gh calls: ${r.chamadas.trim() || '(none)'}`)
    console.error(`   ${porque}`)
  }
}

if (falhas) {
  console.error(`\n${falhas} of ${CASOS.length} cases failed.`)
  console.error('Either a published version stops leaving its installers on the GitHub Release, or a')
  console.error('failure there started breaking a publish that already reached the server.')
  process.exit(1)
}
console.log(`[anexa-na-release] ${CASOS.length} cases · installers go to the Release, and never break the publish`)
