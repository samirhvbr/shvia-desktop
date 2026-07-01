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

if (-not $SkipNpmCi) {
  Write-Host "==> npm ci"
  Invoke-Native "npm ci" { npm ci }
}

Write-Host "==> sincroniza versao (version.md -> manifests)"
Invoke-Native "version:sync" { npm run version:sync }

Write-Host "==> tauri build"
Invoke-Native "tauri build" { npx tauri build }

Write-Host "==> pronto. Instaladores em src-tauri\target\release\bundle\"
Get-ChildItem -Recurse src-tauri\target\release\bundle -Include *.msi, *-setup.exe -ErrorAction SilentlyContinue |
  ForEach-Object { Write-Host "    $($_.FullName)" }
