#!/usr/bin/env bash
# build-local.sh — Build LOCAL do ShvIA Desktop no macOS e Linux.
# Gera os instaladores do app. O ShvIA é shell fino: SEM sidecar.
#
# O BUILD DE RELEASE É LOCAL POR DECISÃO: a matriz de CI (tauri-action) foi removida na
# 0.4.6 por custo e não foi restaurada — mesmo com a conta em GitHub Enterprise (50.000
# min/mês), runners macOS custam 10x. Testes/lint rodam via ci.yml; empacotamento não.
# Este script É o pipeline — inclusive checksums e manifesto
# (release.json), que o item D9 acrescentou e o D1 (auto-update) vai consumir.
#   macOS  -> .dmg + .app.tar.gz
#   Linux  -> .deb + .AppImage (+ .rpm)   (targets="all" do tauri.conf.json)
#             + .pkg.tar.zst (Arch)       — nativo por makepkg quando o build roda
#                                           NUM Arch; o mesmo conteúdo remontado por
#                                           fpm quando roda num Debian
#
# PRÉ-REQUISITOS (instalar uma vez):
#   Comum:   Node 20, Rust (rustup default stable)
#   macOS:   xcode-select --install
#   Linux (Debian/Ubuntu):
#     sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
#       libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
#   Linux (Arch):
#     sudo pacman -S --needed base-devel webkit2gtk-4.1 curl wget file openssl \
#       libayatana-appindicator librsvg xdotool patchelf
#     (base-devel traz makepkg + fakeroot, que empacotam o .pkg.tar.zst)
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
#   ./build-local.sh --publish --dest usuario@HOST:/caminho/   # outro destino
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
#     digitar nada, `ssh-copy-id b3sys@HOST` uma vez e o scp passa a usar a chave.
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
# SIGNING (macOS): an unsigned app is "damaged" to macOS, which offers to MOVE IT TO THE
# TRASH when it is opened after a download (the quarantine attribute). On macOS this script
# signs with the keychain's **Developer ID Application** cert, and `tauri build` **notarizes +
# staples** by itself when there is a notarization credential. Since 1.6.39 the credential is
# an **App Store Connect API key**: `~/.shvia/AuthKey_<KEYID>.p8` plus the issuer ID in
# `~/.shvia/apple-api-issuer` (or APPLE_API_KEY / APPLE_API_ISSUER / APPLE_API_KEY_PATH). The
# old app-specific password in the keychain (service "shvia-notarize") still works, with a
# warning: it travels in notarytool's command line. Never in the repo. docs/build.md, "macOS".
# No cert -> unsigned build; no credential -> signed but not notarized.
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

# ── Qual Linux? ──────────────────────────────────────────────────────────────
# Duas coisas dependem disto e nenhuma é cosmética: (1) o comando de instalar
# dependência que o preflight sugere — dizer "apt-get install" para quem está num
# Arch é uma mensagem de erro que não resolve; (2) COMO o pacote pacman é gerado
# (makepkg nativo aqui, fpm convertendo o .deb lá). Ver o bloco do .pkg.tar.zst.
#
# `ID_LIKE` entra junto porque derivada é o caso comum: Manjaro/EndeavourOS têm
# ID próprio e `ID_LIKE=arch`; Ubuntu/Mint têm `ID_LIKE=debian`. Ler só o `ID`
# faria o script tratar um EndeavourOS como distro desconhecida.
_LINUX_FAMILIA=""   # arch | debian | "" (desconhecida ou não-Linux)
if [ "$_BUILD_OS" = Linux ] && [ -r /etc/os-release ]; then
  # shellcheck source=/dev/null  # arquivo do SISTEMA, não do repo — nada a seguir
  _os_id="$(. /etc/os-release 2>/dev/null; printf '%s' "${ID:-}")"
  # shellcheck source=/dev/null
  _os_like="$(. /etc/os-release 2>/dev/null; printf '%s' "${ID_LIKE:-}")"
  case " $_os_id $_os_like " in
    *" arch "*)   _LINUX_FAMILIA=arch   ;;
    *" debian "*) _LINUX_FAMILIA=debian ;;
    *" ubuntu "*) _LINUX_FAMILIA=debian ;;
  esac
  unset _os_id _os_like
fi
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

  # Linux: o WebKitGTK dev é obrigatório pro WebView do Tauri. O pacote tem nome
  # diferente em cada família, e sugerir o comando da família ERRADA é o mesmo que
  # não sugerir nada — quem está no Arch não tem apt-get para rodar.
  if [ "$_BUILD_OS" = Linux ] && command -v pkg-config >/dev/null 2>&1 \
     && ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    case "$_LINUX_FAMILIA" in
      arch)
        missing+=(
"webkit2gtk-4.1 ausente (e outras deps do Tauri no Linux). Instale:
       sudo pacman -S --needed base-devel webkit2gtk-4.1 curl wget file openssl \\
         libayatana-appindicator librsvg xdotool patchelf"
        )
        ;;
      *)
        missing+=(
"libwebkit2gtk-4.1-dev ausente (e outras deps do Tauri no Linux). Instale:
       sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \\
         libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf"
        )
        ;;
    esac
  fi

  # Arch: makepkg/fakeroot empacotam o .pkg.tar.zst no fim do build. NÃO entram em
  # `missing` — faltar empacotador do pacman não é motivo para abortar um build que
  # ainda produz .deb, .rpm e AppImage. Aviso agora, no primeiro segundo, em vez de
  # 20 minutos adiante, quando o bloco do pacote roda.
  if [ "$_LINUX_FAMILIA" = arch ] && ! command -v makepkg >/dev/null 2>&1; then
    echo "    ⚠️ sem makepkg (grupo base-devel) — o build sai SEM o pacote Arch."
    echo "       sudo pacman -S --needed base-devel"
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

# ── macOS: signing (Developer ID) + notarization ─────────────────────────────
# Without this, a downloaded app (quarantined) is rejected by Gatekeeper and macOS offers
# to MOVE IT TO THE TRASH. Flow:
#   1) finds the "Developer ID Application" cert in the keychain and exports
#      APPLE_SIGNING_IDENTITY — `tauri build` signs the .app (hardened runtime).
#   2) picks the notarization credential (escolhe_credencial_de_notarizacao) and exports it —
#      `tauri build` then NOTARIZES and STAPLES the .app by itself.
# No secret ever enters the repo.
NOTARY_SERVICE="shvia-notarize"
SIGN_ENABLED=0
NOTARIZE_ENABLED=0
NOTARIZE_MODE=""   # "api" (App Store Connect key) or "senha" (app-specific password)

# 🔴 Which credential notarizes (1.6.39, E20). The app-specific password went to notarytool as
# `--password`, on the command line of a process that runs for minutes while Apple answers:
# readable in `ps` by anything on this Mac for that long. Changing only OUR call (the .dmg)
# closed nothing — Tauri's bundler notarizes the .app with APPLE_ID/APPLE_PASSWORD the same way.
# An App Store Connect API key has no secret in argv: notarytool gets the PATH of the .p8.
#
# Order: an API key complete in the environment; else one ~/.shvia/AuthKey_<KEYID>.p8 plus the
# issuer ID (APPLE_API_ISSUER or ~/.shvia/apple-api-issuer); else the keychain password, with a
# warning; else nothing (signed, not notarized — and 1.6.22 refuses to publish that).
# In API mode APPLE_ID/APPLE_PASSWORD are UNSET: Tauri must not be left to choose between two
# credentials, and the password is exactly what this exists to keep out of argv.
escolhe_credencial_de_notarizacao() {
  local chaves n issuer_arq="$HOME/.shvia/apple-api-issuer" falta="" nome_do_arq
  chaves="$(find "$HOME/.shvia" -maxdepth 1 -name 'AuthKey_*.p8' 2>/dev/null | sort || true)"
  n="$(printf '%s' "$chaves" | grep -c . || true)"
  # Any sign of an API key is intent: from here on, what is missing is said out loud instead
  # of the build quietly going back to the password.
  if [ -n "${APPLE_API_KEY_PATH:-}${APPLE_API_KEY:-}${APPLE_API_ISSUER:-}" ] || [ "$n" -gt 0 ] \
     || [ -f "$issuer_arq" ]; then
    if [ -z "${APPLE_API_KEY_PATH:-}" ]; then
      if [ -n "${APPLE_API_KEY:-}" ]; then
        # The Key ID names its file; another key file is never a stand-in for it.
        [ -f "$HOME/.shvia/AuthKey_${APPLE_API_KEY}.p8" ] \
          && APPLE_API_KEY_PATH="$HOME/.shvia/AuthKey_${APPLE_API_KEY}.p8"
      elif [ "$n" -eq 1 ]; then
        APPLE_API_KEY_PATH="$chaves"
      elif [ "$n" -gt 1 ]; then
        echo "  ✗ mais de uma chave da App Store Connect em ~/.shvia — não escolho por você:" >&2
        printf '%s\n' "$chaves" | sed 's/^/      /' >&2
        echo "    Apague a revogada, ou diga qual: export APPLE_API_KEY=<KEYID>" >&2
        return 1
      fi
    fi
    nome_do_arq="$(basename "${APPLE_API_KEY_PATH:-}" | sed -n 's/^AuthKey_\([A-Z0-9]\{1,\}\)\.p8$/\1/p')"
    if [ -z "${APPLE_API_KEY:-}" ]; then
      APPLE_API_KEY="$nome_do_arq"
    fi
    if [ -z "${APPLE_API_ISSUER:-}" ] && [ -f "$issuer_arq" ]; then
      APPLE_API_ISSUER="$(tr -d ' \t\r\n' < "$issuer_arq")"
    fi
    if [ -z "${APPLE_API_KEY_PATH:-}" ]; then
      falta="o arquivo .p8 (~/.shvia/AuthKey_${APPLE_API_KEY:-<KEYID>}.p8, ou APPLE_API_KEY_PATH)"
    elif [ ! -f "$APPLE_API_KEY_PATH" ]; then
      falta="o arquivo $APPLE_API_KEY_PATH"
    fi
    [ -n "${APPLE_API_KEY:-}" ] || falta="${falta:+$falta, }o Key ID (APPLE_API_KEY)"
    if [ -n "$nome_do_arq" ] && [ -n "${APPLE_API_KEY:-}" ] && [ "$nome_do_arq" != "$APPLE_API_KEY" ]; then
      falta="${falta:+$falta, }um Key ID que bata com o arquivo ($APPLE_API_KEY ≠ $nome_do_arq)"
    fi
    [ -n "${APPLE_API_ISSUER:-}" ] || falta="${falta:+$falta, }o Issuer ID (APPLE_API_ISSUER ou $issuer_arq)"
    if [ -z "$falta" ]; then
      case "$(ls -l "$APPLE_API_KEY_PATH" 2>/dev/null | cut -c5-10)" in
        ------) ;;
        *) echo "    ⚠️  $APPLE_API_KEY_PATH pode ser lido por outros usuários — chmod 600 nele." ;;
      esac
      export APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH
      unset APPLE_ID APPLE_PASSWORD
      NOTARIZE_ENABLED=1
      NOTARIZE_MODE=api
      echo "    ✔ notarização: chave de API $APPLE_API_KEY (App Store Connect) — nada secreto na linha de comando"
      echo "      (a notarização sobe o app p/ a Apple e ESPERA — pode levar alguns minutos)"
      return 0
    fi
    echo "    ⚠️  chave de API da App Store Connect incompleta — falta $falta."
    echo "        Sigo pela senha de app do keychain, se houver. Roteiro: docs/build.md, \"macOS\"."
    unset APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH
  fi

  # Fallback: the app-specific password (the path before 1.6.39).
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
    NOTARIZE_MODE=senha
    echo "    ✔ notarização: $APPLE_ID (team $APPLE_TEAM_ID) — tauri build vai notarizar+staple"
    echo "    ⚠️  pela SENHA DE APP: ela vai na linha de comando do notarytool (visível no \`ps\`"
    echo "        enquanto a Apple responde). Troque pela chave de API: docs/build.md, \"macOS\"."
  else
    echo "    ⚠️  vou ASSINAR mas NÃO notarizar (falta credencial). Crie a chave de API da App"
    echo "        Store Connect: docs/build.md, \"macOS\". (Sem notarizar, o app pede liberação"
    echo "        em Ajustes → Privacidade, e o --publish recusa o build.)"
  fi
  return 0
}

# Notarizes one file (the .dmg) with the credential escolhe_credencial_de_notarizacao picked.
notariza_arquivo() {
  if [ "$NOTARIZE_MODE" = api ]; then
    xcrun notarytool submit "$1" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" \
      --issuer "$APPLE_API_ISSUER" --wait
  else
    xcrun notarytool submit "$1" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" \
      --team-id "$APPLE_TEAM_ID" --wait
  fi
}

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

  # 2) Notarization credential: the API key first, the keychain password as the fallback.
  escolhe_credencial_de_notarizacao || exit 1
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
      if notariza_arquivo "$dmg" 2>&1 | sed 's/^/        /' \
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
# Veredito da prova de chave, isolado para poder ser MEDIDO.
#
# 🔴 Até 21/09/2026 a decisão morava embutida no `verify_updater_key`, e falhava aberto: os dois
# leitores de keyid (`updater_pubkey_id`, `updater_sig_id`) devolvem string VAZIA em qualquer
# erro — `catch { write("") }` mais `|| true` —, e a comparação só dispara com os DOIS
# não-vazios. Id ilegível pulava a comparação e a linha de sucesso imprimia o ✔ do mesmo jeito,
# com `(?)` no lugar do id. Um par de parênteses separava "provado" de "não consegui medir", no
# fim de uma linha verde, no guarda que protege o único ato que este produto não desfaz.
#
# ⚠️ O `|| true` dos leitores FICA. O comentário deles está certo: um preflight não pode ser mais
# frágil que aquilo que ele protege. O que faltava era o terceiro desfecho que a casa já usa em
# outros lugares — **0 passou, 1 falhou, 2 não consegui medir**.
#
# $1 = keyid da pubkey · $2 = keyid da assinatura · $3 = 1 se este build vai PUBLICAR
veredito_da_chave() {
  local pubid="${1:-}" sigid="${2:-}" publicando="${3:-0}"

  if [ -z "$pubid" ] || [ -z "$sigid" ]; then
    # Publicar sem ter conferido é o caso em que "não medi" custa caro: o release sai e nenhum
    # cliente instalado o aceita. Num build local, avisar basta.
    [ "$publicando" = "1" ] && echo "nao-medi-e-vai-publicar" || echo "nao-medi"
    return 0
  fi
  [ "$pubid" != "$sigid" ] && echo "errada" || echo "ok"
}

verify_updater_key() {
  local cli tmpd sigfile sigid pubid
  cli="./node_modules/.bin/tauri"

  # O npm ci só roda depois deste passo: em árvore recém-clonada não há CLI para
  # a prova. Avisar e seguir é melhor que baixar a CLI aqui — a alternativa seria
  # rede no meio de um preflight que se vende como instantâneo.
  if [ ! -x "$cli" ]; then
    # 🔴 Until 1.6.21 "adiada" meant "never": this ran before `npm ci`, returned 0, and nothing
    # ran it again — so on a fresh clone the proof, and 1.6.3's abort of an unmeasured
    # publish, were skipped entirely. Now the build calls this again right after `npm ci`
    # ("depois-do-npm-ci"); a CLI still missing then is "could not measure", which aborts a
    # publish like any other unmeasured proof.
    if [ "${1:-}" = "depois-do-npm-ci" ]; then
      if [ "${PUBLISH:-0}" -eq 1 ]; then
        echo "" >&2
        echo "  ✗ NÃO CONSEGUI PROVAR o par da chave (sem a CLI do Tauri mesmo depois do npm ci)," >&2
        echo "    e este build vai PUBLICAR. Rode \`npm ci\` e tente de novo." >&2
        echo "" >&2
        return 1
      fi
      echo "    ⚠️ prova da chave não rodou: sem a CLI do Tauri mesmo depois do npm ci."
      return 0
    fi
    echo "    ⚠️ chave presente; prova de assinatura adiada até o npm ci (node_modules ainda não existe)."
    PROVA_DA_CHAVE_ADIADA=1
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

  case "$(veredito_da_chave "$pubid" "$sigid" "$PUBLISH")" in
    ok) : ;;
    nao-medi)
      echo "    ⚠️ NÃO CONSEGUI CONFERIR o par da chave (keyid ilegível: pub='${pubid:-?}' sig='${sigid:-?}')." >&2
      echo "       A chave ABRIU com a senha — o que não deu para provar é que ela é a do par" >&2
      echo "       publicado. Build local segue; com --publish isto aborta." >&2
      return 0
      ;;
    nao-medi-e-vai-publicar)
      echo "" >&2
      echo "  ✗ NÃO CONSEGUI CONFERIR o par da chave, e este build vai PUBLICAR." >&2
      echo "      keyid lido da pubkey    : ${pubid:-<ilegível>}" >&2
      echo "      keyid lido da assinatura: ${sigid:-<ilegível>}" >&2
      echo "" >&2
      echo "    Publicar sem esta prova é o caso que ela existe para impedir: se a chave for" >&2
      echo "    de outro par, NENHUM cliente instalado aceita o release, e o updater não" >&2
      echo "    conserta a si mesmo depois. Rode sem --publish para ver o erro do leitor." >&2
      echo "" >&2
      return 1
      ;;
  esac

  if [ "$pubid" != "$sigid" ]; then
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

  # O ✔ só chega aqui pelo ramo `ok` do veredito — ou seja, com os DOIS keyids lidos e iguais.
  # Nada de `${sigid:-?}`: um `(?)` numa linha de sucesso era exatamente o disfarce.
  echo "    ✔ chave do updater: abre com a senha e confere com a pubkey ($sigid)"
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
  #
  # 🔴 Until 1.6.23 the list missed inputs the bundle carries: `claude-runner/` (shipped whole
  # as `bundle.resources`), `src-tauri/icons`, `Cargo.lock`, `build.rs`, `public/` and
  # `package-lock.json`. Two commits may share a version (the versioning rule allows it), and a
  # second one that only fixed the runner or bumped a locked dependency was "reused" — the old
  # bundle shipped under the new commit. scripts/prova-reuso-ve-o-que-empacota.mjs reads
  # tauri.conf.json and fails when something it bundles is missing here. `node_modules` is
  # pruned: it is not a source, and scanning it made the check slow.
  local novas
  novas="$(find src src-tauri/src src-tauri/capabilities src-tauri/binaries \
                src-tauri/icons claude-runner public \
                index.html package.json package-lock.json vite.config.ts tsconfig.json \
                src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/build.rs src-tauri/tauri.conf.json \
             \( -name node_modules -prune \) -o \( -type f -newer "$ref" -print -quit \) 2>/dev/null || true)"
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
PUBLISH_DEST="${SHVIA_PUBLISH_DEST:-b3sys@100.64.100.242:/srv/shvia/storage/app/public/desktop/}"
PUBLIC_BASE="${SHVIA_PUBLIC_BASE:-https://ai.shvia.org}"

# Baixa o release.json JÁ PUBLICADO para a raiz, para o release-manifest.mjs
# mesclar a plataforma desta máquina em cima dele em vez de recomeçar.
#
# Sem este passo, o fluxo "cada SO numa máquina" perde entradas: o macOS publica
# um manifesto só-macOS, e a entrada do Windows que estava no servidor desaparece.
# O sintoma é o pior possível — nada falha no build, nada falha no endpoint, e os
# usuários de Windows simplesmente param de receber update.
#
# 🔴 Until 1.6.20 this was fail-open: no network, a 5xx, a timeout or unreadable JSON all
# read as "nothing published", and the scp then replaced the server's manifest with one that
# had only THIS platform — the exact silent loss described above, triggered by a flaky
# network. Now only two answers are trusted: 200 (merge on top of it) and 404 (nothing was
# ever published: start fresh). Anything else aborts the publish. An operator who KNOWS the
# remote manifest must be discarded sets SHVIA_PUBLISH_SEM_MESCLAR=1.
#
# The decision alone, so scripts/prova-manifesto-remoto.mjs can measure it: $1 = curl's exit
# code, $2 = the HTTP status curl reports ("000" when there was no response at all).
veredito_do_manifesto_remoto() {
  local rc="${1:-}" http="${2:-}"
  if [ "$rc" = "0" ] && [ "$http" = "200" ]; then echo "mesclar"; return 0; fi
  if [ "$rc" = "0" ] && [ "$http" = "404" ]; then echo "recomecar"; return 0; fi
  echo "abortar"
}

fetch_remote_manifest() {
  local url="$PUBLIC_BASE/storage/desktop/release.json"
  if [ "${SHVIA_PUBLISH_SEM_MESCLAR:-0}" = "1" ]; then
    echo "    ⚠️ SHVIA_PUBLISH_SEM_MESCLAR=1 — o manifesto publicado será SUBSTITUÍDO, sem mesclar"
    return 0
  fi
  local tmp; tmp="$(mktemp)"
  local http rc=0
  http="$(curl -sS --max-time 20 -o "$tmp" -w '%{http_code}' "$url" 2>/dev/null)" || rc=$?
  case "$(veredito_do_manifesto_remoto "$rc" "$http")" in
    mesclar) ;;
    recomecar)
      echo "    (nenhum manifesto publicado em $url — primeira publicação, nada a mesclar)"
      rm -f "$tmp"; return 0 ;;
    *)
      echo "" >&2
      echo "  ✗ NÃO CONSEGUI LER o manifesto publicado (curl=$rc, HTTP ${http:-?})." >&2
      echo "    Publicar agora substituiria o release.json do servidor por um só com esta" >&2
      echo "    plataforma — as outras param de receber update, sem erro em lugar nenhum." >&2
      echo "    Tente de novo com a rede estável. Para descartar o publicado de propósito:" >&2
      echo "    SHVIA_PUBLISH_SEM_MESCLAR=1 ./build-local.sh --publish" >&2
      rm -f "$tmp"; return 1 ;;
  esac
  if ! node -e 'JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"))' "$tmp" 2>/dev/null; then
    echo "  ✗ o release.json publicado NÃO é JSON válido — publicar por cima apagaria o que" >&2
    echo "    ele descreve. Confira o servidor, ou use SHVIA_PUBLISH_SEM_MESCLAR=1." >&2
    rm -f "$tmp"; return 1
  fi
  # shellcheck disable=SC2016  # `${...}` aqui é template literal de JS, não de bash
  local plats; plats="$(node -e '
    const m = JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"));
    process.stdout.write(`${m.version} [${Object.keys(m.platforms||{}).join(", ")}]`);
  ' "$tmp")"
  mv "$tmp" release.json
  echo "    publicado hoje: $plats — o manifesto novo mescla em cima disto"
}

# The decision alone, measured by scripts/prova-chave-das-assinaturas.mjs (1.6.21).
# $1 = keyid of the pubkey compiled into the clients; $2 = one "file<TAB>keyid" line per
# SIGNED artifact about to ship (an unreadable signature has an empty keyid).
veredito_das_assinaturas() {
  local pubid="${1:-}" linhas="${2:-}" nome id
  [ -z "$linhas" ] && { echo "ok"; return 0; }
  [ -z "$pubid" ] && { echo "nao-medi"; return 0; }
  while IFS=$'\t' read -r nome id; do
    [ -z "$nome" ] && continue
    [ -z "$id" ] && { echo "nao-medi"; return 0; }
    [ "$id" != "$pubid" ] && { echo "errada"; return 0; }
  done <<< "$linhas"
  echo "ok"
}

# 🔴 Every signature about to ship must be from the pair whose public half is compiled into the
# clients (1.6.21). The key proof runs at BUILD time, and the reuse path never runs it: this
# reads the signatures in release.json — the ones the upload actually carries. Same keyid
# comparison as the proof, no new cryptography: a signature from another pair has another id.
confere_chaves_do_manifesto() {
  local pubid linhas
  pubid="$(updater_pubkey_id)"
  # shellcheck disable=SC2016  # `${...}` aqui é template literal de JS, não de bash
  linhas="$(node -e '
    const m = JSON.parse(require("fs").readFileSync("release.json", "utf8"));
    const p = {darwin: "macos", linux: "linux", win32: "windows"}[process.platform];
    const out = [];
    for (const a of (m.platforms?.[p]?.artifacts ?? [])) {
      if (!a.signature) continue;
      let id = "";
      try {
        const txt = Buffer.from(String(a.signature).trim(), "base64").toString();
        id = Buffer.from(txt.trim().split("\n")[1], "base64").slice(2, 10).toString("hex").toUpperCase();
      } catch {}
      out.push(`${a.file}\t${id}`);
    }
    process.stdout.write(out.join("\n"));
  ' 2>/dev/null || true)"
  case "$(veredito_das_assinaturas "$pubid" "$linhas")" in
    ok) return 0 ;;
    errada)
      echo "" >&2
      echo "  ✗ há artefato assinado com OUTRA chave (pubkey publicada: $pubid):" >&2
      printf '%s\n' "$linhas" | sed 's/^/      /' >&2
      echo "    Nenhum cliente instalado aceitaria este release. Nada foi enviado." >&2
      return 1 ;;
    *)
      echo "" >&2
      echo "  ✗ NÃO CONSEGUI LER a chave das assinaturas deste release — nada foi enviado." >&2
      printf '%s\n' "$linhas" | sed 's/^/      /' >&2
      return 1 ;;
  esac
}

# 🔴 The Apple signature of what is about to ship, checked at PUBLISH time (1.6.22).
# Build-time checks do not cover it: a missing identity only printed "BUILD SAI SEM ASSINAR"
# and went on, a failed `codesign --verify` only printed, and the reuse path skips signing
# and its verification entirely. This is the one point every publish passes through.
confere_assinatura_apple() {
  [ "${_BUILD_OS:-}" = macOS ] || return 0
  local app dmg falhou=0
  app="$(find src-tauri/target/release/bundle/macos -maxdepth 1 -name '*.app' 2>/dev/null | head -1 || true)"
  dmg="$(find src-tauri/target/release/bundle/dmg -maxdepth 1 -name '*.dmg' 2>/dev/null | head -1 || true)"
  if [ -z "$app" ] && [ -z "$dmg" ]; then
    echo "  ✗ nenhum .app nem .dmg no bundle para conferir a assinatura — nada foi enviado." >&2
    return 1
  fi
  if [ -n "$app" ] && ! codesign --verify --deep --strict "$app" >/dev/null 2>&1; then
    echo "  ✗ $(basename "$app"): sem assinatura Apple válida (codesign --verify --deep --strict)." >&2
    falhou=1
  fi
  if [ -n "$dmg" ] && ! xcrun stapler validate "$dmg" >/dev/null 2>&1; then
    echo "  ✗ $(basename "$dmg"): sem notarização grampeada (stapler validate)." >&2
    falhou=1
  fi
  if [ "$falhou" -ne 0 ]; then
    echo "    Nada foi enviado. Um build sem assinatura abre como 'danificado' para quem baixa" >&2
    echo "    e troca o app de quem atualiza. Refaça o build com a identidade no keychain." >&2
    return 1
  fi
  echo "    ✔ assinatura Apple conferida no que vai subir"
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

  # ── Banco do repositório pacman ────────────────────────────────────────────
  # Entra FORA da lista do manifesto de propósito: o release.json descreve
  # artefatos versionados (nome, tamanho, sha256, assinatura), e o banco do repo
  # não é um deles — é índice, tem nome fixo e é regravado a cada versão. Colocá-lo
  # no manifesto obrigaria a inventar uma entrada que o endpoint do updater teria
  # de aprender a ignorar.
  # Sem estes quatro arquivos no servidor, `pacman -Syu` não enxerga a versão nova
  # (o .pkg sozinho só serve para `pacman -U` à mão).
  # `if` e não `[ … ] && …`: sob `set -e`, um `&&` que dá falso como ÚLTIMO comando
  # do corpo do laço derruba o script. Aqui o falso é o caso NORMAL — no macOS não
  # existe pasta pacman/ nenhuma —, então isso abortaria a publicação do Mac.
  local _nome_db _achado_db
  for _nome_db in shvia.db shvia.db.tar.gz shvia.files shvia.files.tar.gz; do
    _achado_db="$(find src-tauri/target/release/bundle/pacman -maxdepth 1 -name "$_nome_db" -print -quit 2>/dev/null || true)"
    if [ -n "$_achado_db" ]; then arquivos+=("$_achado_db"); fi
  done

  confere_assinatura_apple || return 1
  confere_chaves_do_manifesto || return 1

  echo "    destino: $PUBLISH_DEST"
  for f in "${arquivos[@]}"; do echo "      $(basename "$f")"; done
  echo "      release.json"
  echo "    (uma senha só — é um scp com todos os arquivos)"

  # UM scp: uma conexão, um prompt de senha.
  scp "${arquivos[@]}" release.json "$PUBLISH_DEST"
  # What reached the server, for anexa_na_release_do_github (f121) to attach afterwards.
  PUBLICADOS=("${arquivos[@]}" release.json)

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

# Attaches what was just published to the GitHub Release of the same version (f121).
#
# Every Release used to have 0 assets: release.yml creates tag and notes, and building,
# signing and publishing happen here, on another machine. The server's release.json is
# overwritten by each publish, so nothing recorded which installers went out for which OS.
# The owner chose to attach them to the Release (23/09/2026).
#
# It NEVER fails the build. The server copy is what users download; the Release copy is the
# record. So every problem is a warning with the command to finish by hand, and the last line
# on stdout is the verdict the ruler reads: anexado | sem-gh | sem-release | falhou.
# `--clobber` because release.json is re-uploaded by each platform's publish, merged.
# Measured by scripts/prova-anexa-na-release.mjs (npm run prova:anexa).
anexa_na_release_do_github() {
  local versao="$1"; shift
  local gh="${GH_BIN:-gh}"
  if ! command -v "$gh" >/dev/null 2>&1; then
    echo "    ⚠️ gh não encontrado — os instaladores NÃO foram anexados à Release $versao." >&2
    echo "       Para completar: gh release upload $versao $* --clobber" >&2
    echo "sem-gh"; return 0
  fi
  if ! "$gh" release view "$versao" >/dev/null 2>&1; then
    echo "    ⚠️ a Release $versao ainda não existe no GitHub (o release.yml a cria quando o" >&2
    echo "       version.md chega ao master). Nada foi anexado. Depois que ela existir:" >&2
    echo "       gh release upload $versao $* --clobber" >&2
    echo "sem-release"; return 0
  fi
  if "$gh" release upload "$versao" "$@" --clobber >/dev/null 2>&1; then
    echo "    ✅ $# arquivo(s) anexado(s) à Release $versao no GitHub" >&2
    echo "anexado"
  else
    echo "    ⚠️ falhou anexar à Release $versao (a publicação no servidor já valeu)." >&2
    echo "       Para completar: gh release upload $versao $* --clobber" >&2
    echo "falhou"
  fi
  return 0
}

PUBLICADOS=()
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

# 🔴 What is published must be what is committed (1.6.34). Nothing tied a publish to the
# repository: uncommitted changes — this checkout is shared by several sessions — shipped as
# "version X" and matched no commit. The owner chose: refuse when something the BUILD reads is
# dirty or untracked; only warn for the rest, and for a HEAD that is not origin/master.
# ENTRADAS_DO_BUILD is what the bundle is made from (see the reuse list and tauri.conf.json),
# plus version.md — the number the publish announces — and the Arch PKGBUILD.
ENTRADAS_DO_BUILD="version.md src src-tauri claude-runner codex-runner public index.html package.json package-lock.json vite.config.ts tsconfig.json scripts build-local.sh packaging"
confere_arvore_para_publicar() {
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "  ⚠️ fora de um repositório git — não dá para conferir que o publicado é o commitado." >&2
    return 0
  fi
  local sujos outros
  # shellcheck disable=SC2086  # the list is meant to split into paths
  sujos="$(git status --porcelain --untracked-files=all -- $ENTRADAS_DO_BUILD 2>/dev/null || true)"
  if [ -n "$sujos" ]; then
    echo "" >&2
    echo "  ✗ --publish com entrada do build suja ou fora do git — o que subiria não é nenhum commit:" >&2
    printf '%s\n' "$sujos" | head -20 | sed 's/^/      /' >&2
    echo "    Commite (ou descarte) e publique de novo. Para um teste local, rode sem --publish." >&2
    echo "" >&2
    return 1
  fi
  outros="$(git status --porcelain 2>/dev/null | wc -l | tr -d ' ')"
  if [ "$outros" -gt 0 ]; then
    echo "    ⚠️ $outros arquivo(s) fora do build sujo(s) na árvore — não entram no bundle."
  fi
  if git rev-parse --verify -q origin/master >/dev/null && [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/master)" ]; then
    echo "    ⚠️ HEAD ($(git rev-parse --short HEAD)) não é o origin/master ($(git rev-parse --short origin/master))."
  fi
  return 0
}
if [ "$PUBLISH" -eq 1 ]; then
  confere_arvore_para_publicar || exit 2
fi

# 🔴 A test build cannot be published (1.6.22). `--no-sign` skips the Apple signature and
# notarization, and until 1.6.22 nothing stopped `--publish` from shipping it — directly, or
# through the reuse path, which checks version, sha256 and freshness but never the signature.
# New Mac users then got a DMG macOS calls "damaged", and existing ones auto-updated to it.
if [ "$PUBLISH" -eq 1 ] && [ "$NO_SIGN" -eq 1 ]; then
  echo "" >&2
  echo "  ✗ --publish com --no-sign: um build de teste, sem assinatura, não é publicável." >&2
  echo "    Rode sem --no-sign (e com a identidade no keychain) para publicar." >&2
  echo "" >&2
  exit 2
fi

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
  if [ "${PROVA_DA_CHAVE_ADIADA:-0}" = "1" ]; then
    step "[1b] prova da chave do updater (estava adiada até o npm ci)"
    PROVA_DA_CHAVE_ADIADA=0
    verify_updater_key depois-do-npm-ci
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
  # ── bundle overrides (1.6.24) ──
  # Tauri REQUIRES every `externalBin` to exist — measured: "resource path
  # `binaries/anna-<triple>` doesn't exist" stops the build script. So "the app ships
  # without the engine" (--no-anna, or no anna found) was never true: --no-anna failed the
  # build, and a missing anna either failed it or bundled a stale one. With no sidecar
  # staged, Tauri is told so (`externalBin: []`); the installed app then finds anna on PATH.
  # Both overrides live under "bundle", so they are merged into ONE --config.
  SEM_ANNA=0
  if ! ls src-tauri/binaries/anna-* >/dev/null 2>&1; then
    SEM_ANNA=1
    echo "    (sem anna em src-tauri/binaries — o bundle sai SEM o motor; o app usa o do PATH)"
  fi
  _BUNDLE_CFG=""
  if [ "$UPDATER_ARTIFACTS" -eq 0 ]; then _BUNDLE_CFG='"createUpdaterArtifacts":false'; fi
  if [ "$SEM_ANNA" -eq 1 ]; then _BUNDLE_CFG="${_BUNDLE_CFG:+$_BUNDLE_CFG,}\"externalBin\":[]"; fi
  _CFG=""
  if [ -n "$_BUNDLE_CFG" ]; then _CFG="{\"bundle\":{$_BUNDLE_CFG}}"; fi
  # ── end bundle overrides ──
  # Quatro braços explícitos em vez de montar array de argumentos: o bash do macOS
  # é 3.2, e ali `"${arr[@]}"` de array VAZIO com `set -u` aborta com "unbound
  # variable" (testado). Verboso, mas roda nos três SOs.
  if [ -n "$BUNDLES" ] && [ -n "$_CFG" ]; then
    npx tauri build --bundles "$BUNDLES" --config "$_CFG"
  elif [ -n "$BUNDLES" ]; then
    npx tauri build --bundles "$BUNDLES"
  elif [ -n "$_CFG" ]; then
    npx tauri build --config "$_CFG"
  else
    npx tauri build
  fi


  if [ "$_BUILD_OS" = macOS ]; then
    step "[macOS] verificação (codesign / spctl / stapler)"
    verify_macos_signature
  fi
fi

# ── Pacote Arch Linux (.pkg.tar.zst) ─────────────────────────────────────────
# O bundler do Tauri (2.x) só conhece deb/rpm/appimage no Linux, então o pacote
# pacman sai daqui. DOIS caminhos, escolhidos pela distro que roda o build:
#
#   Arch   → `makepkg` com o packaging/arch/PKGBUILD. Pacote nativo de verdade:
#            .PKGINFO correto, hook de pós-instalação, deps declaradas uma vez só
#            no PKGBUILD. É o caminho bom.
#   Debian → `fpm -s dir` sobre o payload extraído do .deb, porque a máquina de build
#            do Linux pode não ser um Arch. As deps e o marcador aqui são ESCRITOS À
#            MÃO e precisam casar com o PKGBUILD — dois lugares, mesma lista.
#
# ⚠️ As duas rotas têm de produzir pacote EQUIVALENTE: mesmo pkgname, mesmas deps e o
# marcador dentro. Até a 1.1.17 não produziam — a rota fpm saía sem o marcador e com
# outro nome — e o app instalado por ela tentava o auto-update que falha no fim do
# download. É o que o ADR-029 conta, e é a razão de tanto comentário aqui.
#
# Idempotente nos dois: se o .pkg desta versão já está no disco (reuso), pula.
#
# ⚠️ O pacote pacman NÃO se auto-atualiza (ADR-028). O `tauri-plugin-updater` não
# tem instalador de pacman — `bundle_type()` só devolve Deb/Rpm/AppImage/Msi/Nsis
# — e como este pacote sai do payload do .deb, o binário vem marcado como DEB. Sem
# o guard do src-tauri/src/updater.rs o app baixaria ~80 MB e chamaria `dpkg -i`,
# que não existe no Arch. Quem atualiza é o `pacman -Syu`, contra o repo publicado
# logo abaixo.
if [ "$_BUILD_OS" = Linux ]; then
  _versao_pkg="$(tr -d ' \t\n\r' < version.md)"
  _bundle_pkg=src-tauri/target/release/bundle
  _deb_pkg="$(find "$_bundle_pkg/deb" -maxdepth 1 -name "*_${_versao_pkg}_*.deb" 2>/dev/null | head -1)"

  # Nome do pacote pacman — o MESMO nas duas rotas (o makepkg lê do PKGBUILD, o fpm
  # recebe por `-n`). Até a 1.1.17 a rota fpm publicava `shv-ia`, o nome do pacote
  # Debian de onde ela convertia: dois nomes para o mesmo app, e um `pacman -Syu`
  # cego para a relação entre eles (ADR-029).
  _PKG_ARCH_NOME=shvia-desktop

  # O `shv-ia-*.pkg.tar.*` de build anterior sai do bundle dir. Ele não se desqualifica
  # por ser velho: tem a versão CORRENTE no nome, então o release-manifest o aceitaria e
  # o `find … | head -1` do repo-add poderia escolher ele — publicando o pacote de nome
  # errado e sem o marcador, que é exatamente a falha que a 1.1.18 fecha.
  rm -f "$_bundle_pkg/pacman"/shv-ia-*.pkg.tar.*

  if [ -n "$_deb_pkg" ] && \
     ! find "$_bundle_pkg/pacman" -name "${_PKG_ARCH_NOME}-${_versao_pkg}*.pkg.tar.*" 2>/dev/null | grep -q .; then
    mkdir -p "$_bundle_pkg/pacman"
    _deb_abs="$(cd "$(dirname "$_deb_pkg")" && pwd)/$(basename "$_deb_pkg")"
    _pkgdest_abs="$(cd "$_bundle_pkg/pacman" && pwd)"

    if [ "$_LINUX_FAMILIA" = arch ] && command -v makepkg >/dev/null 2>&1; then
      echo "==> Arch: empacotando com makepkg (packaging/arch/PKGBUILD)..."
      # PKGDEST põe o .pkg.tar.zst junto dos outros bundles em vez de dentro de
      # packaging/arch/ — o resto do pipeline (manifesto, publish, listagem final)
      # varre bundle/, e um pacote fora dali seria invisível para os três.
      # BUILDDIR sai do repo: makepkg escreve src/ e pkg/ ao lado do PKGBUILD, e
      # isso sujaria a árvore versionada a cada build.
      # SHVIA_DEB é o contrato com o PKGBUILD (ver o cabeçalho dele).
      # --nodeps: as deps do PKGBUILD são de RUNTIME do usuário final; exigi-las
      # instaladas aqui só para reempacotar um .deb pronto não prova nada.
      _mk_tmp="$(mktemp -d)"
      if (cd packaging/arch && \
          SHVIA_DEB="$_deb_abs" \
          PKGDEST="$_pkgdest_abs" \
          BUILDDIR="$_mk_tmp" \
          makepkg --force --clean --nodeps >/dev/null); then
        echo "    ✓ $(find "$_bundle_pkg/pacman" -name '*.pkg.tar.*' 2>/dev/null | head -1)"
      else
        echo "AVISO: makepkg falhou — o build segue sem o pacote Arch." >&2
        echo "       rode à mão para ver o erro: cd packaging/arch && makepkg -f" >&2
      fi
      rm -rf "$_mk_tmp"

    elif command -v fpm >/dev/null 2>&1; then
      echo "==> Linux ($_LINUX_FAMILIA): empacotando o .pkg.tar.zst com fpm..."
      # Até a 1.1.17 esta rota era `fpm -s deb -t pacman <deb>`, conversão direta — e
      # ela produzia um pacote que INSTALA mas não ATUALIZA: o payload do .deb não tem
      # o marcador `/usr/share/shvia-desktop/instalado-por`, então o app não sabia que
      # tinha vindo do pacman, pedia `?bundle=deb`, baixava o .deb e morria no `dpkg`
      # que não existe no Arch (ADR-029 — aconteceu de verdade com o pacote da 1.1.17).
      #
      # Por isso agora o payload é EXTRAÍDO, recebe o marcador e só então é empacotado.
      # O preço de `-s dir` é declarar à mão o que o .deb já dizia (nome, versão,
      # licença, deps) — e as deps são as MESMAS do PKGBUILD, dois lugares e uma lista.
      _stage="$(mktemp -d)"
      # `dpkg-deb` primeiro: numa máquina Debian — a única onde esta rota roda — ele é
      # garantido. `bsdtar` (libarchive-tools) é a alternativa, e lê os dois níveis do
      # .deb (o `ar` de fora e o `data.tar.*` de dentro) numa passada.
      if command -v dpkg-deb >/dev/null 2>&1; then
        dpkg-deb -x "$_deb_abs" "$_stage"
      elif command -v bsdtar >/dev/null 2>&1; then
        bsdtar -xOf "$_deb_abs" 'data.tar.*' | bsdtar -xf - -C "$_stage"
      else
        echo "AVISO: sem dpkg-deb nem bsdtar para abrir o .deb — sem pacote Arch." >&2
        echo "       instale: sudo apt install dpkg-dev libarchive-tools" >&2
        rm -rf "$_stage"
        _stage=""
      fi

      if [ -n "$_stage" ]; then
        # O contrato com src-tauri/src/updater.rs (ADR-028). O PKGBUILD escreve o mesmo
        # arquivo com o mesmo conteúdo: as duas rotas têm de entregar pacote
        # equivalente, senão o comportamento do app passa a depender da distro da
        # máquina que fez o build — que é o bug da 1.1.17.
        install -Dm644 /dev/stdin "$_stage/usr/share/shvia-desktop/instalado-por" <<<'pacman'

        # -a explícito: amd64 no mundo deb, x86_64 no pacman — o fpm não traduz sozinho.
        # --no-auto-depends: com `-s dir` não há de onde deduzir, e os nomes Debian
        # (libwebkit2gtk-4.1-0, libgtk-3-0) não existem no pacman de todo modo.
        # --conflicts/--replaces: é o que migra quem instalou o `shv-ia` de antes —
        # sem eles, `pacman -Syu` não vê relação entre os dois nomes e a máquina fica
        # parada na 1.1.17 para sempre.
        if (cd "$_stage" && fpm -s dir -t pacman \
              -n "$_PKG_ARCH_NOME" -v "$_versao_pkg" --iteration 1 \
              -a "$(uname -m)" --no-auto-depends \
              -d webkit2gtk-4.1 -d gtk3 -d libayatana-appindicator -d hicolor-icon-theme \
              --conflicts shv-ia --replaces shv-ia \
              --license 'custom:proprietary' --url 'https://ai.shvia.org' \
              --maintainer 'Samir Hanna Verza <samirhv@me.com>' \
              --description 'ShvIA Desktop — cliente da plataforma de IA da Blue3 (ai.shvia.org)' \
              --pacman-compression zstd -p "$_pkgdest_abs" usr >/dev/null); then
          echo "    ✓ $(find "$_bundle_pkg/pacman" -name '*.pkg.tar.*' 2>/dev/null | head -1)"
          echo "    com o marcador instalado-por: o app manda rodar 'pacman -Syu'."
        else
          echo "AVISO: fpm falhou ao gerar o pacote Arch — o build segue sem ele." >&2
        fi
        rm -rf "$_stage"
      fi

    else
      echo "AVISO: sem makepkg (Arch) nem fpm (Debian) — pulando o .pkg.tar.zst." >&2
      if [ "$_LINUX_FAMILIA" = arch ]; then
        echo "       instale: sudo pacman -S --needed base-devel" >&2
      else
        echo "       instale: sudo apt install ruby ruby-dev build-essential zstd libarchive-tools && sudo gem install fpm" >&2
      fi
    fi
  fi

  # ── Banco do repositório pacman ────────────────────────────────────────────
  # É o que faz `pacman -Syu` funcionar: sem o .db o usuário teria de baixar o
  # arquivo e rodar `pacman -U` à mão a cada versão.
  #
  # O banco é REGERADO DO ZERO (apagando os arquivos antes), listando só a versão
  # corrente. Duas razões: esta máquina só tem o pacote que ela acabou de gerar, e
  # o repo existe para servir a última versão.
  #
  # ⚠️ Regenerar é apagar mesmo — NÃO existe flag do repo-add para isso, e as duas
  # que parecem servir fazem outra coisa:
  #   --new    só adiciona pacote AINDA NÃO presente no banco, e explicitamente
  #            NÃO atualiza a entrada de um que já existe. Como bundle/pacman/ não
  #            é limpo entre builds, o banco sobreviveria e a versão nova jamais
  #            entraria nele — `pacman -Syu` continuaria oferecendo a antiga.
  #   --remove APAGA DO DISCO o arquivo do pacote antigo ao atualizar a entrada.
  #
  # `repo-add` vem do pacote `pacman`: nativo no Arch, e no Debian está em
  # `pacman-package-manager`. Sem ele o .pkg ainda é publicado e instalável por
  # `pacman -U`; só não há `-Syu`.
  _pkg_arch_file="$(find "$_bundle_pkg/pacman" -name "${_PKG_ARCH_NOME}-${_versao_pkg}*.pkg.tar.*" ! -name '*.sig' ! -name '*.sha256' 2>/dev/null | head -1)"
  if [ -n "$_pkg_arch_file" ]; then
    if command -v repo-add >/dev/null 2>&1; then
      echo "==> Linux: gerando o banco do repositório pacman (shvia.db)..."
      rm -f "$_bundle_pkg/pacman"/shvia.db* "$_bundle_pkg/pacman"/shvia.files*
      if (cd "$(dirname "$_pkg_arch_file")" && \
          repo-add shvia.db.tar.gz "$(basename "$_pkg_arch_file")" >/dev/null 2>&1); then
        # repo-add cria shvia.db/shvia.files como LINKS para os .tar.gz. O pacman
        # busca justamente `shvia.db`, então o link tem de virar arquivo de verdade
        # antes de subir — um symlink pendurado no servidor devolve 404.
        # `if` e não `[ -L … ] && …`: sob `set -e`, o teste falso no fim do corpo do
        # laço abortaria o subshell e o segundo arquivo nunca seria convertido.
        (cd "$(dirname "$_pkg_arch_file")" && \
         for _l in shvia.db shvia.files; do
           if [ -L "$_l" ]; then
             cp --remove-destination "$(readlink -f "$_l")" "$_l"
           fi
         done) || true
        echo "    ✓ shvia.db + shvia.files ao lado do pacote"
      else
        echo "AVISO: repo-add falhou — o pacote sai sem repositório (só pacman -U)." >&2
      fi
    else
      echo "AVISO: sem repo-add — o .pkg é publicado, mas sem repo para 'pacman -Syu'." >&2
      if [ "$_LINUX_FAMILIA" != arch ]; then
        echo "       instale: sudo apt install pacman-package-manager" >&2
      fi
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
  step "[f121] anexa os instaladores à Release do GitHub"
  anexa_na_release_do_github "$(tr -d '[:space:]' < version.md)" "${PUBLICADOS[@]}" >/dev/null
fi

_summary
