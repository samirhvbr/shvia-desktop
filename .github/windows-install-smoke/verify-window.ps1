param([IntPtr]$Handle, [string]$EvidenceDirectory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;

public static class SmokeWindow {
  [StructLayout(LayoutKind.Sequential)]
  public struct Rect {
    public int Left, Top, Right, Bottom;
  }
  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
}
'@
$element = [Windows.Automation.AutomationElement]::FromHandle($Handle)
$password = New-Object Windows.Automation.PropertyCondition(
  [Windows.Automation.AutomationElement]::IsPasswordProperty, $true
)
$document = New-Object Windows.Automation.PropertyCondition(
  [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Document
)
$deadline = (Get-Date).AddSeconds(35)
do {
  $page = $element.FindFirst([Windows.Automation.TreeScope]::Descendants, $document)
  $field = $element.FindFirst([Windows.Automation.TreeScope]::Descendants, $password)
  if ($field -and $page.Current.Name -like '*ShvIA*') { break }
  Start-Sleep -Seconds 1
} while ((Get-Date) -lt $deadline)

$rect = New-Object SmokeWindow+Rect
if ([SmokeWindow]::GetWindowRect($Handle, [ref]$rect)) {
  $width = $rect.Right - $rect.Left
  $height = $rect.Bottom - $rect.Top
  if ($width -gt 0 -and $height -gt 0) {
    $image = New-Object System.Drawing.Bitmap($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($image)
    try {
      $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $image.Size)
      $image.Save((Join-Path $EvidenceDirectory 'desktop-window.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    } catch {
      Write-Warning "Window capture unavailable: $($_.Exception.Message)"
    } finally {
      $graphics.Dispose()
      $image.Dispose()
    }
  }
}
if ($field -and $page.Current.Name -like '*ShvIA*') {
  Write-Output "Windows UI Automation verified WebView2 document '$($page.Current.Name)' and its password field."
} else {
  Write-Output "Accessible document: '$($page.Current.Name)'. Checking the WebView2 renderer through CDP."
  node "$PSScriptRoot/verify-webview.mjs" $EvidenceDirectory
  if ($LASTEXITCODE -ne 0) { throw 'The ShvIA login page could not be verified in WebView2.' }
}
