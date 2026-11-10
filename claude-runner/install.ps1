#requires -Version 5.1
<#
  claude-runner/install.ps1 — the Windows installer of the "Claude Code (assinatura)" engine
  (1.6.56). It does on Windows what install.sh does on Linux and macOS:

    - assembles the runner in %LOCALAPPDATA%\shvia-claude-runner.incoming;
    - runs `npm ci --omit=dev` there, from the versioned lock (finding F-18);
    - proves that copy LOADS (1.4.22), and only then puts it in place of
      %LOCALAPPDATA%\shvia-claude-runner (1.38.13);
    - leaves %LOCALAPPDATA%\shvia\bin\claude-runner.cmd, for a terminal only.

  The desktop does not go through the .cmd. It finds claude-runner.mjs in the install folder
  and runs node on it directly (code_bridge/motores.rs), because stopping a .cmd would stop
  cmd.exe and leave node running.

  Until 1.6.56 there was no Windows path at all: install.sh needs bash, and the app looked for
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
$Files = @('claude-runner.mjs', 'politica.mjs', 'parada.mjs', 'orientacao.mjs', 'uso.mjs', 'package.json', 'package-lock.json')

$Dir = $PSScriptRoot
if (-not $env:LOCALAPPDATA) { Write-Output 'erro: LOCALAPPDATA não está definido (isto roda no Windows).'; exit 1 }
$Dest = Join-Path $env:LOCALAPPDATA 'shvia-claude-runner'
# The new install is assembled BESIDE the old one and replaces it only once it loads (1.38.13). Until
# then the files were copied over the installed runner and `npm ci` ran there; `npm ci` deletes
# node_modules first, so an update that lost the network left the new files with no Agent SDK, and the
# app, which compares those files and the version, counted the runner as up to date from the next start
# on. On Windows a FIRST install that failed was stuck the same way, because the app finds the runner
# by its .mjs. install.sh has the long version of this.
$Stage = "$Dest.incoming"
$Old = "$Dest.previous"
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

# npm.cmd, not npm: on Windows `npm` may resolve to npm.ps1, which a Restricted execution
# policy refuses even when this script itself was allowed to run.
$Npm = Find-Exe $(if ($OnWindows) { 'npm.cmd' } else { 'npm' })
if (-not $Npm) { Write-Output 'erro: npm não encontrado no PATH (ele vem com o Node).'; exit 1 }

function Remove-Folder([string]$Path) {
  if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath $Path -Recurse -Force }
}
# Every failure before the swap goes through here: the staged copy goes, the installed runner stays.
function Stop-Install([string[]]$Lines) {
  try { Remove-Folder $Stage } catch { }
  foreach ($l in $Lines) { Write-Output $l }
  exit 1
}

Remove-Folder $Stage
New-Item -ItemType Directory -Force -Path $Stage, $Bin | Out-Null
try {
  foreach ($f in $Files) { Copy-Item -Force -LiteralPath (Join-Path $Dir $f) -Destination $Stage }
} catch { Stop-Install @("erro: não consegui copiar o runner: $_") }

Push-Location -LiteralPath $Stage
try {
  & $Npm ci --omit=dev --no-audit --no-fund
  $NpmCode = $LASTEXITCODE
} finally { Pop-Location }
if ($NpmCode -ne 0) { Stop-Install @("erro: npm ci falhou (código $NpmCode); o runner instalado antes ficou como estava.") }

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
$probe = Invoke-Probe $NodeAbs @('-e', $load, (Join-Path $Stage 'claude-runner.mjs'))
if ($probe.Code -ne 0) {
  Stop-Install (@('🔴 a instalação ficou incompleta — o runner não carrega:') +
    @($probe.Text -split "`n" | Select-Object -First 5 | ForEach-Object { "   $_" }) +
    @('   (falta copiar algum arquivo do runner? veja a lista $Files no install.ps1)'))
}

# The swap: two renames in one folder. If the second one fails, the old install goes back. A file
# of the old install still in use (a session running its engine) makes the first one fail, and then
# nothing changed. What a stopped run leaves behind is cleared by the next one.
try { Remove-Folder $Old } catch { }
$HadOld = Test-Path -LiteralPath $Dest
$SwapError = $null
try {
  if ($HadOld) { Rename-Item -LiteralPath $Dest -NewName (Split-Path -Leaf $Old) }
  Rename-Item -LiteralPath $Stage -NewName (Split-Path -Leaf $Dest)
} catch {
  $SwapError = $_
  if ($HadOld -and -not (Test-Path -LiteralPath $Dest) -and (Test-Path -LiteralPath $Old)) {
    try { Rename-Item -LiteralPath $Old -NewName (Split-Path -Leaf $Dest) } catch { }
  }
}
if ($SwapError) { Stop-Install @("🔴 não consegui pôr a instalação nova no lugar de ${Dest}: $SwapError. A anterior ficou como estava.") }
try { Remove-Folder $Old } catch { }

# The terminal wrapper. The path goes through %LOCALAPPDATA% instead of being written out:
# cmd.exe reads the file in the OEM code page, and a user folder with an accent would break.
$Wrapper = Join-Path $Bin 'claude-runner.cmd'
$cmd = "@echo off`r`nset `"NODE=$NodeAbs`"`r`nif not exist `"%NODE%`" set `"NODE=node`"`r`n`"%NODE%`" `"%LOCALAPPDATA%\shvia-claude-runner\claude-runner.mjs`" %*`r`n"
[System.IO.File]::WriteAllText($Wrapper, $cmd, [System.Text.Encoding]::ASCII)

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
