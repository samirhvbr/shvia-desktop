#requires -Version 5.1
<#
  claude-runner/install.ps1 — the Windows installer of the "Claude Code (assinatura)" engine
  (1.6.55). It does on Windows what install.sh does on Linux and macOS:

    - copies the runner to %LOCALAPPDATA%\shvia-claude-runner;
    - runs `npm ci --omit=dev` there, from the versioned lock (finding F-18);
    - leaves %LOCALAPPDATA%\shvia\bin\claude-runner.cmd, for a terminal only;
    - proves the installed runner LOADS before it says it is installed (1.4.22).

  The desktop does not go through the .cmd. It finds claude-runner.mjs in the install folder
  and runs node on it directly (code_bridge/motores.rs), because stopping a .cmd would stop
  cmd.exe and leave node running.

  Until 1.6.55 there was no Windows path at all: install.sh needs bash, and the app looked for
  a claude-runner.exe that nothing builds. The owner answered "Sim, precisa dos dois no Windows"
  on 24/09/2026. What is proved here runs under pwsh in CI (scripts/prova-instaladores-ps1.mjs);
  the Windows machine itself is the owner's runbook (docs/roteiro-validacao-windows.md).

  The file is UTF-8 WITH a BOM on purpose: Windows PowerShell 5.1 reads a file without one as
  the ANSI code page, and every accented message below would come out garbled.
#>
$ErrorActionPreference = 'Stop'
# The app reads this output as UTF-8 (install button): without this, 5.1 writes the OEM page.
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }

# ⚠️ Every LOCAL module the runner imports goes in this list, and on this one line:
# scripts/prova-instalador-copia-os-imports.mjs reads it (the 1.4.7 class — a new import that
# is not copied makes an installation that is born broken). install.sh has the same list.
$Files = @('claude-runner.mjs', 'politica.mjs', 'parada.mjs', 'package.json', 'package-lock.json')

$Dir = $PSScriptRoot
if (-not $env:LOCALAPPDATA) { Write-Output 'erro: LOCALAPPDATA não está definido (isto roda no Windows).'; exit 1 }
$Dest = Join-Path $env:LOCALAPPDATA 'shvia-claude-runner'
$Bin = Join-Path (Join-Path $env:LOCALAPPDATA 'shvia') 'bin'
$OnWindows = $env:OS -eq 'Windows_NT'

# The first candidate that EXISTS, not the first name on PATH: a stale shim or a broken link
# ahead of the real one (measured: a dangling ~/.local/bin/npm) is found first and cannot run.
# Test-Path answers for the LINK itself, broken or not, so a link is judged by its target.
# nvm-windows points C:\Program Files\nodejs at the active version with a link of this kind.
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
$NodeAbs = Find-Exe 'node'
if (-not $NodeAbs) { Write-Output 'erro: Node 18+ não encontrado no PATH.'; exit 1 }

New-Item -ItemType Directory -Force -Path $Dest, $Bin | Out-Null
foreach ($f in $Files) { Copy-Item -Force -LiteralPath (Join-Path $Dir $f) -Destination $Dest }

# npm.cmd, not npm: on Windows `npm` may resolve to npm.ps1, which a Restricted execution
# policy refuses even when this script itself was allowed to run.
$Npm = Find-Exe $(if ($OnWindows) { 'npm.cmd' } else { 'npm' })
if (-not $Npm) { Write-Output 'erro: npm não encontrado no PATH (ele vem com o Node).'; exit 1 }
Push-Location -LiteralPath $Dest
try {
  & $Npm ci --omit=dev --no-audit --no-fund
  if ($LASTEXITCODE -ne 0) { Write-Output "erro: npm ci falhou (código $LASTEXITCODE)."; exit 1 }
} finally { Pop-Location }

# The terminal wrapper. The path goes through %LOCALAPPDATA% instead of being written out:
# cmd.exe reads the file in the OEM code page, and a user folder with an accent would break.
$Wrapper = Join-Path $Bin 'claude-runner.cmd'
$cmd = "@echo off`r`nset `"NODE=$NodeAbs`"`r`nif not exist `"%NODE%`" set `"NODE=node`"`r`n`"%NODE%`" `"%LOCALAPPDATA%\shvia-claude-runner\claude-runner.mjs`" %*`r`n"
[System.IO.File]::WriteAllText($Wrapper, $cmd, [System.Text.Encoding]::ASCII)

# Load proof (1.4.22, 1.6.25). Importing the runner RUNS it, and it waits on stdin; the stdin
# here is closed at once, so it gets EOF and exits, and the probe is bounded anyway.
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
# node builds the file URL itself: [System.Uri] on a Unix path gives a RELATIVE uri whose
# AbsoluteUri is empty (measured under pwsh), and `import('')` then fails for the wrong reason.
$load = "import(require('node:url').pathToFileURL(process.argv[1]).href).catch((e) => { console.error((e && e.stack) || e); process.exit(1); })"
$probe = Invoke-Probe $NodeAbs @('-e', $load, (Join-Path $Dest 'claude-runner.mjs'))
if ($probe.Code -ne 0) {
  Write-Output '🔴 a instalação ficou incompleta — o runner não carrega:'
  ($probe.Text -split "`n" | Select-Object -First 5) | ForEach-Object { Write-Output "   $_" }
  Write-Output '   (falta copiar algum arquivo do runner? veja a lista $Files no install.ps1)'
  exit 1
}

$sdk = '?'
$sdkJson = Join-Path (Join-Path (Join-Path (Join-Path $Dest 'node_modules') '@anthropic-ai') 'claude-agent-sdk') 'package.json'
try { $sdk = (Get-Content -Raw -LiteralPath $sdkJson | ConvertFrom-Json).version } catch { }
Write-Output "✓ claude-runner instalado em $Dest (Agent SDK $sdk)"
if (-not (($env:Path -split [System.IO.Path]::PathSeparator) -contains $Bin)) {
  Write-Output "   para usar no terminal, ponha $Bin no PATH (o app não precisa: ele acha o runner na pasta acima)."
}
if (-not (Get-Command claude -ErrorAction SilentlyContinue)) {
  Write-Output "⚠️  o CLI 'claude' não está no PATH — instale o Claude Code e rode 'claude login'."
}
