#requires -Version 5.1
<#
  build-local.ps1 — Build LOCAL do ShvIA Desktop no Windows.
  Gera os instaladores (.msi + -setup.exe). O ShvIA é shell fino: SEM sidecar.

  O BUILD DE RELEASE É LOCAL POR DECISÃO: a matriz de CI (tauri-action) foi removida na
  0.4.6 por custo e não foi restaurada — mesmo com a conta em GitHub Enterprise (50.000
  min/mês), runners macOS custam 10x. Testes/lint rodam via ci.yml; empacotamento não.
  Este script É o pipeline do Windows — inclusive checksums, manifesto
  (release.json) e assinatura (item D9).

  PRE-REQUISITOS (instalar uma vez):
    - Node 20      (winget install OpenJS.NodeJS.LTS)
    - Rust + MSVC  (winget install Rustlang.Rustup ; rustup default stable
                    + "Desktop development with C++" do VS Build Tools)
    - WebView2     (ja vem no Windows 11)

  USO (PowerShell, na raiz do repo):
    .\build-local.ps1                # build normal (.msi + -setup.exe)
    .\build-local.ps1 -SkipNpmCi     # pula 'npm ci' (deps ja instaladas)
    .\build-local.ps1 -SkipGitPull   # NAO sincroniza com o remoto antes do build
    .\build-local.ps1 -NoSign        # NÃO assina — build de teste
    .\build-local.ps1 -Anna C:\bin\anna.exe   # empacota ESTE anna (item D5)
    .\build-local.ps1 -NoAnna        # NÃO empacota o motor

  MOTOR EMPACOTADO (item D5): o `anna` é o gargalo de adoção do Modo Code — hoje é
  pré-requisito externo, e quem instala o app não tem a feature até resolver à mão.
  Por padrão o build empacota o `anna` do PATH como sidecar (externalBin) e SEMPRE
  imprime qual versão está indo. Sem `anna` no PATH, o build segue e avisa.

  ASSINATURA (item D9): sem assinar, o SmartScreen mostra "Editor desconhecido" e
  esconde o botão de instalar atrás de "Mais informações" — a maioria das pessoas
  desiste ali. O certificado NUNCA vem do repo; vem de:
    SHVIA_WIN_CERT_THUMBPRINT -> impressão digital de um cert já no repositório de
                                 certificados do Windows. É o caminho preferido:
                                 a chave privada não vira arquivo em disco.
    SHVIA_WIN_PFX + SHVIA_WIN_PFX_PASSWORD -> caminho de um .pfx e a senha.
  Sem nenhuma das duas, o build segue SEM assinar e avisa (mesma postura do
  macOS). A senha vai em variável de ambiente da SESSÃO, nunca em arquivo.

  GIT PULL (padrao da casa): antes de tudo, o script sincroniza com o remoto via
  scripts/git-sync.mjs. Ele restaura ao HEAD SO os manifests cuja unica diferenca
  e a linha de versao (Cargo.toml e cia. — lixo regeneravel pelo version:sync) e
  preserva qualquer mudanca real, depois faz 'git pull --ff-only'. Nunca cria
  merge e nunca trava o build. Pule com -SkipGitPull.

  Pra sincronizar NA MAO (fora do build), use 'npm run pull' em vez de
  'git pull' — mesma protecao, fim do conflito no Cargo.toml.

  Saida: src-tauri\target\release\bundle\
  Obs.: o 1o build compila o Rust inteiro (~minutos); os proximos sao incrementais.
#>
param(
  [switch]$SkipNpmCi,
  [switch]$SkipGitPull,
  [switch]$NoSign,
  [string]$Anna,
  [switch]$NoAnna
)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

# -- Cronometro do build: tempo total + por etapa ----------------------------
# Mesmo padrao do SHVTERM/build-local.ps1: o "built in Xs" do vite/cargo e so
# UMA etapa interna. Aqui medimos o script INTEIRO (npm -> versao -> bundle),
# por etapa e no total, inclusive quando aborta por erro (trap). Serve p/
# comparar com Linux/macOS (build-local.sh) e achar em qual fase otimizar.
$script:buildStart = Get-Date
$script:phases  = New-Object System.Collections.ArrayList
$script:phCur   = $null
$script:phStart = $script:buildStart
function Format-Elapsed([TimeSpan]$d) {  # "1h 02m 03s" / "4m 05s" / "37s"
  if     ($d.TotalHours   -ge 1) { '{0}h {1:00}m {2:00}s' -f [int]$d.TotalHours,   $d.Minutes, $d.Seconds }
  elseif ($d.TotalMinutes -ge 1) { '{0}m {1:00}s'         -f [int]$d.TotalMinutes, $d.Seconds }
  else                           { '{0:0}s'               -f $d.TotalSeconds }
}
function Step([string]$label) {  # fecha a etapa anterior, abre a nova, mostra o relogio
  $now = Get-Date
  if ($script:phCur) {
    [void]$script:phases.Add([pscustomobject]@{ Name = $script:phCur; Span = $now - $script:phStart })
  } elseif (($now - $script:buildStart).TotalSeconds -ge 1) {
    [void]$script:phases.Add([pscustomobject]@{ Name = 'preparacao'; Span = $now - $script:buildStart })
  }
  $script:phCur = $label; $script:phStart = $now
  Write-Host ("==> [{0}] {1}" -f (Format-Elapsed ($now - $script:buildStart)), $label) -ForegroundColor Cyan
}
function Show-BuildSummary {  # tabela final: cada etapa + TOTAL
  if ($script:phCur) {
    [void]$script:phases.Add([pscustomobject]@{ Name = $script:phCur; Span = (Get-Date) - $script:phStart })
    $script:phCur = $null
  }
  Write-Host "`nTempo por etapa (Windows):" -ForegroundColor Green
  foreach ($p in $script:phases) {
    Write-Host ('{0,9}  {1}' -f (Format-Elapsed $p.Span), $p.Name)
  }
  Write-Host '          --------'
  Write-Host ('{0,9}  TOTAL' -f (Format-Elapsed ((Get-Date) - $script:buildStart)))
}
# trap (toda a faixa do script): se abortar por erro, ainda mostra quanto rodou
trap {
  Write-Host ("`nERRO: build abortou apos {0} (Windows)" -f (Format-Elapsed ((Get-Date) - $script:buildStart))) -ForegroundColor Red
  break   # re-lanca o erro original e encerra
}

# Ferramentas nativas (npm, cargo/tauri) escrevem progresso e avisos no stderr.
# Com "$ErrorActionPreference = Stop", stderr capturado (log/CI/redirecionamento)
# vira erro terminante e mata o build por engano. Rodamos cada nativo com
# EAP=Continue e validamos o que de fato importa: o exit code ($LASTEXITCODE).
function Invoke-Native {
  param([Parameter(Mandatory)][string]$Nome, [Parameter(Mandatory)][scriptblock]$Cmd)
  $prev = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try { & $Cmd } finally { $ErrorActionPreference = $prev }
  if ($LASTEXITCODE -ne 0) { throw "$Nome falhou (exit $LASTEXITCODE)" }
}

# git pull antes do build (padrao da casa): puxa o remoto ANTES de tudo, pra nao
# empacotar codigo velho. fast-forward-only (nunca cria merge). NAO trava o build:
# se nao aplicar (offline, mudancas locais, branch divergente), avisa e segue com
# o codigo LOCAL. git escreve no stderr no fluxo normal, entao rodamos com
# EAP=Continue e validamos o exit code — sem virar erro terminante por engano.
# Delega a sincronia pro scripts/git-sync.mjs (uma implementacao so, igual no
# build-local.sh e no `npm run pull`). Ele restaura ao HEAD APENAS os manifests
# cuja unica diferenca e a linha de versao (lixo regeneravel) e preserva
# qualquer mudanca real (ex.: dep nova no Cargo.toml), depois faz pull --ff-only.
# Nunca derruba o build (o proprio git-sync.mjs sai 0 sempre).
function Invoke-GitSync {
  if ($SkipGitPull) { Write-Host "    (pulado: -SkipGitPull)" -ForegroundColor Yellow; return }
  if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Host "    (node nao encontrado — pulando a sincronia)" -ForegroundColor Yellow; return
  }
  $prev = $ErrorActionPreference; $ErrorActionPreference = "Continue"
  try { node scripts/git-sync.mjs } finally { $ErrorActionPreference = $prev }
}

# ── What the bundler would only say at the END (1.6.50) ──────────────────────
# Two failures build-local.sh stops early and this script did not:
#   * Tauri requires every `bundle.externalBin` file to exist (measured for build-local.sh in
#     1.6.24): with no anna staged, `-NoAnna` or no anna on PATH failed the build at its end.
#   * With `bundle.createUpdaterArtifacts` on, the bundler needs TAURI_SIGNING_PRIVATE_KEY and
#     only complains at the end of the bundle.
# The Windows validation runbook (1.6.44) worked around both with a hand-set TAURI_CONFIG. Now
# the key is checked before `npm ci`, and the externalBin decision is made after the anna step.
# A TAURI_CONFIG the person set is respected as it is.
#
# The override reaches `tauri build` as `--config <file>` (1.7.2). Until then it went only into
# $env:TAURI_CONFIG, and the first real Windows run (26/09/2026) showed what that does: the Rust
# compiled for 13 minutes and the bundler then failed on `binaries\anna-<triple>.exe`. tauri-build
# and tauri-codegen read TAURI_CONFIG from the environment; the CLI builds its bundle config from
# tauri.conf.json, the platform file and `--config` only (tauri-cli 2.11.5, helpers/config.rs).
# A file and not inline JSON: Windows PowerShell 5.1 strips the double quotes inside a native
# argument.
#
# And the path is LITERAL text in the `npx` line, never a variable (1.7.3). Where `npx` resolves
# to npm's npx.ps1 (nvm4w puts it first on PATH), that shim takes the caller's statement as TEXT
# and runs it again with Invoke-Expression, in its own scope, under Set-StrictMode Latest. So
# `$script:TauriConfigFile` meant npx.ps1's script scope and failed as unset (26/09/2026, 1.7.2).
# No `$` in an `npx` line: prova:ps1 checks it.

# The updater key, from the environment or from the files build-local.sh reads (1.1.9):
# $HOME\.shvia\updater.key and updater.pass. The same pair on the three OSes (ADR-022). The
# password is the file's first line, as build-local.sh reads it.
function Resolve-UpdaterKey {
  param([string]$HomeDir = $HOME)
  if ($env:TAURI_SIGNING_PRIVATE_KEY) { return $true }
  $shvia = Join-Path $HomeDir '.shvia'
  $keyFile = Join-Path $shvia 'updater.key'
  if (-not (Test-Path $keyFile)) { return $false }
  $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content -Raw $keyFile).Trim()
  $passFile = Join-Path $shvia 'updater.pass'
  if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -and (Test-Path $passFile)) {
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [string](Get-Content -TotalCount 1 $passFile)
  }
  return $true
}

# The TAURI_CONFIG this build needs, or $null when it needs none. $Triple is rustc's host
# triple; stage-anna.mjs stages the sidecar as src-tauri\binaries\anna-<triple>.exe.
function Get-TauriConfigOverride {
  param([string]$Root, [string]$Triple, [bool]$HasKey, [bool]$NoSign, [bool]$UpdaterArtifacts = $true)
  $bundle = [ordered]@{}
  $sidecar = Join-Path (Join-Path (Join-Path $Root 'src-tauri') 'binaries') "anna-$Triple.exe"
  if (-not (Test-Path $sidecar)) { $bundle['externalBin'] = @() }
  if ($UpdaterArtifacts -and -not $HasKey) {
    if (-not $NoSign) {
      throw "falta a chave do updater (TAURI_SIGNING_PRIVATE_KEY, ou ~\.shvia\updater.key + updater.pass). Sem ela o bundler aborta no FIM do empacotamento. Build de teste, sem artefato de updater: -NoSign."
    }
    $bundle['createUpdaterArtifacts'] = $false
  }
  if ($bundle.Count -eq 0) { return $null }
  return (@{ bundle = $bundle } | ConvertTo-Json -Depth 5 -Compress)
}

# Writes $Json where `tauri build --config` reads it and returns the path. UTF-8 without a BOM:
# 5.1's `Set-Content -Encoding UTF8` writes one, and a BOM is not JSON. The `tauri build` call
# names this same path as literal text, relative to the repo root (see above).
function Write-TauriConfigFile {
  param([string]$Root, [string]$Json)
  $file = Join-Path (Join-Path (Join-Path $Root 'src-tauri') 'target') 'build-local.tauri-config.json'
  New-Item -ItemType Directory -Force (Split-Path $file) | Out-Null
  [System.IO.File]::WriteAllText($file, $Json, (New-Object System.Text.UTF8Encoding $false))
  return $file
}

Write-Host "==> ShvIA Desktop — build local (Windows)"

# The updater key is required BEFORE compiling, as in build-local.sh: a missing key used to
# surface only at the end of the bundle (1.6.50).
$script:ConfDoTauri = Get-Content -Raw (Join-Path (Join-Path $PSScriptRoot 'src-tauri') 'tauri.conf.json') | ConvertFrom-Json
$script:ComUpdater = [bool]$script:ConfDoTauri.bundle.createUpdaterArtifacts
$script:TemChave = Resolve-UpdaterKey
if ($script:ComUpdater -and -not $script:TemChave) {
  if ($NoSign) {
    Write-Host "    ⚠️ sem a chave do updater e com -NoSign: build de TESTE, sem artefato de updater — NÃO publique." -ForegroundColor Yellow
  } elseif (-not $env:TAURI_CONFIG) {
    throw "falta a chave do updater (TAURI_SIGNING_PRIVATE_KEY, ou ~\.shvia\updater.key + updater.pass). Sem ela o bundler aborta no FIM do empacotamento. Build de teste, sem artefato de updater: -NoSign."
  }
}

Step "[git] sincroniza com o remoto (git pull --ff-only)"
Invoke-GitSync

Step "[1/4] dependencias do frontend (npm ci)"
if (-not $SkipNpmCi) {
  Invoke-Native "npm ci" { npm ci }
} else {
  Write-Host "    (pulado: -SkipNpmCi)" -ForegroundColor Yellow
}

Step "[2/4] sincroniza versao (version.md -> manifests)"
Invoke-Native "version:sync" { npm run version:sync }

# Limpa instaladores de builds anteriores (padrao SHVTERM): o bundle dir acumula
# .msi/-setup.exe de versoes antigas. So o artefato do build ATUAL deve sobrar
# na listagem final.
if (Test-Path src-tauri\target\release\bundle) { Remove-Item -Recurse -Force src-tauri\target\release\bundle }

# ── Assinatura (item D9) ─────────────────────────────────────────────────────
# Sem assinar, o SmartScreen mostra "Editor desconhecido" e esconde o botão de
# instalar atrás de "Mais informações" — a maioria das pessoas desiste ali. É o
# equivalente Windows do "app danificado" que o macOS mostra sem Developer ID.
#
# Assina o .msi E o -setup.exe: são dois instaladores distintos, e assinar só um
# deixa metade dos usuários vendo o aviso.
function Find-SignTool {
  # O signtool.exe vive no Windows SDK, em caminho versionado. Pegar o MAIS NOVO
  # evita fixar uma versão de SDK que a máquina pode não ter.
  $cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  $raizes = @("${env:ProgramFiles(x86)}\Windows Kits\10\bin", "$env:ProgramFiles\Windows Kits\10\bin")
  foreach ($raiz in $raizes) {
    if (-not (Test-Path $raiz)) { continue }
    $achado = Get-ChildItem $raiz -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
      Where-Object { $_.FullName -match '\\x64\\' } |
      Sort-Object FullName -Descending | Select-Object -First 1
    if ($achado) { return $achado.FullName }
  }
  return $null
}

function Invoke-Signing {
  if ($NoSign) { Write-Host "    (pulado: -NoSign)" -ForegroundColor Yellow; return }

  $thumb = $env:SHVIA_WIN_CERT_THUMBPRINT
  $pfx   = $env:SHVIA_WIN_PFX
  if (-not $thumb -and -not $pfx) {
    # Aviso EXPLÍCITO e não silêncio: um build sem assinatura que parece normal é
    # o que faz alguém publicar e só descobrir pelo relato do usuário.
    Write-Host "    ⚠️ SEM CERTIFICADO — os instaladores sairão NÃO ASSINADOS." -ForegroundColor Yellow
    Write-Host "       Defina SHVIA_WIN_CERT_THUMBPRINT (cert no repositório do Windows)" -ForegroundColor Yellow
    Write-Host "       ou SHVIA_WIN_PFX + SHVIA_WIN_PFX_PASSWORD. O SmartScreen vai" -ForegroundColor Yellow
    Write-Host "       mostrar 'Editor desconhecido' para quem baixar." -ForegroundColor Yellow
    return
  }

  $signtool = Find-SignTool
  if (-not $signtool) {
    Write-Host "    ⚠️ signtool.exe não encontrado (instale o Windows SDK) — SEM assinar." -ForegroundColor Yellow
    return
  }

  $alvos = Get-ChildItem -Recurse src-tauri\target\release\bundle -Include *.msi, *-setup.exe -ErrorAction SilentlyContinue
  if (-not $alvos) { Write-Host "    (nenhum instalador para assinar)" -ForegroundColor Yellow; return }

  # /fd sha256 e /td sha256: SHA-1 é recusado pelo Windows moderno.
  # /tr (timestamp RFC3161): SEM ele a assinatura EXPIRA junto com o certificado, e
  # um instalador de hoje para de ser confiável no dia em que o cert vencer.
  $comuns = @('/fd', 'sha256', '/td', 'sha256', '/tr', 'http://timestamp.digicert.com')
  $cred = if ($thumb) { @('/sha1', $thumb) } else { @('/f', $pfx, '/p', $env:SHVIA_WIN_PFX_PASSWORD) }

  foreach ($a in $alvos) {
    Write-Host "    assinando $($a.Name)"
    & $signtool sign @cred @comuns $a.FullName
    if ($LASTEXITCODE -ne 0) { throw "signtool falhou em $($a.Name) (exit $LASTEXITCODE)" }
    # Verifica de verdade: assinar e não conferir deixa passar cert expirado ou
    # cadeia incompleta, que o usuário descobre no SmartScreen.
    & $signtool verify /pa /v $a.FullName | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "verificação da assinatura falhou em $($a.Name)" }
    Write-Host "      ✓ assinado e verificado" -ForegroundColor Green
  }
}

# ── Motor empacotado (item D5) ────────────────────────────────────────────────
# ANTES do `tauri build`: o Tauri lê bundle.externalBin na hora de empacotar, e um
# binário que chegue depois simplesmente não entra no bundle.
Step "[D5] motor (anna) para dentro do bundle"
if ($NoAnna) {
  Write-Host "    (pulado: -NoAnna — o app sairá SEM Modo Code pronto)" -ForegroundColor Yellow
  if (Test-Path src-tauri\binaries) { Remove-Item -Recurse -Force src-tauri\binaries }
} elseif ($Anna) {
  Invoke-Native "stage-anna" { node scripts/stage-anna.mjs --from $Anna }
} else {
  Invoke-Native "stage-anna" { node scripts/stage-anna.mjs }
}

$override = $env:TAURI_CONFIG
if ($override) {
  Write-Host "    TAURI_CONFIG definido por você — respeitado como está: $override" -ForegroundColor Yellow
} else {
  $triple = ((& rustc -vV) | Select-String '^host:').ToString().Split(' ')[1]
  $override = Get-TauriConfigOverride -Root $PSScriptRoot -Triple $triple -HasKey $script:TemChave -NoSign ([bool]$NoSign) -UpdaterArtifacts $script:ComUpdater
}
$script:TauriConfigFile = $null
if ($override) {
  $script:TauriConfigFile = Write-TauriConfigFile -Root $PSScriptRoot -Json $override
  Write-Host "    --config $($script:TauriConfigFile) = $override" -ForegroundColor Yellow
}

Step "[3/4] Tauri build"
if ($script:TauriConfigFile) {
  # Literal path, relative to the repo root (Set-Location at the top): see the npx.ps1 note above.
  Invoke-Native "tauri build" { npx tauri build --config src-tauri/target/build-local.tauri-config.json }
} else {
  Invoke-Native "tauri build" { npx tauri build }
}

Step "[4/4] assinatura + checksums + release.json (item D9)"
Invoke-Signing
# O manifesto vem DEPOIS da assinatura: assinar altera os bytes, então um sha256
# calculado antes descreveria um arquivo que não existe mais.
Invoke-Native "release-manifest" { node scripts/release-manifest.mjs }

Write-Host "`n[OK] Instaladores em src-tauri\target\release\bundle\" -ForegroundColor Green
Get-ChildItem -Recurse src-tauri\target\release\bundle -Include *.msi, *-setup.exe -ErrorAction SilentlyContinue |
  ForEach-Object { Write-Host "    $($_.FullName)" }
Show-BuildSummary
