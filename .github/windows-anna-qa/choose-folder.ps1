param([int]$AppId, [string]$ProjectFolder)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;

public static class PickerFocus {
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")]
    public static extern IntPtr GetDlgItem(IntPtr hwnd, int id);
    [DllImport("user32.dll")]
    public static extern bool IsWindowEnabled(IntPtr hwnd);
    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
}
'@
if ($ProjectFolder -notmatch '^[A-Za-z]:\\[\w\\ .-]+$' -or
    -not (Test-Path -LiteralPath $ProjectFolder -PathType Container)) {
  throw 'Invalid synthetic project folder'
}
$scope = [System.Windows.Automation.TreeScope]::Descendants
$root = [System.Windows.Automation.AutomationElement]::RootElement
$pidCondition = [System.Windows.Automation.PropertyCondition]::new(
  [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $AppId)
$dialog = $null
$until = (Get-Date).AddSeconds(22)
do {
  $windows = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $pidCondition)
  $dialog = $windows | Where-Object { $_.Current.ClassName -eq '#32770' -and
    $_.Current.Name -match '^(Select Folder|Choose Folder|Open|Selecionar pasta)$' } | Select-Object -First 1
  if ($dialog) { break }
  Start-Sleep -Milliseconds 350
} while ((Get-Date) -lt $until)
if (-not $dialog) { throw "Folder picker did not appear. Window titles: $(@($windows | ForEach-Object { $_.Current.Name }) -join ', ')" }
$handle = [IntPtr]$dialog.Current.NativeWindowHandle
$folderName = [System.Windows.Automation.PropertyCondition]::new(
  [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
  [System.Windows.Automation.ControlType]::Edit)
$editor = $dialog.FindAll($scope, $folderName) | Where-Object {
  $_.Current.Name -match '^(Folder:|Folder name:|Pasta:|Nome da pasta:)$' -or $_.Current.AutomationId -eq '1152'
} | Select-Object -First 1
$selection = [System.Windows.Automation.PropertyCondition]::new(
  [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
  [System.Windows.Automation.ControlType]::Button)
$button = $dialog.FindAll($scope, $selection) | Where-Object {
  $_.Current.AutomationId -eq '1' -or
    $_.Current.Name -match '^(&?Select Folder|&?Choose Folder|&?Selecionar pasta|&?Open|&?Abrir)$'
} | Select-Object -First 1
if ($editor -and $button) {
  $editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($ProjectFolder)
  $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
} else {
  [void][PickerFocus]::SetForegroundWindow($handle)
  if ([PickerFocus]::GetForegroundWindow() -ne $handle) { throw 'Folder picker is not the foreground window' }
  [System.Windows.Forms.SendKeys]::SendWait('%d')
  [System.Windows.Forms.SendKeys]::SendWait($ProjectFolder + '{ENTER}')
  Start-Sleep -Milliseconds 700
  if ([PickerFocus]::GetForegroundWindow() -eq $handle) {
    $button = $dialog.FindAll($scope, $selection) | Where-Object {
      $_.Current.AutomationId -eq '1' -or
    $_.Current.Name -match '^(&?Select Folder|&?Choose Folder|&?Selecionar pasta|&?Open|&?Abrir)$'
    } | Select-Object -First 1
    if ($button) {
      $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    } else {
      # IFileDialog uses IDOK for the confirmation even when UI Automation omits it.
      $confirm = [PickerFocus]::GetDlgItem($handle, 1)
      if ($confirm -eq [IntPtr]::Zero -or -not [PickerFocus]::IsWindowEnabled($confirm) -or
          -not [PickerFocus]::PostMessage($confirm, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)) {
        throw 'Could not confirm the folder in the foreground Windows picker'
      }
    }
  }
}
Write-Output 'Selected the synthetic folder through the Windows folder picker.'
