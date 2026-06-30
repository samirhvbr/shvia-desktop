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

Write-Host "==> ShvIA Desktop — build local (Windows)"

if (-not $SkipNpmCi) {
  Write-Host "==> npm ci"
  npm ci
}

Write-Host "==> sincroniza versao (version.md -> manifests)"
npm run version:sync

Write-Host "==> tauri build"
npx tauri build

Write-Host "==> pronto. Instaladores em src-tauri\target\release\bundle\"
Get-ChildItem -Recurse src-tauri\target\release\bundle -Include *.msi, *-setup.exe -ErrorAction SilentlyContinue |
  ForEach-Object { Write-Host "    $($_.FullName)" }
