#requires -Version 5.1
param(
    [Parameter(Mandatory)][ValidateSet('nsis', 'msi')][string]$InstallerKind,
    [Parameter(Mandatory)][string]$PackageDirectory,
    [Parameter(Mandatory)][string]$EvidenceDirectory
)

$ErrorActionPreference = 'Stop'
if ($env:RUNNER_OS -ne 'Windows' -or $env:RUNNER_ARCH -ne 'X64') {
    throw 'Run this test in a clean Windows x64 GitHub Actions VM.'
}

$baseline = '1.43.3'
$expected = '1.44.1'
$origin = 'https://ai.shvia.org'
$evidence = (New-Item -ItemType Directory -Path $EvidenceDirectory -Force).FullName
Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, OSArchitecture | Format-List

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class ShvIAUpgradeWindow {
    private delegate bool WindowCallback(IntPtr hwnd, IntPtr data);
    [DllImport("user32.dll")]
    private static extern bool EnumWindows(WindowCallback callback, IntPtr data);
    [DllImport("user32.dll")]
    private static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint process);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int size);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect {
        public int Left, Top, Right, Bottom;
    }
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);

    public static IntPtr Find(uint processId, string title) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((hwnd, data) => {
            uint owner;
            GetWindowThreadProcessId(hwnd, out owner);
            if (owner != processId || !IsWindowVisible(hwnd)) return true;
            var caption = new StringBuilder(256);
            GetWindowText(hwnd, caption, caption.Capacity);
            if (caption.ToString() != title) return true;
            Rect rect;
            if (!GetWindowRect(hwnd, out rect) || rect.Right <= rect.Left || rect.Bottom <= rect.Top) return true;
            result = hwnd;
            return false;
        }, IntPtr.Zero);
        return result;
    }
}
'@

function Save-Screenshot([IntPtr]$Handle, [string]$Name) {
    $rect = New-Object ShvIAUpgradeWindow+Rect
    if (-not [ShvIAUpgradeWindow]::GetWindowRect($Handle, [ref]$rect)) { return }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) { return }
    try {
        $image = New-Object System.Drawing.Bitmap($width, $height)
        $graphics = [System.Drawing.Graphics]::FromImage($image)
        try {
            $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $image.Size)
            $image.Save((Join-Path $evidence $Name), [System.Drawing.Imaging.ImageFormat]::Png)
        } finally {
            $graphics.Dispose()
            $image.Dispose()
        }
    } catch {
        Write-Warning "Could not capture window: $($_.Exception.Message)"
    }
}

function Get-WindowText([System.Windows.Automation.AutomationElement]$Window) {
    $elements = $Window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition)
    return (@($elements | ForEach-Object { $_.Current.Name } | Where-Object { $_ }) -join "`n")
}

function Confirm-Upgrade($Process) {
    $promptTitle = 'Atualização disponível'
    $errorTitle = 'Atualização'
    $buttonCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Button
    )
    $deadline = (Get-Date).AddSeconds(100)
    do {
        $Process.Refresh()
        if ($Process.HasExited) { throw "Desktop exited before offering the upgrade: $($Process.ExitCode)." }

        $unexpected = [ShvIAUpgradeWindow]::Find($Process.Id, $errorTitle)
        if ($unexpected -ne [IntPtr]::Zero) {
            $text = Get-WindowText ([System.Windows.Automation.AutomationElement]::FromHandle($unexpected))
            throw "Unexpected updater message before installation: $text"
        }

        $handle = [ShvIAUpgradeWindow]::Find($Process.Id, $promptTitle)
        if ($handle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 500
    } while ((Get-Date) -lt $deadline)
    if ($handle -eq [IntPtr]::Zero) { throw "Desktop never offered to upgrade $baseline to $expected." }

    $prompt = [System.Windows.Automation.AutomationElement]::FromHandle($handle)
    $text = Get-WindowText $prompt
    if ($text -notmatch [regex]::Escape($baseline) -or $text -notmatch [regex]::Escape($expected)) {
        throw "The update confirmation did not show both expected versions: $text"
    }
    $buttons = $prompt.FindAll([System.Windows.Automation.TreeScope]::Descendants, $buttonCondition)
    $install = $buttons | Where-Object { $_.Current.Name.Replace('&', '').Trim() -ieq 'Instalar e reiniciar' } |
        Select-Object -First 1
    Save-Screenshot $handle 'update-offer.png'
    if ($install) {
        $invoke = $install.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $invoke.Invoke()
    } else {
        # rfd 0.17.2 implements this confirmation with a Windows task dialog.
        # TDM_CLICK_BUTTON and the pinned rfd custom OK ID select its install action.
        $tdmClickButton = 0x0466
        $customOk = [IntPtr]1000
        if (-not [ShvIAUpgradeWindow]::PostMessage($handle, $tdmClickButton, $customOk, [IntPtr]::Zero)) {
            throw 'Could not activate the install button in the native update dialog.'
        }
        Write-Output 'Activated the install action using the native task dialog message.'
    }
    Write-Output "Accepted the native update confirmation for $baseline -> $expected."
}

function Find-InstalledProcess([string]$ExecutablePath) {
    Get-Process -Name 'ShvIA', 'shvia-desktop' -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -ieq $ExecutablePath }
}

function Confirm-Relaunch([string]$ExecutablePath, $OriginalProcess) {
    $documentCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Document
    )
    $passwordCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::IsPasswordProperty, $true
    )
    $deadline = (Get-Date).AddMinutes(4)
    $detectedVersion = $baseline
    do {
        $OriginalProcess.Refresh()
        if (-not $OriginalProcess.HasExited) {
            $failure = [ShvIAUpgradeWindow]::Find($OriginalProcess.Id, 'Atualização')
            if ($failure -ne [IntPtr]::Zero) {
                $text = Get-WindowText ([System.Windows.Automation.AutomationElement]::FromHandle($failure))
                Save-Screenshot $failure 'update-failure.png'
                throw "Desktop reported an update failure: $text"
            }
        }
        if (Test-Path -LiteralPath $ExecutablePath) {
            $detectedVersion = (Get-Item -LiteralPath $ExecutablePath).VersionInfo.ProductVersion
        }
        if ($OriginalProcess.HasExited -and $detectedVersion -eq $expected) {
            foreach ($instance in @(Find-InstalledProcess $ExecutablePath)) {
                if ($instance.Id -eq $OriginalProcess.Id) { continue }
                $handle = [ShvIAUpgradeWindow]::Find($instance.Id, 'ShvIA')
                if ($handle -eq [IntPtr]::Zero) { continue }
                try {
                    $element = [System.Windows.Automation.AutomationElement]::FromHandle($handle)
                    $field = $element.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $passwordCondition)
                    $document = $element.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $documentCondition)
                } catch [System.Windows.Automation.ElementNotAvailableException] {
                    continue
                }
                if ($field -and $document -and $document.Current.Name -like '*ShvIA*') {
                    Save-Screenshot $handle 'updated-login.png'
                    Write-Output "Desktop $expected relaunched from the same path (PID $($instance.Id)) and displayed '$($document.Current.Name)' with a password field."
                    return
                }
            }
        }
        Start-Sleep -Seconds 1
    } while ((Get-Date) -lt $deadline)
    throw "No automatic relaunch with a ShvIA login form. Last installed version observed: $detectedVersion."
}

$formatName = if ($InstallerKind -eq 'nsis') { "ShvIA_${baseline}_x64-setup.exe" } else { "ShvIA_${baseline}_x64_en-US.msi" }
$installer = (Get-Item -LiteralPath (Join-Path $PackageDirectory $formatName)).FullName
$baselineHash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash
if (Test-Path -LiteralPath "$installer.sha256") {
    $declared = [regex]::Match((Get-Content -LiteralPath "$installer.sha256" -Raw), '\A[0-9a-fA-F]{64}').Value
    if ($declared -ne $baselineHash) { throw "Baseline installer checksum mismatch: $formatName." }
}

# Check the published endpoint without fetching or invoking the target installer here.
# The installed Desktop must perform the download and signature verification itself.
$check = Invoke-WebRequest -UseBasicParsing -TimeoutSec 30 -Uri "$origin/api/v1/desktop/update/windows-x86_64/${baseline}?bundle=$InstallerKind"
if ($check.StatusCode -ne 200) { throw "Public update endpoint HTTP $($check.StatusCode)." }
$offer = $check.Content | ConvertFrom-Json
$artifact = $offer.platforms.'windows-x86_64'
if ($offer.version -ne $expected -or -not $artifact.url -or -not $artifact.signature) {
    throw 'The public endpoint did not offer the expected signed upgrade.'
}
$url = [uri]$artifact.url
if ($url.GetLeftPart([System.UriPartial]::Authority) -ne $origin) {
    throw "Unexpected public download origin: $($url.GetLeftPart([System.UriPartial]::Authority))."
}
$expectedFile = $formatName.Replace($baseline, $expected)
if ($url.AbsolutePath -ne "/storage/desktop/$expectedFile") {
    throw "The public endpoint offered the wrong installer: $($url.AbsolutePath)."
}
Write-Output "Public update endpoint offers $expectedFile with a signature."

$installDirs = @(
    (Join-Path $env:ProgramFiles 'ShvIA')
    (Join-Path $env:LOCALAPPDATA 'ShvIA')
    (Join-Path $env:LOCALAPPDATA 'Programs/ShvIA')
)
foreach ($dir in $installDirs) {
    if (Test-Path -LiteralPath $dir) { throw "Expected a clean install location: $dir." }
}
if (Get-Process -Name 'ShvIA', 'shvia-desktop' -ErrorAction SilentlyContinue) {
    throw 'A ShvIA process is already running on this VM.'
}

if ($InstallerKind -eq 'nsis') {
    $installation = Start-Process -FilePath $installer -ArgumentList '/S' -Wait -PassThru
} else {
    $log = Join-Path $evidence 'baseline-msi.log'
    $arguments = '/i "{0}" /qn /norestart /L*v "{1}"' -f $installer, $log
    $installation = Start-Process -FilePath msiexec.exe -ArgumentList $arguments -Wait -PassThru
}
if ($installation.ExitCode -ne 0) { throw "Baseline installer exited with code $($installation.ExitCode)." }
$existingDirs = @($installDirs | Where-Object { Test-Path -LiteralPath $_ })
if ($existingDirs.Count -eq 0) { throw 'The baseline installer did not create a ShvIA directory.' }
$exe = Get-ChildItem -LiteralPath $existingDirs -File |
    Where-Object { $_.Name -in @('ShvIA.exe', 'shvia-desktop.exe') } | Select-Object -First 1
if (-not $exe -or $exe.VersionInfo.ProductVersion -ne $baseline) {
    throw "Expected an installed ShvIA binary reporting version $baseline."
}
Write-Output "$formatName installed with exit code 0. Baseline product version: $($exe.VersionInfo.ProductVersion). SHA-256: $baselineHash."

# Prevent unrelated runner setup and use a fresh WebView2 profile for this process tree.
$env:SHVIA_RUNNERS_AUTO = '0'
$env:SHVIA_CODEX_AUTO = '0'
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $env:RUNNER_TEMP 'ShvIA-Upgrade-WebView2'
$app = Start-Process -FilePath $exe.FullName -WorkingDirectory $exe.DirectoryName -PassThru
try {
    Confirm-Upgrade $app
    Confirm-Relaunch $exe.FullName $app
    $engine = & (Join-Path $exe.DirectoryName 'anna.exe') --version
    if ($LASTEXITCODE -ne 0 -or $engine -ne 'anna 0.16.6') {
        throw "Expected bundled Anna 0.16.6 after the update. Found: $engine."
    }
    Write-Output "Verified $InstallerKind update $baseline -> $expected and bundled $engine."
} finally {
    Find-InstalledProcess $exe.FullName | Stop-Process -ErrorAction SilentlyContinue
}
