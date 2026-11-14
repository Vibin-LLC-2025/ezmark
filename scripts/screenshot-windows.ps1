# Real-app screenshot on Windows: launch ezmark on a file, wait for it to
# render, capture its window from the screen. Used by CI and by hand before
# each release (plan section 11).
#
#   powershell -File scripts/screenshot-windows.ps1 -Binary target\release\ezmark.exe -File doc.md -Out shot.png
#
# With a test-harness build the script waits for the harness result line
# (hold mode keeps the window open and on top); with a production build it
# waits a fixed time.
param(
  [Parameter(Mandatory = $true)][string]$Binary,
  [Parameter(Mandatory = $true)][string]$File,
  [Parameter(Mandatory = $true)][string]$Out,
  [int]$WaitSeconds = 10
)
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class EzmarkWin {
  public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hWnd, int attribute, out RECT rect, int size);
}
'@
[void][EzmarkWin]::SetProcessDPIAware()

$log = Join-Path ([IO.Path]::GetTempPath()) ("ezmark-screenshot-" + [guid]::NewGuid().ToString('N') + '.log')
$env:EZMARK_HARNESS_MODE = 'hold'
$env:EZMARK_HARNESS_ID = 'screenshot'
$process = Start-Process -FilePath $Binary -ArgumentList ('"' + $File + '"') -PassThru -NoNewWindow -RedirectStandardOutput $log

try {
  $deadline = (Get-Date).AddSeconds(90)
  while ($process.MainWindowHandle -eq [IntPtr]::Zero) {
    if ((Get-Date) -gt $deadline) { throw 'ezmark window did not appear' }
    if ($process.HasExited) { throw "ezmark exited early with code $($process.ExitCode)" }
    Start-Sleep -Milliseconds 250
    $process.Refresh()
  }
  $rendered = (Get-Date).AddSeconds($WaitSeconds)
  while ((Get-Date) -lt $rendered) {
    if ((Test-Path $log) -and (Select-String -Path $log -Pattern 'EZMARK_HARNESS_RESULT' -Quiet)) { break }
    Start-Sleep -Milliseconds 250
  }
  Start-Sleep -Milliseconds 750

  $handle = $process.MainWindowHandle
  [void][EzmarkWin]::SetForegroundWindow($handle)
  Start-Sleep -Milliseconds 500
  $rect = New-Object EzmarkWin+RECT
  # DWMWA_EXTENDED_FRAME_BOUNDS = 9: the visible frame, without the invisible resize border.
  [void][EzmarkWin]::DwmGetWindowAttribute($handle, 9, [ref]$rect, [Runtime.InteropServices.Marshal]::SizeOf($rect))
  $width = $rect.Right - $rect.Left
  $height = $rect.Bottom - $rect.Top
  if ($width -le 0 -or $height -le 0) { throw "bad window bounds: $width x $height" }

  $bitmap = New-Object System.Drawing.Bitmap $width, $height
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
  $bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
  $graphics.Dispose()
  $bitmap.Dispose()
  Write-Output "saved $Out ($width x $height)"
}
finally {
  if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
  Remove-Item -ErrorAction SilentlyContinue $log
}
