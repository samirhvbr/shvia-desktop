#requires -Version 5.1
<#
  build-local.ps1 — Build LOCAL do ShvIA Desktop no Windows (sem CI).
  Gera os instaladores (.msi + -setup.exe), replicando o runner windows-latest
  do .github/workflows/build.yml. O ShvIA é shell fino: SEM sidecar.

  PRE-REQUISITOS (instalar uma vez):
    - Node 20      (winget install OpenJS.NodeJS.LTS)
    - Rust + MSVC  (winget install Rustlang.Rustup ; rustup default stable
                    + "Desktop development with C++" do VS Build Tools)
    - WebView2     (ja vem no Windows 11)

  USO (PowerShell, na raiz do repo):
    .\build-local.ps1                # build normal (.msi + -setup.exe)
    .\build-local.ps1 -SkipNpmCi     # pula 'npm ci' (deps ja instaladas)

  Saida: src-tauri\target\release\bundle\
  Obs.: o 1o build compila o Rust inteiro (~minutos); os proximos sao incrementais.
#>
param(
  [switch]$SkipNpmCi
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

Write-Host "==> ShvIA Desktop — build local (Windows)"

Step "[1/3] dependencias do frontend (npm ci)"
if (-not $SkipNpmCi) {
  Invoke-Native "npm ci" { npm ci }
} else {
  Write-Host "    (pulado: -SkipNpmCi)" -ForegroundColor Yellow
}

Step "[2/3] sincroniza versao (version.md -> manifests)"
Invoke-Native "version:sync" { npm run version:sync }

# Limpa instaladores de builds anteriores (padrao SHVTERM): o bundle dir acumula
# .msi/-setup.exe de versoes antigas. So o artefato do build ATUAL deve sobrar
# na listagem final.
if (Test-Path src-tauri\target\release\bundle) { Remove-Item -Recurse -Force src-tauri\target\release\bundle }

Step "[3/3] Tauri build"
Invoke-Native "tauri build" { npx tauri build }

Write-Host "`n[OK] Instaladores em src-tauri\target\release\bundle\" -ForegroundColor Green
Get-ChildItem -Recurse src-tauri\target\release\bundle -Include *.msi, *-setup.exe -ErrorAction SilentlyContinue |
  ForEach-Object { Write-Host "    $($_.FullName)" }
Show-BuildSummary
