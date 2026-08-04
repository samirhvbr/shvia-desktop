#!/usr/bin/env bash
# build-local.sh — Build LOCAL do ShvIA Desktop no macOS e Linux.
# Gera os instaladores do app. O ShvIA é shell fino: SEM sidecar.
#
# NÃO HÁ CI: ela foi removida na 0.4.6 por custo, e o build é 100% local por
# decisão. Este script É o pipeline — inclusive checksums e manifesto
# (release.json), que o item D9 acrescentou e o D1 (auto-update) vai consumir.
#   macOS  -> .dmg + .app.tar.gz
#   Linux  -> .deb + .AppImage (+ .rpm)   (targets="all" do tauri.conf.json)
#             + .pkg.tar.zst (Arch)       (conversão do .deb via fpm — best-effort)
#
# PRÉ-REQUISITOS (instalar uma vez):
#   Comum:   Node 20, Rust (rustup default stable)
#   macOS:   xcode-select --install
#   Linux (Debian/Ubuntu):
#     sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
#       libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
#
# USO (na raiz do repo):
#   ./build-local.sh                 # build normal (todos os targets do SO)
#   ./build-local.sh --skip-npm-ci   # pula 'npm ci' (deps já instaladas)
#   ./build-local.sh --bundles deb   # só um target (deb|appimage|rpm|dmg|app)
#   ./build-local.sh --no-sign       # (macOS) NÃO assina/notariza — build de teste
#   ./build-local.sh --skip-git-pull # NÃO sincroniza com o remoto antes do build
#   ./build-local.sh --anna /caminho/para/anna   # empacota ESTE anna (item D5)
#   ./build-local.sh --no-anna       # NÃO empacota o motor (app sai sem Modo Code
#                                    # pronto — o usuário terá de instalar à mão)
#   ./build-local.sh --publish       # publica no servidor por scp (item D1)
#   ./build-local.sh --publish --dest root@HOST:/caminho/   # outro destino
#   ./build-local.sh --force         # reconstrói mesmo já havendo build da versão
#
# REUSO DE BUILD: se já existe build DESTA versão no disco e nenhuma fonte mudou,
# o script NÃO recompila — vai direto ao manifesto (e à publicação, com
# --publish). Foi feito para o caso de esquecer o --publish e não pagar um rebuild
# inteiro só para subir arquivo que já existe.
#
# "O arquivo existe" não é prova: no macOS o artefato do updater é
# `ShvIA.app.tar.gz`, SEM versão no nome. A identidade vem do sha256 gravado no
# release.json pelo build que o gerou, e o script CONFERE esse hash. Além disso,
# se qualquer fonte (src/, src-tauri/src/, capabilities/, binaries/, manifests)
# for mais nova que o artefato, ele RECOMPILA — senão editar código sem bumpar a
# versão publicaria binário velho assinado como se fosse a versão nova.
# Para forçar: --force, ou apague src-tauri/target/release/bundle.
#
# CHAVE DO UPDATER: como o bundle gera artefato de updater (ADR-022), o build
# EXIGE `TAURI_SIGNING_PRIVATE_KEY` no ambiente e isto é verificado ANTES de
# compilar — o Tauri só reclamaria no fim (na máquina Linux custou 2m01s antes de
# abortar). E a variável preenchida não basta: o preflight ASSINA um arquivo
# descartável (~1s) para provar que a senha abre a chave e que ela é a do par
# publicado — assinar com outro par gera um release que nenhum cliente instalado
# aceita, sem conserto pelo próprio updater. Para build de teste sem chave, use
# --no-sign: sai sem artefato de updater e NÃO deve ser publicado.
#
# PUBLICAR (--publish, item D1): manda os artefatos DESTA plataforma + o
# release.json para o servidor por scp, e confere o resultado pela URL pública.
# Funciona igual no macOS e no Linux — a lista de arquivos sai do próprio
# release.json, então o que sobe é exatamente o que o manifesto declara.
#
#   - Uma senha só: é UM `scp` com todos os arquivos (uma conexão). Para não
#     digitar nada, `ssh-copy-id root@HOST` uma vez e o scp passa a usar a chave.
#   - O `.sig` NÃO sobe, e é de propósito: o conteúdo dele já está embutido no
#     release.json (campo `signature`), que é de onde o servidor lê.
#   - ANTES de gerar o manifesto, o script BAIXA o release.json publicado para a
#     raiz do repo. É o que faz a mescla de plataformas acontecer sozinha: sem
#     isso, publicar do macOS apagaria a entrada do Windows do servidor e os
#     usuários de Windows paravam de receber update SEM NENHUM SINTOMA no build.
#   - DEPOIS de subir, confere o sha256 do artefato de updater **pela URL
#     pública**. É o passo que pega o erro real de 28/07/2026: o `.app.tar.gz`
#     ficou de fora do upload e o endpoint continuou devolvendo 200 (o manifesto
#     estava certo), então a falha só aparecia quando o app tentava baixar.
#
# MOTOR EMPACOTADO (item D5): o `anna` é o gargalo de adoção do Modo Code — hoje é
# pré-requisito externo, e quem instala o app não tem a feature até resolver isso à
# mão. Por padrão o build empacota o `anna` do PATH como sidecar (`externalBin`),
# e SEMPRE imprime qual versão está indo — empacotar "o que estiver instalado" é
# como uma versão velha vai parar dentro de um release. Sem `anna` no PATH, o build
# segue e avisa.
#
# GIT PULL (padrão da casa): antes de tudo, o script faz 'git pull --ff-only'
# para você não empacotar código velho sem querer. É fast-forward-only (nunca
# cria merge) e NÃO trava o build se falhar (offline, mudanças locais ou branch
# divergente) — só avisa e segue com o que está local. Pule com --skip-git-pull.
#
# ASSINATURA (macOS): sem assinar, o macOS trata o app como "danificado" e oferece
# MOVER PARA A LIXEIRA quando ele é aberto depois de baixado/enviado (atributo de
# quarentena). Este script, no macOS, assina o app com o cert **Developer ID
# Application** do keychain e o `tauri build` **notariza + staple** sozinho quando
# há credencial de notarização. A senha de app (Apple ID) fica no **keychain**
# (serviço "shvia-notarize"), NUNCA no repo. Guardar uma vez:
#   security add-generic-password -U -s shvia-notarize -a SEU_APPLE_ID -w
# (pede a senha de app escondida — gere em appleid.apple.com › Senhas de app).
# Sem cert -> build sai sem assinar; sem credencial -> assina mas não notariza.
#
# VM/SEM GPU: o app empacotado pode abrir com **janela em branco** se o WebKitGTK
# não tiver acesso à GPU (DRM). Para ABRIR aqui, rode com
# WEBKIT_DISABLE_DMABUF_RENDERER=1 (render por software). Não afeta o build, só a
# execução. Detalhes em docs/decisoes.md / .continue.
#
# Saída: src-tauri/target/release/bundle/ (+ um .sha256 ao lado de cada instalador)
#        release.json na raiz — o manifesto que o auto-update (D1) vai ler.
#
# O release.json é MESCLADO, não sobrescrito: cada SO é empacotado numa máquina
# diferente e nenhuma vê os artefatos das outras, então o build do Windows não pode
# apagar a entrada do macOS. O script avisa quais plataformas ainda faltam.
#
# Obs.: o 1º build compila o Rust inteiro (~minutos); os próximos são incrementais.
set -euo pipefail
cd "$(dirname "$0")"

# Item D5: de onde vem o `anna` a empacotar, e se empacota.
ANNA_FROM=""
NO_ANNA=0

# ── Cronômetro do build: tempo total (parede) + por etapa ───────────────────
# Mesmo padrão do SHVTERM/build-local.sh: o "built in Xs" do Vite/cargo é só
# UMA etapa interna. Aqui medimos o script INTEIRO (npm → versão → bundle),
# por etapa e no total, e imprimimos mesmo quando aborta por erro (trap EXIT).
# Serve p/ comparar Linux × macOS × Windows e achar em qual fase otimizar.
SECONDS=0
_BUILD_OS=$(uname -s); [ "$_BUILD_OS" = Darwin ] && _BUILD_OS=macOS
_PH_NAMES=(); _PH_TIMES=(); _PH_CUR=""; _PH_START=0
_fmt() {  # $1 = segundos -> "1h 02m 03s" / "4m 05s" / "37s"
  local t=$1
  if   [ "$t" -ge 3600 ]; then printf '%dh %02dm %02ds' $((t/3600)) $(((t%3600)/60)) $((t%60))
  elif [ "$t" -ge 60 ];   then printf '%dm %02ds' $((t/60)) $((t%60))
  else                         printf '%ds' "$t"; fi
}
step() {  # fecha a etapa anterior, abre a nova, e mostra o relógio corrente
  local now=$SECONDS
  if [ -n "$_PH_CUR" ]; then
    _PH_NAMES+=("$_PH_CUR"); _PH_TIMES+=($((now - _PH_START)))
  elif [ "$now" -gt 0 ]; then
    _PH_NAMES+=("preparação"); _PH_TIMES+=("$now")
  fi
  _PH_CUR="$1"; _PH_START=$now
  echo "==> [$(_fmt "$now")] $1"
}
_summary() {  # tabela final: cada etapa + TOTAL
  if [ -n "$_PH_CUR" ]; then
    _PH_NAMES+=("$_PH_CUR"); _PH_TIMES+=($((SECONDS - _PH_START))); _PH_CUR=""
  fi
  echo ""
  echo "⏱  tempo por etapa ($_BUILD_OS):"
  if [ "${#_PH_NAMES[@]}" -gt 0 ]; then
    local i
    for i in "${!_PH_NAMES[@]}"; do
      printf '     %8s  %s\n' "$(_fmt "${_PH_TIMES[$i]}")" "${_PH_NAMES[$i]}"
    done
  fi
  echo "     ────────"
  printf '     %8s  TOTAL\n' "$(_fmt "$SECONDS")"
}
_on_exit() {  # se abortar (exit != 0), ainda mostra quanto tempo rodou
  local code=$?
  if [ "$code" -ne 0 ]; then
    echo "" >&2
    echo "❌ build abortou após $(_fmt "$SECONDS")  ($_BUILD_OS, exit $code)" >&2
  fi
}
trap _on_exit EXIT

usage() { awk 'NR>1{ if($0=="set -euo pipefail") exit; sub(/^# ?/,""); print }' "$0"; }

# ── git pull antes do build (padrão da casa) ────────────────────────────────
# Sincroniza o remoto ANTES de qualquer passo, pra não empacotar código velho,
# via scripts/git-sync.mjs (a mesma lógica do build-local.ps1 e do `npm run
# pull`). É fast-forward-only e NÃO trava o build. Pule com --skip-git-pull.
# Sincronia manual (fora do build): use `npm run pull` no lugar de `git pull`.
# Delega a sincronia pro scripts/git-sync.mjs (uma implementação só, igual no
# build-local.ps1 e no `npm run pull`). Ele restaura ao HEAD APENAS os manifests
# cuja única diferença é a linha de versão (lixo regenerável) e preserva
# qualquer mudança real (ex.: dep nova no Cargo.toml), depois faz pull --ff-only.
# Nunca derruba o build (o próprio git-sync.mjs sai 0 sempre).
git_sync() {
  if [ "${SKIP_GIT_PULL:-0}" -eq 1 ]; then
    echo "    (pulado: --skip-git-pull)"
    return 0
  fi
  if ! command -v node >/dev/null 2>&1; then
    echo "    (node não encontrado — pulando a sincronia)"
    return 0
  fi
  node scripts/git-sync.mjs || true
}

# ── Preflight: checa o toolchain ANTES dos passos lentos ────────────────────
# Falha em <1s com mensagem ACIONÁVEL (ex.: "instale o Rust: curl … rustup.rs")
# em vez do erro críptico "cargo metadata: No such file or directory" 5s adentro.
# Coleta TODOS os faltantes de uma vez. Recupera o cargo de ~/.cargo/bin se ele
# existe mas não está no PATH (caso comum: rustup instalado, mas o terminal novo
# não recarregou o PATH).
preflight() {
  local missing=()

  command -v node >/dev/null 2>&1 || missing+=(
    "Node.js não encontrado. Instale o Node 20+ (https://nodejs.org, 'brew install node' ou nvm)."
  )
  command -v npm >/dev/null 2>&1 || missing+=(
    "npm não encontrado (vem com o Node)."
  )

  # Rust: se o cargo não está no PATH mas o rustup instalou em ~/.cargo/bin,
  # puxa pro PATH desta execução e avisa — não obriga a reabrir o shell.
  if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
    echo "    (cargo achado em ~/.cargo/bin — adicionado ao PATH desta execução; pra"
    echo "     fixar, rode 'source \$HOME/.cargo/env' ou reabra o terminal)"
  fi
  if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
    missing+=(
"Rust (cargo) não encontrado — é o que o Tauri usa pra compilar.
       Instale:  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
       Depois:   source \"\$HOME/.cargo/env\"   (ou reabra o terminal)"
    )
  fi

  # macOS: o toolchain da Apple (clang/linker) vem do Command Line Tools do Xcode.
  # (o próprio rustup precisa dele pra linkar — checar junto evita erro na 2ª tentativa.)
  if [ "$_BUILD_OS" = macOS ] && ! xcode-select -p >/dev/null 2>&1; then
    missing+=(
"Command Line Tools do Xcode ausentes (clang/linker do macOS).
       Instale:  xcode-select --install"
    )
  fi

  # Linux (Debian/Ubuntu): o WebKitGTK dev é obrigatório pro WebView do Tauri.
  if [ "$_BUILD_OS" = Linux ] && command -v pkg-config >/dev/null 2>&1 \
     && ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    missing+=(
"libwebkit2gtk-4.1-dev ausente (e outras deps do Tauri no Linux). Instale:
       sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \\
         libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf"
    )
  fi

  if [ "${#missing[@]}" -gt 0 ]; then
    echo "" >&2
    echo "❌ pré-requisitos faltando ($_BUILD_OS) — corrija e rode de novo:" >&2
    echo "" >&2
    local m
    for m in "${missing[@]}"; do
      echo "   • $m" >&2
      echo "" >&2
    done
    exit 1
  fi
}

# ── macOS: assinatura (Developer ID) + notarização (senha de app) ────────────
# Sem isto, um app baixado/enviado (com quarentena) é rejeitado pelo Gatekeeper e
# o macOS oferece MOVER PARA A LIXEIRA. Fluxo:
#   1) acha o cert "Developer ID Application" no keychain e exporta
#      APPLE_SIGNING_IDENTITY — o `tauri build` assina o .app (hardened runtime).
#   2) se houver credencial de notarização (senha de app no keychain, serviço
#      "shvia-notarize"), exporta APPLE_ID/APPLE_PASSWORD/APPLE_TEAM_ID — aí o
#      `tauri build` NOTARIZA e faz STAPLE do .app automaticamente.
# Segredo NUNCA entra no repo: a senha de app mora só no keychain deste Mac.
NOTARY_SERVICE="shvia-notarize"
SIGN_ENABLED=0
NOTARIZE_ENABLED=0

setup_macos_signing() {
  [ "$_BUILD_OS" = macOS ] || return 0
  if [ "${NO_SIGN:-0}" -eq 1 ]; then
    echo "    (--no-sign: build de teste, SEM assinar/notarizar)"
    return 0
  fi

  # 1) Identidade de assinatura: 1ª "Developer ID Application" do keychain,
  #    salvo se APPLE_SIGNING_IDENTITY já vier do ambiente.
  if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
    APPLE_SIGNING_IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
      | awk -F'"' '/Developer ID Application/{print $2; exit}' || true)"
  fi
  if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
    echo "    ⚠️  sem cert 'Developer ID Application' no keychain — BUILD SAI SEM ASSINAR."
    echo "        (o macOS vai oferecer 'mover p/ lixeira' ao abrir o app baixado)"
    return 0
  fi
  export APPLE_SIGNING_IDENTITY
  SIGN_ENABLED=1
  echo "    ✔ assinatura: $APPLE_SIGNING_IDENTITY"

  # Team ID: extrai o (XXXXXXXXXX) do fim da identidade, salvo se já vier do ambiente.
  if [ -z "${APPLE_TEAM_ID:-}" ]; then
    APPLE_TEAM_ID="$(printf '%s' "$APPLE_SIGNING_IDENTITY" \
      | sed -n 's/.*(\([A-Z0-9]\{10\}\))$/\1/p')"
  fi

  # 2) Credencial de notarização (senha de app) do keychain, salvo se já vier do ambiente.
  if [ -z "${APPLE_PASSWORD:-}" ]; then
    APPLE_PASSWORD="$(security find-generic-password -s "$NOTARY_SERVICE" -w 2>/dev/null || true)"
  fi
  if [ -z "${APPLE_ID:-}" ]; then
    APPLE_ID="$(security find-generic-password -s "$NOTARY_SERVICE" 2>/dev/null \
      | awk -F'"' '/"acct"/{print $4}' || true)"
  fi

  if [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
    export APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID
    NOTARIZE_ENABLED=1
    echo "    ✔ notarização: $APPLE_ID (team $APPLE_TEAM_ID) — tauri build vai notarizar+staple"
    echo "      (a notarização sobe o app p/ a Apple e ESPERA — pode levar alguns minutos)"
  else
    echo "    ⚠️  vou ASSINAR mas NÃO notarizar (falta credencial). Guarde a senha de app:"
    echo "        security add-generic-password -U -s \"$NOTARY_SERVICE\" -a \"SEU_APPLE_ID\" -w"
    echo "        (sem notarizar, o app abre mas ainda pede liberação em Ajustes → Privacidade)"
  fi
}

# Verificação pós-build: prova que o .app/.dmg ficaram aceitáveis ao Gatekeeper.
verify_macos_signature() {
  [ "$_BUILD_OS" = macOS ] || return 0
  [ "$SIGN_ENABLED" -eq 1 ] || { echo "    (build sem assinatura — nada a verificar)"; return 0; }

  local app dmg
  app="$(find src-tauri/target/release/bundle/macos -maxdepth 1 -name '*.app' 2>/dev/null | head -1 || true)"
  dmg="$(find src-tauri/target/release/bundle/dmg  -maxdepth 1 -name '*.dmg' 2>/dev/null | head -1 || true)"
  # NOTA: no build só-DMG (`--bundles dmg`), o `tauri build` APAGA o .app depois de
  # dobrá-lo dentro do .dmg ("Cleaning …/ShvIA.app") — então aqui NÃO há .app avulso
  # pra checar. As verificações do .app ficam sob `if [ -n "$app" ]`, mas o bloco do
  # .dmg RODA SEMPRE — antes ele estava dentro de um `return 0` precoce ("sem .app"),
  # e o caminho só-DMG saía com o .dmg SEM notarizar. O .app dentro do .dmg já foi
  # notarizado+stapled antes de o tauri limpá-lo, então só falta notarizar o .dmg.
  if [ -n "$app" ]; then
    echo "  • codesign --verify (deep, strict):"
    if codesign --verify --deep --strict --verbose=2 "$app" >/tmp/_cs.txt 2>&1; then
      echo "      ✔ assinatura íntegra"
    else
      echo "      ❌ assinatura inválida:"; sed 's/^/        /' /tmp/_cs.txt
    fi

    echo "  • autoridade + hardened runtime:"
    codesign -dvvv "$app" 2>&1 \
      | grep -E 'Authority=|TeamIdentifier=|Identifier=|flags=' | sed 's/^/      /' || true

    echo "  • Gatekeeper (spctl assess):"
    spctl -a -t exec -vvv "$app" 2>&1 | sed 's/^/      /' || true

    echo "  • staple (ticket de notarização anexado):"
    if xcrun stapler validate "$app" >/dev/null 2>&1; then
      echo "      ✔ .app com staple (abre offline, sem prompt)"
    else
      echo "      ⚠️  .app SEM staple — notarização não rodou/falhou (ainda pede liberação)"
    fi
  else
    echo "    (sem .app avulso — build só-DMG; o tauri já limpou o .app, verifico o .dmg)"
  fi
  if [ -n "$dmg" ]; then
    if xcrun stapler validate "$dmg" >/dev/null 2>&1; then
      echo "      ✔ .dmg com staple"
    elif [ "$NOTARIZE_ENABLED" -eq 1 ]; then
      echo "      • .dmg ainda sem staple — notarizando o próprio .dmg (submit + staple)…"
      if xcrun notarytool submit "$dmg" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" \
             --team-id "$APPLE_TEAM_ID" --wait 2>&1 | sed 's/^/        /' \
         && xcrun stapler staple "$dmg" 2>&1 | sed 's/^/        /'; then
        echo "      ✔ .dmg notarizado + stapled"
      else
        echo "      ⚠️  não notarizei o .dmg — mas o .app dentro dele já está"
        echo "          notarizado+stapled, então distribuir o .dmg funciona mesmo assim."
      fi
    fi
  fi
}

# macOS: destrava imagens .dmg DESTE repo que ficaram montadas de um build anterior.
# Contexto do bug: o bundle_dmg.sh cria um rw.*.dmg temporário, monta em
# /Volumes/dmg.XXXX (nome interno do volume = "ShvIA"), arruma a janela via
# AppleScript e desmonta. Se o processo morre/é morto no meio, a imagem fica
# ATTACHADA. No próximo build o AppleScript roda `tell disk "ShvIA"` com DOIS
# volumes "ShvIA" montados → ambíguo → erro → "failed to run bundle_dmg.sh" (e o
# tauri engole o erro real do script). Filtra pelo image-path dentro do nosso
# bundle dir pra NÃO ejetar DMGs de outros projetos/apps montados pelo usuário.
detach_stale_build_images() {
  [ "$_BUILD_OS" = macOS ] || return 0
  command -v hdiutil >/dev/null 2>&1 || return 0
  local bundle_abs devs d
  bundle_abs="$(pwd)/src-tauri/target/release/bundle"
  devs="$(hdiutil info 2>/dev/null | awk -v b="$bundle_abs" '
    /^image-path/            { p = (index($0, b) > 0) }
    p && /^\/dev\/disk[0-9]/ { print $1; p = 0 }
  ' || true)"
  for d in $devs; do
    echo "    imagem presa de build anterior — ejetando $d"
    hdiutil detach "$d" >/dev/null 2>&1 || hdiutil detach -force "$d" >/dev/null 2>&1 || true
  done
}

# sha256 de um arquivo, portátil. No macOS existe `shasum`; em muitas distros o
# que existe é `sha256sum`. Sem isto o caminho de Linux morreria justamente na
# verificação que este script existe para fazer.
_sha256() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  else echo ""; fi
}

# ── A chave do updater tem de existir ANTES de compilar ──────────────────────
# Com `bundle.createUpdaterArtifacts: true` + `plugins.updater.pubkey` no
# tauri.conf.json (ADR-022), o Tauri EXIGE `TAURI_SIGNING_PRIVATE_KEY` — e só
# descobre a ausência dela no FIM do empacotamento, depois de compilar tudo.
#
# Aconteceu na máquina Linux em 28/07/2026: **2m01s** de compilação, os três
# bundles gerados (.deb, .rpm, .AppImage), e então `exit 1` com "A public key has
# been found, but no private key". Este teste custa milissegundos e falha no
# primeiro segundo — e o erro do Tauri não diz onde a chave mora, este diz.
UPDATER_ARTIFACTS=1

# Onde a senha mora quando não vem do ambiente. Definido aqui, antes das funções
# que a citam, para não depender da ordem em que o corpo do script executa.
UPDATER_PASS_FILE="${SHVIA_UPDATER_PASS_FILE:-$HOME/.shvia/updater.pass}"

# O keyid (8 bytes) que mora DENTRO da pubkey declarada no tauri.conf.json — é
# ele que todo cliente já instalado usa para aceitar ou recusar um update.
updater_pubkey_id() {
  node -e '
    try {
      const c = require("./src-tauri/tauri.conf.json");
      const pub = Buffer.from(c.plugins.updater.pubkey, "base64").toString();
      const raw = Buffer.from(pub.trim().split("\n").pop(), "base64");
      process.stdout.write(raw.slice(2, 10).toString("hex").toUpperCase());
    } catch { process.stdout.write(""); }
  ' 2>/dev/null || true
}

# O mesmo keyid, lido de uma assinatura recém-gerada ($1 = arquivo .sig).
updater_sig_id() {
  node -e '
    try {
      const fs = require("fs");
      const sig = Buffer.from(fs.readFileSync(process.argv[1], "utf8").trim(), "base64").toString();
      const raw = Buffer.from(sig.trim().split("\n")[1], "base64");
      process.stdout.write(raw.slice(2, 10).toString("hex").toUpperCase());
    } catch { process.stdout.write(""); }
  ' "$1" 2>/dev/null || true
}

# ── A chave existir não é prova: ela tem de ABRIR e ser a chave CERTA ────────
# Duas coisas ainda podem estar erradas depois de TAURI_SIGNING_PRIVATE_KEY estar
# preenchida, e nenhuma das duas aparece antes do fim do empacotamento:
#
#   a) a SENHA não abre a chave — o `tauri build` aborta no último passo, e os
#      minutos de compilação vão junto;
#   b) a chave é de OUTRO par — e este é o caro. O build termina, o release sai,
#      e todo cliente já instalado RECUSA o update ("signature error"): quem está
#      na versão antiga fica preso nela, sem volta possível pelo próprio updater.
#      Não é hipótese: esta máquina tem mais de uma chave minisign no disco
#      (~/.shvia/updater.key, ~/.tauri/*.key, a do SSHVTERM) e a errada é
#      igualmente válida aos olhos do bundler.
#
# Assinar um arquivo descartável custa ~1s e elimina as duas.
verify_updater_key() {
  local cli tmpd sigfile sigid pubid
  cli="./node_modules/.bin/tauri"

  # O npm ci só roda depois deste passo: em árvore recém-clonada não há CLI para
  # a prova. Avisar e seguir é melhor que baixar a CLI aqui — a alternativa seria
  # rede no meio de um preflight que se vende como instantâneo.
  if [ ! -x "$cli" ]; then
    echo "    ⚠️ chave presente; prova de assinatura adiada (node_modules ainda não existe)."
    return 0
  fi

  pubid="$(updater_pubkey_id)"
  tmpd="$(mktemp -d)" || return 0
  printf 'shvia-updater-keycheck' > "$tmpd/probe"

  # Chave e senha viajam por AMBIENTE, nunca por argumento: argv é legível no `ps`
  # por qualquer processo desta máquina. E a senha vazia precisa estar EXPORTADA,
  # não apenas ausente — sem a variável a CLI abre prompt e o build para esperando
  # alguém que não está olhando a tela.
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"

  if ! "$cli" signer sign "$tmpd/probe" >"$tmpd/out" 2>&1 </dev/null; then
    echo "" >&2
    echo "  ✗ a chave do updater não abriu:" >&2
    echo "      $(grep -m1 -i 'error\|password\|key' "$tmpd/out" | sed 's/^ *//')" >&2
    echo "" >&2
    # Dizer o ESTADO do arquivo, não só o nome dele: "preencha X" é inútil quando
    # X já foi preenchido, e "senha errada" com o arquivo vazio é ruído.
    if [ -s "$UPDATER_PASS_FILE" ]; then
      echo "    A senha veio de $UPDATER_PASS_FILE e o rsign a recusou." >&2
      echo "    Confira contra o cofre — e olhe espaço sobrando no fim da linha" >&2
      echo "    (a quebra de linha o build já remove sozinho)." >&2
    else
      echo "    Falta a senha. Ela está no cofre e é a mesma nos três SOs (ADR-022)." >&2
      echo "    Cole no arquivo abaixo — só a senha, nada de export nem aspas:" >&2
      echo "" >&2
      echo "      \$EDITOR $UPDATER_PASS_FILE" >&2
      echo "" >&2
      echo "    O build lê esse arquivo sozinho, em toda máquina e todo terminal." >&2
      if [ "$_BUILD_OS" = macOS ]; then
        echo "    No Mac dá para usar o keychain no lugar do arquivo (1.1.8):" >&2
        echo "      security add-generic-password -U -s shvia-updater -a \"\$USER\" -w" >&2
      fi
    fi
    echo "" >&2
    rm -rf "$tmpd"
    return 1
  fi

  # `|| true` nos dois: com `set -o pipefail`, um SIGPIPE do find ou um .sig
  # ilegível derrubariam o build inteiro por causa da VERIFICAÇÃO — o preflight
  # não pode ser mais frágil que aquilo que ele protege.
  sigfile="$(find "$tmpd" -name '*.sig' -type f 2>/dev/null | head -1 || true)"
  sigid=""
  if [ -n "$sigfile" ]; then sigid="$(updater_sig_id "$sigfile")"; fi
  rm -rf "$tmpd"

  if [ -n "$pubid" ] && [ -n "$sigid" ] && [ "$pubid" != "$sigid" ]; then
    echo "" >&2
    echo "  ✗ chave do updater ERRADA — é de outro par de chaves." >&2
    echo "      assina com: $sigid" >&2
    echo "      publicado : $pubid   (pubkey no src-tauri/tauri.conf.json)" >&2
    echo "" >&2
    echo "    Publicar assim entrega um release que NENHUM cliente instalado" >&2
    echo "    aceita, e o updater não conserta a si mesmo depois. Aponte" >&2
    echo "    TAURI_SIGNING_PRIVATE_KEY para a chave do par publicado — nesta" >&2
    echo "    máquina há outras chaves minisign no disco, e todas 'funcionam'." >&2
    echo "" >&2
    return 1
  fi

  echo "    ✔ chave do updater: abre com a senha e confere com a pubkey (${sigid:-?})"
  return 0
}

check_updater_key() {
  local exige
  exige="$(node -e '
    try {
      const c = require("./src-tauri/tauri.conf.json");
      const tem = c && c.plugins && c.plugins.updater && c.plugins.updater.pubkey;
      process.stdout.write(tem && c.bundle && c.bundle.createUpdaterArtifacts ? "1" : "");
    } catch { process.stdout.write(""); }
  ' 2>/dev/null || true)"

  # Sem pubkey/createUpdaterArtifacts não há o que exigir.
  [ -z "$exige" ] && return 0

  # ⚠️ SÓ `TAURI_SIGNING_PRIVATE_KEY` serve. O `..._KEY_PATH` existe e funciona no
  # `tauri signer sign`, mas o BUNDLER (`tauri build`) o IGNORA — e só reclama no FIM
  # do empacotamento. Verificado na prática em 28/07/2026 num build do SSHVTERM: o
  # .app foi assinado e notarizado, o .dmg saiu, e só então veio "A public key has
  # been found, but no private key". Aceitar o KEY_PATH aqui derrotaria o propósito
  # deste teste, que é falhar no primeiro segundo em vez de no vigésimo minuto.
  #
  # Para não guardar a chave DENTRO do arquivo de credenciais, use substituição de
  # comando no signing.env: o arquivo fica com o caminho, a variável com o conteúdo.
  # Presente não basta — verify_updater_key prova que abre e que é a chave certa.
  if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    verify_updater_key
    return $?
  fi

  if [ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]; then
    echo "" >&2
    echo "  ✗ só TAURI_SIGNING_PRIVATE_KEY_PATH está definida, e o bundler a IGNORA." >&2
    echo "    (ela vale para o 'tauri signer sign', não para o 'tauri build'.)" >&2
    echo "" >&2
    echo "    No signing.env, troque por:" >&2
    echo "      export TAURI_SIGNING_PRIVATE_KEY=\"\$(cat ~/.shvia/updater.key)\"" >&2
    echo "" >&2
    return 1
  fi

  # --no-sign já significa "build de teste, não publicável" no macOS. Estendido
  # aqui para os três SOs: desliga o artefato de updater em vez de abortar.
  if [ "$NO_SIGN" -eq 1 ]; then
    UPDATER_ARTIFACTS=0
    echo "    ⚠️ sem TAURI_SIGNING_PRIVATE_KEY e com --no-sign: build de TESTE."
    echo "       Sai SEM artefato de updater — NÃO publique este build."
    return 0
  fi

  echo "" >&2
  echo "  ✗ falta a chave do updater, e este build gera artefato de updater." >&2
  echo "    Sem ela o Tauri aborta — mas só no FIM do empacotamento (minutos)." >&2
  echo "" >&2
  echo "    Resolva UMA VEZ nesta máquina, e não a cada build — dois arquivos," >&2
  echo "    sem export e sem sintaxe de shell:" >&2
  echo "" >&2
  echo "      ~/.shvia/updater.key    a chave (copie da outra máquina de release)" >&2
  echo "      ~/.shvia/updater.pass   A SENHA e mais nada" >&2
  echo "" >&2
  echo "    O build lê os dois sozinho, em qualquer terminal, para sempre." >&2
  echo "    Também aceita ./signing.env ou ~/.config/shvia/build.env (modelo:" >&2
  echo "    signing.env.example) e um export pontual — mas nada disso é preciso." >&2
  echo "" >&2
  echo "    O par é UM SÓ para as três máquinas (ADR-022): a mesma chave que" >&2
  echo "    assinou o release do macOS assina o do Linux e do Windows. Copie do" >&2
  echo "    gerenciador de senhas — nunca por chat." >&2
  echo "" >&2
  echo "    Build de teste, sem chave: ./build-local.sh --no-sign" >&2
  echo "" >&2
  return 1
}

# ── Reaproveitar build (evita recompilar o que já está pronto) ────────────────
# Devolve 0 quando os artefatos no disco são PROVADAMENTE desta versão e nada
# mudou desde que foram gerados. Motivador: esquecer o `--publish` custava um
# rebuild inteiro só para subir arquivo que já existia.
#
# "O arquivo existe" NÃO é prova suficiente, e o macOS mostra por quê: o artefato
# do updater é `ShvIA.app.tar.gz`, SEM versão no nome. A identidade dele vem do
# `sha256` gravado no release.json pelo build que o produziu. Então o teste é:
#
#   1. release.json existe e é da versão de version.md;
#   2. todo artefato que ele declara para ESTA plataforma existe no disco;
#   3. o sha256 de cada um ainda confere — é isto que prova que o arquivo sem
#      versão no nome é o desta versão, e não sobra de um build anterior;
#   4. nenhuma FONTE é mais nova que o artefato mais antigo.
#
# O passo 4 é o que impede o pior resultado possível deste atalho: editar código
# sem bumpar a versão passaria em 1-3 (o release.json antigo continua descrevendo
# os binários antigos corretamente) e publicaríamos **binário velho como se fosse
# a versão nova** — silencioso e assinado, o que é pior que um erro.
can_reuse_build() {
  [ "$FORCE_BUILD" -eq 1 ] && return 1
  [ -f release.json ] || return 1

  local ver_disco ver_manifesto
  ver_disco="$(tr -d ' \t\n\r' < version.md)"
  ver_manifesto="$(node -e '
    try { console.log(JSON.parse(require("fs").readFileSync("release.json","utf8")).version || ""); }
    catch { console.log(""); }
  ')"
  if [ -z "$ver_manifesto" ] || [ "$ver_disco" != "$ver_manifesto" ]; then
    return 1
  fi

  # Artefatos declarados para esta plataforma + hash esperado.
  local linhas nome esperado caminho ref="" n=0
  # shellcheck disable=SC2016  # `${...}` aqui é template literal de JS, não de bash
  linhas="$(node -e '
    const m = JSON.parse(require("fs").readFileSync("release.json","utf8"));
    const p = {darwin:"macos", linux:"linux", win32:"windows"}[process.platform];
    for (const a of (m.platforms?.[p]?.artifacts ?? [])) console.log(`${a.file}\t${a.sha256}`);
  ' 2>/dev/null)" || return 1
  [ -z "$linhas" ] && return 1

  while IFS=$'\t' read -r nome esperado; do
    [ -z "$nome" ] && continue
    caminho="$(find src-tauri/target/release/bundle -maxdepth 3 -type f -name "$nome" -print -quit 2>/dev/null || true)"
    [ -z "$caminho" ] && return 1
    [ "$(_sha256 "$caminho")" = "$esperado" ] || return 1
    # Referência de tempo: o artefato MAIS ANTIGO (o mais estrito).
    if [ -z "$ref" ] || [ "$caminho" -ot "$ref" ]; then ref="$caminho"; fi
    n=$((n + 1))
  done <<< "$linhas"
  [ "$n" -eq 0 ] && return 1
  [ -z "$ref" ] && return 1

  # Passo 4: alguma fonte mudou depois do build?
  # `version:sync` só reescreve os manifests quando a versão muda de verdade
  # (scripts/sync-version.mjs), então incluí-los aqui não gera falso positivo.
  # `src-tauri/binaries/` entra por causa do D5: um `anna` novo empacotado exige
  # rebuild, e ele não aparece em nenhuma outra fonte.
  local novas
  novas="$(find src src-tauri/src src-tauri/capabilities src-tauri/binaries \
                index.html package.json vite.config.ts tsconfig.json \
                src-tauri/Cargo.toml src-tauri/tauri.conf.json \
             -type f -newer "$ref" -print -quit 2>/dev/null || true)"
  if [ -n "$novas" ]; then
    REUSE_MOTIVO="fonte mais nova que o build: $novas"
    return 1
  fi

  REUSE_REF="$ref"; REUSE_N="$n"
  return 0
}

# ── Publicação no servidor (item D1) ─────────────────────────────────────────
# Destino e base pública são CONSTANTES documentadas, sobrescrevíveis por env ou
# flag. Não há segredo aqui: o destino é um host da tailnet e a base é a URL que o
# app já usa. A senha do scp NUNCA entra em variável nem em arquivo — o scp
# pergunta, ou você instala a chave com ssh-copy-id.
PUBLISH_DEST="${SHVIA_PUBLISH_DEST:-root@100.64.100.242:/srv/shvia/storage/app/public/desktop/}"
PUBLIC_BASE="${SHVIA_PUBLIC_BASE:-https://ai.shvia.org}"

# Baixa o release.json JÁ PUBLICADO para a raiz, para o release-manifest.mjs
# mesclar a plataforma desta máquina em cima dele em vez de recomeçar.
#
# Sem este passo, o fluxo "cada SO numa máquina" perde entradas: o macOS publica
# um manifesto só-macOS, e a entrada do Windows que estava no servidor desaparece.
# O sintoma é o pior possível — nada falha no build, nada falha no endpoint, e os
# usuários de Windows simplesmente param de receber update.
#
# Fail-open: sem rede, sem manifesto publicado ou JSON ilegível, segue sem mesclar
# (o release-manifest.mjs recomeça, que é o comportamento de antes deste passo).
fetch_remote_manifest() {
  local url="$PUBLIC_BASE/storage/desktop/release.json"
  local tmp; tmp="$(mktemp)"
  if ! curl -fsS --max-time 20 -o "$tmp" "$url" 2>/dev/null; then
    echo "    (sem manifesto publicado em $url — nada a mesclar)"
    rm -f "$tmp"; return 0
  fi
  if ! node -e 'JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"))' "$tmp" 2>/dev/null; then
    echo "    ⚠️ o release.json publicado não é JSON válido — ignorando"
    rm -f "$tmp"; return 0
  fi
  # shellcheck disable=SC2016  # `${...}` aqui é template literal de JS, não de bash
  local plats; plats="$(node -e '
    const m = JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"));
    process.stdout.write(`${m.version} [${Object.keys(m.platforms||{}).join(", ")}]`);
  ' "$tmp")"
  mv "$tmp" release.json
  echo "    publicado hoje: $plats — o manifesto novo mescla em cima disto"
}

# Sobe os artefatos DESTA plataforma + o release.json, e verifica pela URL pública.
publish_release() {
  if [ ! -f release.json ]; then
    echo "  ⚠️ sem release.json — nada a publicar." >&2
    return 1
  fi

  # A lista sai do PRÓPRIO manifesto: garante que sobe exatamente o que ele
  # declara. Um arquivo declarado e não encontrado no disco é ERRO, não aviso —
  # publicar um manifesto que aponta para arquivo ausente é justamente o 404 que
  # só aparece quando o app tenta baixar.
  local arquivos=() faltando=() nome caminho
  while IFS= read -r nome; do
    [ -z "$nome" ] && continue
    caminho="$(find src-tauri/target/release/bundle -maxdepth 3 -type f -name "$nome" -print -quit 2>/dev/null || true)"
    if [ -z "$caminho" ]; then faltando+=("$nome"); continue; fi
    arquivos+=("$caminho")
    [ -f "$caminho.sha256" ] && arquivos+=("$caminho.sha256")
  done < <(node -e '
    const m = JSON.parse(require("fs").readFileSync("release.json","utf8"));
    const p = {darwin:"macos", linux:"linux", win32:"windows"}[process.platform];
    for (const a of (m.platforms?.[p]?.artifacts ?? [])) console.log(a.file);
  ')

  if [ "${#faltando[@]}" -gt 0 ]; then
    echo "  ✗ declarados no release.json e AUSENTES no bundle: ${faltando[*]}" >&2
    echo "    (rode o build antes de publicar — não vou publicar manifesto quebrado)" >&2
    return 1
  fi
  if [ "${#arquivos[@]}" -eq 0 ]; then
    echo "  ⚠️ o manifesto não lista artefato desta plataforma — nada a publicar." >&2
    return 1
  fi

  echo "    destino: $PUBLISH_DEST"
  for f in "${arquivos[@]}"; do echo "      $(basename "$f")"; done
  echo "      release.json"
  echo "    (uma senha só — é um scp com todos os arquivos)"

  # UM scp: uma conexão, um prompt de senha.
  scp "${arquivos[@]}" release.json "$PUBLISH_DEST"

  # ── Verificação PELA URL PÚBLICA ────────────────────────────────────────────
  # Não basta o arquivo estar no diretório: tem de ser servido, e chegar inteiro.
  # Upload truncado dá 200 com bytes errados, e aí o updater falha na verificação
  # de assinatura com uma mensagem que não explica nada.
  step "[D1] verifica a publicação pela URL pública"
  local alvo esperado obtido
  # shellcheck disable=SC2016  # `${...}` aqui é template literal de JS, não de bash
  alvo="$(node -e '
    const m = JSON.parse(require("fs").readFileSync("release.json","utf8"));
    const p = {darwin:"macos", linux:"linux", win32:"windows"}[process.platform];
    const a = (m.platforms?.[p]?.artifacts ?? []).find(x => x.signature);
    if (a) console.log(`${a.file}\t${a.sha256}`);
  ')"
  if [ -z "$alvo" ]; then
    echo "    ⚠️ nenhum artefato ASSINADO nesta plataforma — o auto-update não vai"
    echo "       oferecer esta versão. Faltou TAURI_SIGNING_PRIVATE_KEY no build?"
    return 0
  fi
  nome="${alvo%%$'\t'*}"; esperado="${alvo##*$'\t'}"

  obtido="$(curl -fsS --max-time 300 "$PUBLIC_BASE/storage/desktop/$nome" \
            | shasum -a 256 | awk '{print $1}')" || obtido=""
  if [ "$obtido" = "$esperado" ]; then
    echo "    ✅ $nome servido e íntegro (${esperado:0:16}…)"
  else
    echo "    ✗ $nome NÃO confere pela URL pública" >&2
    echo "      esperado: $esperado" >&2
    echo "      obtido:   ${obtido:-<falhou ao baixar>}" >&2
    echo "      O endpoint vai devolver 200 e o app vai falhar no DOWNLOAD." >&2
    return 1
  fi
}

SKIP_NPM_CI=0
NO_SIGN=0
SKIP_GIT_PULL=0
PUBLISH=0
FORCE_BUILD=0
BUNDLES=""
# Preenchidos por can_reuse_build para o relatório de "por que reusei / por que não".
REUSE_REF=""
REUSE_N=0
REUSE_MOTIVO=""
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-npm-ci)  SKIP_NPM_CI=1 ;;
    --no-sign)      NO_SIGN=1 ;;
    --skip-git-pull) SKIP_GIT_PULL=1 ;;
    --publish)      PUBLISH=1 ;;
    --force|-f)     FORCE_BUILD=1 ;;
    --dest)         shift; PUBLISH_DEST="${1:-}" ;;
    --base-url)     shift; PUBLIC_BASE="${1:-}" ;;
    --bundles)      shift; BUNDLES="${1:-}" ;;
    --anna)         shift; ANNA_FROM="${1:-}" ;;
    --no-anna)      NO_ANNA=1 ;;
    -h|--help)      usage; exit 0 ;;
    *) echo "opção desconhecida: $1 (use --help)" >&2; exit 2 ;;
  esac
  shift
done

echo "==> ShvIA Desktop — build local ($_BUILD_OS)"

# ── Credenciais desta máquina, uma vez em vez de a cada build ────────────────
# Nasceu de um atrito real: sem isto, cada release exigia reexportar
# TAURI_SIGNING_PRIVATE_KEY(_PASSWORD) na mão, e esquecer significava descobrir no
# fim do empacotamento. Ver signing.env.example (versionado; o preenchido é
# gitignorado) — ele guarda o CAMINHO da chave e a senha, não a chave.
#
# DOIS lugares aceitos, nesta ordem — o primeiro que existir vence:
#
#   1. ./signing.env             — dentro do repo, gitignorado.
#   2. ~/.config/shvia/build.env — FORA do repo. Sobrevive a clone novo, a
#      `git clean -xdf` e a apagar a árvore inteira. É o mesmo endereço que o
#      SSHVTERM-DESKTOP usa (~/.config/sshvterm/build.env): a máquina de release é
#      a mesma, e um hábito só para os dois repos é menos coisa para lembrar.
#
# Outro caminho: $SHVIA_BUILD_ENV. O arquivo é o mesmo modelo nos três casos.
#
# ANUNCIA que carregou e DE ONDE, de propósito: "o build saiu assinado ou não" não
# pode depender de um arquivo invisível. Se algo estiver estranho, a primeira linha
# da saída já diz de onde vieram as credenciais.
#
# O `.example` usa `${VAR:-...}`, então um export feito no shell VENCE o arquivo —
# um teste pontual não é sobrescrito por ele.
CREDS_FILE=""
for _c in "${SHVIA_BUILD_ENV:-}" "./signing.env" "$HOME/.config/shvia/build.env"; do
  [ -n "$_c" ] && [ -f "$_c" ] && { CREDS_FILE="$_c"; break; }
done
if [ -n "$CREDS_FILE" ]; then
  # shellcheck source=/dev/null
  . "$CREDS_FILE"
  echo "    credenciais: $CREDS_FILE carregado"
fi

# ── O caminho que NÃO depende de shell: dois arquivos em ~/.shvia/ ───────────
# O arquivo de credenciais acima é sintaxe de shell, e o `export` no terminal
# morre junto com o terminal. As duas coisas produzem o mesmo estrago: o build
# assinava ontem e hoje não, sem nada ter mudado no repo.
#
# Então, sem configuração nenhuma, o build procura por CONTEÚDO no lugar óbvio:
#
#   ~/.shvia/updater.key    — a chave (já era o endereço de sempre)
#   ~/.shvia/updater.pass   — A SENHA e mais nada. Sem `export`, sem aspas, sem
#                             `${VAR:-}`: abre, cola do cofre, salva.
#
# Um `export` feito no shell continua vencendo (teste pontual), e quem preferir o
# signing.env também continua funcionando — isto é só o piso, para que uma máquina
# que TEM a chave e a senha no disco nunca mais precise lembrar de nada.
if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && [ -s "$HOME/.shvia/updater.key" ]; then
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.shvia/updater.key")"
  export TAURI_SIGNING_PRIVATE_KEY
  echo "    chave do updater: ~/.shvia/updater.key"
fi

if [ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ] && [ -s "$UPDATER_PASS_FILE" ]; then
  # `head -1` e `tr -d '\r\n'`: o arquivo é feito à mão num editor, e editor põe
  # quebra de linha no fim. Um "\n" a mais é senha errada para o rsign, e o erro
  # ("Wrong password") não sugere em momento nenhum que o problema é invisível.
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(head -1 "$UPDATER_PASS_FILE" | tr -d '\r\n')"
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD
  echo "    senha do updater: $UPDATER_PASS_FILE"
fi

# No macOS há um cofre melhor que arquivo, e a 1.1.8 passou a usá-lo: o keychain,
# o mesmo lugar onde a senha de notarização (`shvia-notarize`) já mora. Fica DEPOIS
# do arquivo por um motivo prático: `security` não existe no Linux nem no Windows,
# e o `signing.env` que chama isso direto devolve senha VAZIA fora do Mac — sem
# erro, sem aviso, e o build só morre lá no fim. Aqui cada SO usa o que tem.
if [ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ] && command -v security >/dev/null 2>&1; then
  _kc="$(security find-generic-password -s shvia-updater -w 2>/dev/null || true)"
  if [ -n "$_kc" ]; then
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$_kc"
    export TAURI_SIGNING_PRIVATE_KEY_PASSWORD
    echo "    senha do updater: keychain (shvia-updater)"
  fi
  unset _kc
fi

step "[git] sincroniza com o remoto (git pull --ff-only)"
git_sync

# ── Reusar ou construir? ──────────────────────────────────────────────────────
# Esquecer o `--publish` custava um rebuild inteiro para subir arquivo que já
# existia. Ver can_reuse_build: reusa só quando os artefatos são PROVADAMENTE
# desta versão (sha256 do release.json) e nenhuma fonte mudou desde então.
step "[reuso] há build desta versão pronto no disco?"
if can_reuse_build; then
  REUSE=1
  echo "    ✅ sim — $REUSE_N artefato(s) da $(tr -d ' \t\n\r' < version.md) com sha256 conferido."
  # De QUANDO é o build reusado: sem isto, "reusei" é uma afirmação que não se
  # pode auditar, e reusar build de ontem sem perceber é o susto que o passo 4 do
  # can_reuse_build existe para impedir.
  if [ -n "$REUSE_REF" ]; then
    if _dt="$(date -r "$REUSE_REF" '+%d/%m %H:%M' 2>/dev/null)"; then
      echo "       gerado em $_dt"
    fi
  fi
  echo "       Pulando npm ci / version:sync / tauri build."
  echo "       Para reconstruir: --force (ou apague src-tauri/target/release/bundle)."
else
  REUSE=0
  if [ "$FORCE_BUILD" -eq 1 ]; then
    echo "    não (--force)"
  elif [ -n "$REUSE_MOTIVO" ]; then
    echo "    não — $REUSE_MOTIVO"
  else
    echo "    não — sem build desta versão no disco (ou artefato/hash divergente)"
  fi
fi

if [ "$REUSE" -eq 0 ]; then
  step "[pré-requisitos] verifica o toolchain (Rust, Node, Xcode/WebKitGTK)"
  preflight
  # ANTES de compilar: sem a chave do updater, o Tauri só reclamaria no fim.
  check_updater_key

  step "[1/3] dependências do frontend (npm ci)"
  if [ "$SKIP_NPM_CI" -eq 0 ]; then
    npm ci
  else
    echo "    (pulado: --skip-npm-ci)"
  fi

  step "[2/3] sincroniza versão (version.md -> manifests)"
  npm run version:sync

  # Ejeta imagens .dmg montadas de um build anterior ANTES do rm abaixo: se um rw.*.dmg
  # ainda está attachado e apagamos o arquivo de origem, sobra um volume "ShvIA" órfão
  # em /Volumes/ que quebra o AppleScript do próximo bundle_dmg.sh (ver a função).
  detach_stale_build_images

  # Limpa instaladores de builds anteriores (padrão SHVTERM): o bundle dir acumula
  # .deb/.AppImage/.rpm de versões antigas (ex.: ShvIA_0.4.6 ao lado do 0.5.0).
  # Instalar o errado faz o app rodar versão velha — só o artefato do build ATUAL
  # deve sobrar na listagem final.
  rm -rf src-tauri/target/release/bundle

  if [ "$_BUILD_OS" = macOS ]; then
    step "[macOS] assinatura + notarização (Developer ID + notarytool)"
    setup_macos_signing
  fi

  # ── AppImage (Linux): destrava o linuxdeploy nesta e em qualquer VM ──────────────
  # App WebKitGTK tem árvore de deps ENORME. Por padrão o linuxdeploy roda um
  # `dpkg-query` de copyright POR biblioteca — em VM isso arrasta por minutos e o build
  # morria com "failed to run linuxdeploy" (reproduzido: >120s travado no dpkg-query;
  # com as env abaixo, 41s e "Success"). Também há FUSE aninhado (linuxdeploy e
  # appimagetool são AppImages). As env são lidas direto por essas ferramentas:
  #   DISABLE_COPYRIGHT_FILES_DEPLOYMENT → pula o dpkg-query de copyright (o gargalo)
  #   APPIMAGE_EXTRACT_AND_RUN → extrai+roda os AppImages (sem depender de FUSE aninhado)
  #   NO_STRIP → não faz strip (rpath $ORIGIN já bloqueava caso a caso; evita o passo)
  #   ARCH → o appimagetool exige a arquitetura explícita
  if [ "$_BUILD_OS" = Linux ]; then
    ARCH="$(uname -m)"; export ARCH
    export DISABLE_COPYRIGHT_FILES_DEPLOYMENT=1
    export APPIMAGE_EXTRACT_AND_RUN=1
    export NO_STRIP=1
  fi

  # ── Motor empacotado (item D5) ────────────────────────────────────────────────
  # ANTES do `tauri build`: o Tauri lê `bundle.externalBin` na hora de empacotar, e
  # um binário que chegue depois simplesmente não entra no bundle.
  step "[D5] motor (anna) para dentro do bundle"
  if [ "$NO_ANNA" = "1" ]; then
    echo "    (pulado: --no-anna — o app sairá SEM Modo Code pronto)"
    rm -rf src-tauri/binaries
  elif [ -n "$ANNA_FROM" ]; then
    node scripts/stage-anna.mjs --from "$ANNA_FROM"
  else
    node scripts/stage-anna.mjs
  fi

  step "[3/3] Tauri build"
  # Quatro braços explícitos em vez de montar array de argumentos: o bash do macOS
  # é 3.2, e ali `"${arr[@]}"` de array VAZIO com `set -u` aborta com "unbound
  # variable" (testado). Verboso, mas roda nos três SOs.
  _CFG_SEM_UPDATER='{"bundle":{"createUpdaterArtifacts":false}}'
  if [ -n "$BUNDLES" ] && [ "$UPDATER_ARTIFACTS" -eq 0 ]; then
    npx tauri build --bundles "$BUNDLES" --config "$_CFG_SEM_UPDATER"
  elif [ -n "$BUNDLES" ]; then
    npx tauri build --bundles "$BUNDLES"
  elif [ "$UPDATER_ARTIFACTS" -eq 0 ]; then
    npx tauri build --config "$_CFG_SEM_UPDATER"
  else
    npx tauri build
  fi


  if [ "$_BUILD_OS" = macOS ]; then
    step "[macOS] verificação (codesign / spctl / stapler)"
    verify_macos_signature
  fi
fi

# ── Pacote Arch Linux (.pkg.tar.zst): conversão do .deb via fpm ───────────────
# O bundler do Tauri (2.x) só conhece deb/rpm/appimage no Linux; o pacote pacman
# sai convertendo o .deb recém-gerado com o fpm — na própria máquina Debian, sem
# precisar de um host Arch. Dependências trocadas À MÃO pelos nomes do Arch
# (--no-auto-depends): os nomes Debian (libwebkit2gtk-4.1-0, libgtk-3-0) não
# existem no pacman e o pacote sairia ininstalável; libayatana-appindicator
# porque o app usa tray-icon (Cargo.toml). Best-effort: sem fpm, avisa e segue.
# Idempotente: se o .pkg desta versão já está no disco (reuso), pula.
# NB: fica FORA do release.json/publish por enquanto — o fpm não gera o .sig do
# updater, e o endpoint do ShvIA trata artefato sem assinatura como inexistente;
# entrar no manifesto quebraria a verificação da publicação. Distribuição manual
# até o endpoint servir artefato de download sem assinatura.
if [ "$_BUILD_OS" = Linux ]; then
  _versao_pkg="$(tr -d ' \t\n\r' < version.md)"
  _bundle_pkg=src-tauri/target/release/bundle
  _deb_pkg="$(find "$_bundle_pkg/deb" -maxdepth 1 -name "*_${_versao_pkg}_*.deb" 2>/dev/null | head -1)"
  if [ -n "$_deb_pkg" ] && \
     ! find "$_bundle_pkg/pacman" -name "*${_versao_pkg}*.pkg.tar.*" 2>/dev/null | grep -q .; then
    if command -v fpm >/dev/null 2>&1; then
      echo "==> Linux: convertendo o .deb em pacote Arch (.pkg.tar.zst) via fpm..."
      mkdir -p "$_bundle_pkg/pacman"
      _deb_abs="$(cd "$(dirname "$_deb_pkg")" && pwd)/$(basename "$_deb_pkg")"
      # -a explícito: amd64 no mundo deb, x86_64 no pacman — o fpm não traduz sozinho.
      if (cd "$_bundle_pkg/pacman" && fpm -s deb -t pacman --no-auto-depends \
            -d webkit2gtk-4.1 -d gtk3 -d libayatana-appindicator \
            --pacman-compression zstd -a "$(uname -m)" "$_deb_abs" >/dev/null); then
        echo "    ✓ $(find "$_bundle_pkg/pacman" -name '*.pkg.tar.*' 2>/dev/null | head -1)"
      else
        echo "AVISO: fpm falhou ao gerar o pacote Arch — o build segue sem ele." >&2
      fi
    else
      echo "AVISO: sem fpm no PATH — pulando o pacote Arch (.pkg.tar.zst)." >&2
      echo "       instale: sudo apt install ruby ruby-dev build-essential zstd libarchive-tools && sudo gem install fpm" >&2
    fi
  fi
fi

# Com reuso, a listagem abaixo mostra o que será publicado — o mesmo que o
# build imprimiria, e é a confirmação visual de qual artefato está indo.
echo ""
echo "[OK] Instaladores em src-tauri/target/release/bundle/:"
find src-tauri/target/release/bundle -maxdepth 2 -type f \
  \( -name '*.deb' -o -name '*.AppImage' -o -name '*.rpm' -o -name '*.dmg' \
     -o -name '*.app.tar.gz' -o -name '*.pkg.tar.*' \) -exec ls -lh {} \; 2>/dev/null || true


# ── Checksums + release.json (item D9) ────────────────────────────────────────
# DEPOIS da assinatura/notarização de propósito: assinar e stapler ALTERAM os
# bytes do artefato, então um sha256 calculado antes descreveria um arquivo que
# não existe mais — e o updater recusaria o download por hash divergente.
# Com --publish, o manifesto publicado vem ANTES para o merge de plataformas
# acontecer sozinho. Ver o comentário de fetch_remote_manifest: sem isso, publicar
# do macOS apaga a entrada do Windows do servidor sem nenhum sintoma.
if [ "$PUBLISH" -eq 1 ]; then
  step "[D1] baixa o manifesto publicado (para mesclar as outras plataformas)"
  fetch_remote_manifest
fi

step "[D9] checksums + release.json"
node scripts/release-manifest.mjs || echo "  (manifesto não gerado — build segue válido)"

# ── Publicação (item D1) ─────────────────────────────────────────────────────
# Depois do manifesto de propósito: é ele que diz o que subir.
if [ "$PUBLISH" -eq 1 ]; then
  step "[D1] publica no servidor (scp)"
  publish_release
fi

_summary
