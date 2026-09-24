#requires -Version 5.1
<#
  codex-runner/install.ps1 — the Windows installer of the Codex engine runner (1.6.55). It does
  on Windows what install.sh does on Linux and macOS:

    - copies the runner to %LOCALAPPDATA%\shvia-codex-runner, and politica.mjs to
      %LOCALAPPDATA%\claude-runner, because the runner imports "../claude-runner/politica.mjs";
    - generates the app-server schema from the installed `codex`, or falls back to the
      repository's copy and says so;
    - leaves %LOCALAPPDATA%\shvia\bin\codex-runner.cmd, for a terminal only;
    - proves the installed runner answers --version before it says it is installed.

  No npm step: like install.sh, this runner has no npm dependency (it drives the `codex` CLI).
  The desktop runs node on codex-runner.mjs directly, not the .cmd (see claude-runner/install.ps1).

  UTF-8 WITH a BOM on purpose, for Windows PowerShell 5.1 (see claude-runner/install.ps1).
#>
$ErrorActionPreference = 'Stop'
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }

# ⚠️ Every LOCAL module the runner imports from its own folder goes in this list, on this one
# line — scripts/prova-instalador-copia-os-imports.mjs reads it, as it reads install.sh's `cp`.
$Files = @('codex-runner.mjs', 'protocolo.mjs', 'esquema.mjs', 'package.json')

$Dir = $PSScriptRoot
if (-not $env:LOCALAPPDATA) { Write-Output 'erro: LOCALAPPDATA não está definido (isto roda no Windows).'; exit 1 }
$Dest = Join-Path $env:LOCALAPPDATA 'shvia-codex-runner'
$Bin = Join-Path (Join-Path $env:LOCALAPPDATA 'shvia') 'bin'
$Politica = Join-Path $env:LOCALAPPDATA 'claude-runner'

function Test-Runnable([string]$Path) {
  $item = Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
  if (-not $item) { return $false }
  if ($item.LinkType -and $item.Target) {
    $target = @($item.Target)[0]
    if (-not [System.IO.Path]::IsPathRooted($target)) { $target = Join-Path (Split-Path -Parent $Path) $target }
    return [bool](Test-Path -LiteralPath $target)
  }
  return $true
}
function Find-Exe([string]$Name) {
  Get-Command $Name -CommandType Application -All -ErrorAction SilentlyContinue |
    Where-Object { Test-Runnable $_.Source } | Select-Object -First 1 -ExpandProperty Source
}
function Invoke-Probe([string]$Exe, [string[]]$Arguments, [int]$TimeoutMs = 60000) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $Exe
  $psi.Arguments = ($Arguments | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join ' '
  $psi.UseShellExecute = $false
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $p = [System.Diagnostics.Process]::Start($psi)
  $p.StandardInput.Close()
  $out = $p.StandardOutput.ReadToEndAsync()
  $err = $p.StandardError.ReadToEndAsync()
  if (-not $p.WaitForExit($TimeoutMs)) {
    try { $p.Kill() } catch { }
    return @{ Code = -1; Text = "não terminou em $([int]($TimeoutMs / 1000)) s" }
  }
  $p.WaitForExit()
  return @{ Code = $p.ExitCode; Text = ($out.Result + $err.Result).Trim() }
}

$NodeAbs = Find-Exe 'node'
if (-not $NodeAbs) { Write-Output 'erro: Node 18+ não encontrado no PATH.'; exit 1 }

New-Item -ItemType Directory -Force -Path $Dest, $Bin, $Politica | Out-Null
foreach ($f in $Files) { Copy-Item -Force -LiteralPath (Join-Path $Dir $f) -Destination $Dest }
Copy-Item -Force -LiteralPath (Join-Path (Join-Path (Split-Path -Parent $Dir) 'claude-runner') 'politica.mjs') -Destination $Politica

# The schema the payload validation reads: from the installed codex when it can generate it,
# otherwise the repository's copy, said out loud (install.sh has the same two branches).
$Schemas = Join-Path $Dest 'schemas'
New-Item -ItemType Directory -Force -Path $Schemas | Out-Null
$SchemaFile = Join-Path $Schemas 'codex_app_server_protocol.v2.schemas.json'
$Codex = Find-Exe 'codex'
$gerado = $false
if ($Codex) {
  try { & $Codex app-server generate-json-schema --out $Schemas *> $null; $gerado = ($LASTEXITCODE -eq 0) -and (Test-Path -LiteralPath $SchemaFile) } catch { $gerado = $false }
}
if ($gerado) {
  $cv = ''
  try { $cv = (& $Codex --version 2>$null | Select-Object -First 1) } catch { }
  Write-Output "  schema: gerado do codex instalado ($cv)"
} else {
  Copy-Item -Force -LiteralPath (Join-Path (Join-Path $Dir 'schemas') 'codex_app_server_protocol.v2.schemas.json') -Destination $Schemas
  Write-Output '  ⚠️  schema: cópia do repositório — este codex não gera o schema, então a'
  Write-Output '      validação de payload pode estar medindo uma versão diferente do protocolo.'
}

$Wrapper = Join-Path $Bin 'codex-runner.cmd'
$cmd = "@echo off`r`nset `"NODE=$NodeAbs`"`r`nif not exist `"%NODE%`" set `"NODE=node`"`r`n`"%NODE%`" `"%LOCALAPPDATA%\shvia-codex-runner\codex-runner.mjs`" %*`r`n"
[System.IO.File]::WriteAllText($Wrapper, $cmd, [System.Text.Encoding]::ASCII)

$probe = Invoke-Probe $NodeAbs @((Join-Path $Dest 'codex-runner.mjs'), '--version')
if ($probe.Code -ne 0) {
  Write-Output '🔴 a instalação ficou incompleta — o runner não responde --version:'
  ($probe.Text -split "`n" | Select-Object -First 5) | ForEach-Object { Write-Output "   $_" }
  Write-Output '   (falta copiar algum arquivo do runner? veja a lista $Files no install.ps1)'
  exit 1
}
Write-Output "✓ codex-runner $($probe.Text) instalado em $Dest"
if (-not (($env:Path -split [System.IO.Path]::PathSeparator) -contains $Bin)) {
  Write-Output "   para usar no terminal, ponha $Bin no PATH (o app não precisa: ele acha o runner na pasta acima)."
}
if (-not $Codex) {
  Write-Output "⚠️  o CLI 'codex' não está no PATH — instale o Codex CLI e rode 'codex login'."
} else {
  & $Codex login status *> $null
  if ($LASTEXITCODE -ne 0) { Write-Output "⚠️  o 'codex' não está autenticado — rode 'codex login'." }
}
