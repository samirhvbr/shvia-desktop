$ErrorActionPreference = 'Stop'
$account = $env:QA_EMAIL
$password = $env:QA_PASSWORD
Remove-Item Env:QA_EMAIL, Env:QA_PASSWORD -ErrorAction SilentlyContinue
if (-not $account -or -not $password) { throw 'The temporary test account was not provisioned' }

$exe = Get-Item -LiteralPath $env:DESKTOP_QA_EXE
$engine = Join-Path $exe.DirectoryName 'anna.exe'
$fixture = (New-Item -ItemType Directory (Join-Path $env:RUNNER_TEMP 'ShvIA-Anna-QA')).FullName
$marker = 'WINDOWS-ANNA-' + ([guid]::NewGuid().ToString('N'))
$file = Join-Path $fixture 'probe.txt'
Set-Content -LiteralPath $file -Value $marker -Encoding ascii -NoNewline
$before = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash
$engineProof = Join-Path $env:RUNNER_TEMP 'shvia-anna-process-verified.txt'

$policy = 'HKLM:\Software\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments'
if (-not (Test-Path -LiteralPath $policy)) { New-Item -Path $policy -Force | Out-Null }
$oldPolicy = Get-ItemProperty -LiteralPath $policy -Name $exe.Name -ErrorAction SilentlyContinue
if ($oldPolicy) { throw 'This VM already has a WebView2 policy for the installed app' }
New-ItemProperty -LiteralPath $policy -Name $exe.Name -PropertyType String -Value '--remote-debugging-port=19383 --remote-debugging-address=127.0.0.1' | Out-Null
$env:SHVIA_RUNNERS_AUTO = '0'
$env:SHVIA_CODEX_AUTO = '0'
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $env:RUNNER_TEMP 'ShvIA-WebView2-QA'
try {
  $app = Start-Process -FilePath $exe.FullName -WorkingDirectory $exe.DirectoryName -PassThru
  $observe = Start-Job -ArgumentList $app.Id,$engine,$engineProof -ScriptBlock {
    param($AppId, $ExpectedEngine, $ProofFile)
    $start = Get-Date
    while ((Get-Date) -lt $start.AddMinutes(8)) {
      $child = Get-CimInstance Win32_Process -Filter "ParentProcessId=$AppId AND Name='anna.exe'"
      if ($child | Where-Object { $_.ExecutablePath -ieq $ExpectedEngine }) {
        Set-Content -LiteralPath $ProofFile -Value 'Bundled Anna was a child process of the installed Desktop.'
        break
      }
      Start-Sleep -Milliseconds 350
    }
  }
  $env:QA_EMAIL = $account
  $env:QA_PASSWORD = $password
  $env:QA_FIXTURE = $fixture
  $env:QA_MARKER = $marker
  $env:QA_DESKTOP_PID = $app.Id
  node "$PSScriptRoot/desktop-qa.mjs"
  if ($LASTEXITCODE -ne 0) { throw 'The installed Desktop did not complete the login and local file task' }
  if (-not (Test-Path -LiteralPath $engineProof)) { throw 'Did not observe the bundled Anna process' }
  Get-Content -LiteralPath $engineProof
  if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ne $before) { throw 'The read-only fixture changed' }
  Write-Output 'The synthetic file retained its original SHA-256.'
} finally {
  Remove-Item Env:QA_EMAIL, Env:QA_PASSWORD -ErrorAction SilentlyContinue
  if ($observe) { Remove-Job -Job $observe -Force -ErrorAction SilentlyContinue }
  if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -ErrorAction SilentlyContinue }
  Remove-ItemProperty -LiteralPath $policy -Name $exe.Name -ErrorAction SilentlyContinue
}
