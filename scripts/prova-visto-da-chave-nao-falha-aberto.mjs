#!/usr/bin/env node
/**
 * O ✔ da prova de chave só sai quando a prova ACONTECEU.
 *
 * ## O caso, medido em 21/09/2026
 *
 * `verify_updater_key` protege o único ato que este produto não desfaz: publicar um release
 * assinado com a chave errada faz **todo cliente instalado recusar o update**, e o updater não
 * conserta a si mesmo depois — está escrito no `docs/build.md`.
 *
 * 🔴 **E ele falhava aberto.** Os dois leitores de keyid devolvem string VAZIA em qualquer erro
 * (`catch { write("") }` mais `|| true`), e a comparação só disparava com os dois não-vazios.
 * Id ilegível pulava a comparação, e a linha de sucesso imprimia o ✔ do mesmo jeito — com `(?)`
 * no lugar do id. Um par de parênteses separava *"provado"* de *"não consegui medir"*, no fim
 * de uma linha verde.
 *
 * ⚠️ **O `|| true` dos leitores continua lá, de propósito.** O comentário deles está certo: um
 * preflight não pode ser mais frágil que aquilo que ele protege. O que faltava era o terceiro
 * desfecho que a casa já usa — **0 passou, 1 falhou, 2 não consegui medir** — e a distinção
 * entre um build local (avisa) e um que vai publicar (aborta).
 *
 * Esta régua mede o `veredito_da_chave`, que é a decisão isolada, chamando-o no bash de verdade.
 */
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const MAX_BUFFER = 64 * 1024 * 1024
const RAIZ = join(dirname(fileURLToPath(import.meta.url)), '..')

/* Só a FUNÇÃO: o `build-local.sh` inteiro tem `set -e` e efeitos de verdade (mktemp, scp,
 * compilação), então a régua recorta o bloco e o roda sozinho — mede a decisão, não o script.
 *
 * 🔬 A primeira versão fazia o recorte com `$(sed …)` DENTRO do bash, e o shell executou o
 * texto extraído como comando em vez de defini-lo: `veredito_da_chave(): command not found`,
 * nove de nove casos "falhando" por um motivo que nada tinha a ver com o que eles medem. */
const FONTE = readFileSync(join(RAIZ, 'build-local.sh'), 'utf8')
const BLOCO = (() => {
  const i = FONTE.indexOf('veredito_da_chave() {')
  if (i < 0) {
    console.error('🔴 `veredito_da_chave` não existe mais no build-local.sh.')
    console.error('   Se a decisão voltou para dentro do `verify_updater_key`, o ✔ pode ter')
    console.error('   voltado a sair sem a prova — e esta régua não tem mais o que medir.')
    process.exit(1)
  }
  const fim = FONTE.indexOf('\n}\n', i)
  return FONTE.slice(i, fim + 3)
})()

function veredito(pub, sig, publicando) {
  const script = `set -u\n${BLOCO}\nveredito_da_chave ${JSON.stringify(pub)} ${JSON.stringify(sig)} ${JSON.stringify(publicando)}`

  return execFileSync('bash', ['-c', script], { encoding: 'utf8', maxBuffer: MAX_BUFFER }).trim()
}

const CASOS = [
  // pub, sig, publicando, esperado, por quê
  ['ABC123', 'ABC123', '0', 'ok', 'os dois lidos e iguais — o único caso que merece ✔'],
  ['ABC123', 'ABC123', '1', 'ok', 'publicar com o par provado segue normal'],
  ['ABC123', 'DEF456', '0', 'errada', 'par diferente aborta, publicando ou não'],
  ['ABC123', 'DEF456', '1', 'errada', 'idem'],
  ['', 'ABC123', '0', 'nao-medi', '🔴 pubkey ilegível: ANTES isto imprimia ✔'],
  ['ABC123', '', '0', 'nao-medi', '🔴 assinatura ilegível: ANTES isto imprimia ✔ com "(?)"'],
  ['', '', '0', 'nao-medi', 'nenhum dos dois lido'],
  ['', 'ABC123', '1', 'nao-medi-e-vai-publicar', '🔴 não medi E vai publicar — é aqui que custa'],
  ['ABC123', '', '1', 'nao-medi-e-vai-publicar', 'idem, pelo outro leitor'],
]

let falhas = 0
for (const [pub, sig, pubn, esperado, porque] of CASOS) {
  let obtido
  try {
    obtido = veredito(pub, sig, pubn)
  } catch (e) {
    obtido = `ERRO: ${e.message}`
  }
  if (obtido !== esperado) {
    falhas++
    console.error(`🔴 pub=${JSON.stringify(pub)} sig=${JSON.stringify(sig)} publicando=${pubn}`)
    console.error(`   esperado: ${esperado}   obtido: ${obtido}`)
    console.error(`   ${porque}`)
  }
}

if (falhas) {
  console.error(`\n${falhas} de ${CASOS.length} casos falharam.`)
  console.error('O ✔ da prova de chave voltou a sair sem a prova ter acontecido, ou o terceiro')
  console.error('desfecho ("não consegui medir") sumiu. Ele é o que separa provado de silencioso.')
  process.exit(1)
}

console.log(`[visto-da-chave] ${CASOS.length} casos · o ✔ só sai com os dois keyids lidos e iguais`)
