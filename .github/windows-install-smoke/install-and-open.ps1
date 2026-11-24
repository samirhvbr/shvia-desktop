param([Parameter(Mandatory)][ValidateSet('nsis', 'msi')][string]$InstallerKind)
$ErrorActionPreference = 'Stop'
if ($env:RUNNER_OS -ne 'Windows' -or $env:RUNNER_ARCH -ne 'X64') {
  throw 'This test requires a GitHub-hosted Windows x64 runner.'
}
Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, OSArchitecture | Format-List
$evidence = (New-Item -ItemType Directory -Path smoke-evidence).FullName
$version = '1.44.1'
if ($InstallerKind -eq 'nsis') {
  $package = 'ShvIA_1.44.1_x64-setup.exe'
} else {
  $package = 'ShvIA_1.44.1_x64_en-US.msi'
}
$installer = (Get-Item "test-packages/$package").FullName
$checksum = (Get-Content -Raw "test-packages/$package.sha256").Trim().Split(' ')[0]
if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash -ne $checksum) {
  throw "Checksum mismatch for $package"
}
$nsisDir = Join-Path $env:RUNNER_TEMP 'ShvIA'
$installDirs = @($nsisDir, (Join-Path $env:ProgramFiles 'ShvIA'),
  (Join-Path $env:LOCALAPPDATA 'ShvIA'), (Join-Path $env:LOCALAPPDATA 'Programs/ShvIA'))
foreach ($dir in $installDirs) {
  if (Test-Path -LiteralPath $dir) { throw "Install location is already present: $dir" }
}
if ($InstallerKind -eq 'nsis') {
  $install = Start-Process -FilePath $installer -ArgumentList '/S', "/D=$nsisDir" -Wait -PassThru
} else {
  $log = Join-Path $evidence 'msi-install.log'
  $arguments = '/i "{0}" /qn /norestart /L*v "{1}"' -f $installer, $log
  $install = Start-Process msiexec.exe -ArgumentList $arguments -Wait -PassThru
}
if ($install.ExitCode -ne 0) { throw "$package exited with code $($install.ExitCode)" }
Write-Output "$package installed with exit code 0. SHA-256 $checksum"

$existingDirs = @($installDirs | Where-Object { Test-Path -LiteralPath $_ })
if ($existingDirs.Count -eq 0) { throw 'The installer did not create a ShvIA folder.' }
$exe = Get-ChildItem -LiteralPath $existingDirs -File |
  Where-Object { $_.Name -in @('ShvIA.exe', 'shvia-desktop.exe') } |
  Select-Object -First 1
if (-not $exe) { throw 'The installed Desktop binary was not found.' }
if ($exe.VersionInfo.ProductVersion -ne $version) {
  throw "Unexpected installed Desktop version: $($exe.VersionInfo.ProductVersion)"
}
$engine = & (Join-Path $exe.DirectoryName 'anna.exe') --version
if ($LASTEXITCODE -ne 0 -or $engine -ne 'anna 0.16.6') { throw "Unexpected bundled Anna version: $engine" }

# Enable loopback diagnostics only in the disposable process environment.
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=19383 --remote-debugging-address=127.0.0.1'
$env:SHVIA_RUNNERS_AUTO = '0'
$env:SHVIA_CODEX_AUTO = '0'
$app = Start-Process -FilePath $exe.FullName -WorkingDirectory $exe.DirectoryName -PassThru
try {
  node "$PSScriptRoot/verify-webview.mjs" $evidence
  if ($LASTEXITCODE -ne 0) { throw 'The ShvIA login page did not load in WebView2.' }
  $app.Refresh()
  if ($app.HasExited -or $app.MainWindowHandle -eq [IntPtr]::Zero -or $app.MainWindowTitle -ne 'ShvIA') {
    throw "Expected native ShvIA window. Title: '$($app.MainWindowTitle)'"
  }
  Write-Output "Verified ShvIA $version, $engine, PID $($app.Id), title '$($app.MainWindowTitle)'"
  Write-Output "Installed binary: $($exe.FullName)"
} finally {
  if (-not $app.HasExited) { Stop-Process -Id $app.Id }
}
