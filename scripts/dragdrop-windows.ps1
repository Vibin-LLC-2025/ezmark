# Requires an unlocked interactive Windows desktop, Windows PowerShell 5.1 or pwsh.
# Example: powershell -STA -File scripts/dragdrop-windows.ps1 -Binary target/debug/ezmark.exe -Out result.json
param(
  [Parameter(Mandatory = $true)][string]$Binary,
  [string]$Out
)
# Actions uses pwsh by default. Use the same built-in .NET Framework WinForms
# runtime as Windows PowerShell 5.1 without installing anything on the runner.
if ($PSVersionTable.PSEdition -eq 'Core') {
  & "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -STA -File $PSCommandPath @PSBoundParameters
  exit $LASTEXITCODE
}
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# The source uses real OLE FileDrop data. No page events, IPC calls, WM_DROPFILES,
# Explorer, clipboard, or shell file associations participate in this test.
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
if (-not ('EzmarkOleDrag' -as [type])) {
  Add-Type -ReferencedAssemblies System.Windows.Forms, System.Drawing @'
using System;
using System.ComponentModel;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows.Forms;
public static class EzmarkOleDrag {
  [StructLayout(LayoutKind.Sequential)] struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT {
    public int dx, dy; public uint data, flags, time; public UIntPtr extra;
  }
  [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT {
    public ushort vk, scan; public uint flags, time; public UIntPtr extra;
  }
  [StructLayout(LayoutKind.Explicit)] struct UNION {
    [FieldOffset(0)] public MOUSEINPUT mouse;
    [FieldOffset(0)] public KEYBDINPUT keyboard;
  }
  [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public UNION u; }
  [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
  [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr process);
  [DllImport("user32.dll")] static extern bool AttachThreadInput(uint from, uint to, bool attach);
  [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h, int command);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int height, uint flags);
  [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr h, uint flags);
  [DllImport("user32.dll", SetLastError=true)] static extern uint SendInput(uint n, INPUT[] input, int size);
  static Form source;
  static Thread thread;
  static readonly ManualResetEvent ready = new ManualResetEvent(false);
  static volatile bool dragging;
  static Exception failure;
  public static void Start(string file, IntPtr target) {
    SetProcessDPIAware();
    Rectangle area = Screen.PrimaryScreen.WorkingArea;
    if (area.Width < 640 || area.Height < 480) throw new Exception("Desktop must be at least 640 x 480");
    ShowWindow(target, 9);
    if (!SetWindowPos(target, new IntPtr(-1), area.Left, area.Top + 90,
        Math.Min(920, area.Width), Math.Min(820, area.Height - 90), 0x40))
      throw new Exception("Could not position target window");
    ready.Reset(); failure = null;
    thread = new Thread(() => {
      try {
        source = new Form { Text = "ezmark OLE drag source", StartPosition = FormStartPosition.Manual,
          Bounds = new Rectangle(area.Left + 10, area.Top + 5, 260, 75), TopMost = true };
        source.MouseDown += (s, e) => {
          if (e.Button != MouseButtons.Left) return;
          dragging = true;
          try {
            var data = new DataObject(DataFormats.FileDrop, new string[] { file });
            source.DoDragDrop(data, DragDropEffects.Copy);
          } catch (Exception ex) { failure = ex; }
          finally { dragging = false; }
        };
        source.Shown += (s, e) => ready.Set();
        Application.Run(source);
      } catch (Exception ex) { failure = ex; ready.Set(); }
    });
    thread.IsBackground = true;
    thread.SetApartmentState(ApartmentState.STA);
    thread.Start();
    if (!ready.WaitOne(10000)) throw new Exception("OLE source window did not appear");
    if (failure != null) throw failure;
  }
  static void Mouse(int x, int y, uint flags) {
    Rectangle screen = SystemInformation.VirtualScreen;
    INPUT input = new INPUT { type = 0 };
    input.u.mouse = new MOUSEINPUT { dx = (int)((x - screen.Left) * 65535L / (screen.Width - 1)),
      dy = (int)((y - screen.Top) * 65535L / (screen.Height - 1)), flags = flags | 0xC001 };
    if (SendInput(1, new INPUT[] { input }, Marshal.SizeOf(typeof(INPUT))) != 1)
      throw new Win32Exception(Marshal.GetLastWin32Error(), "SendInput failed");
  }
  static void Foreground(IntPtr h) {
    for (int i = 0; i < 5; i++) {
      uint current = GetCurrentThreadId();
      uint foreground = GetWindowThreadProcessId(GetForegroundWindow(), IntPtr.Zero);
      uint target = GetWindowThreadProcessId(h, IntPtr.Zero);
      bool joinedForeground = foreground != 0 && foreground != current && AttachThreadInput(current, foreground, true);
      bool joinedTarget = target != current && target != foreground && AttachThreadInput(current, target, true);
      try {
        // Re-raise within the topmost group when other desktop tests appear.
        SetWindowPos(h, new IntPtr(-1), 0, 0, 0, 0, 0x43);
        SetForegroundWindow(h);
      } finally {
        if (joinedTarget) AttachThreadInput(current, target, false);
        if (joinedForeground) AttachThreadInput(current, foreground, false);
      }
      Thread.Sleep(80);
      if (GetForegroundWindow() == h) return;
    }
    throw new Exception("Foreground verification failed, another window may be stealing focus");
  }
  static bool Over(IntPtr h, int x, int y) {
    return GetAncestor(WindowFromPoint(new POINT { X = x, Y = y }), 2) == h;
  }
  public static void Drop(IntPtr target) {
    Rectangle area = Screen.PrimaryScreen.WorkingArea;
    int sx = area.Left + 100, sy = area.Top + 50;
    int tx = area.Left + Math.Min(920, area.Width) / 2;
    int ty = area.Top + 90 + Math.Min(820, area.Height - 90) / 2;
    try {
      Foreground(target);
      Foreground(source.Handle);
      if (!Over(source.Handle, sx, sy)) throw new Exception("OLE source is obscured");
      Mouse(sx, sy, 0);
      Foreground(source.Handle);
      Mouse(sx, sy, 0x2);
      for (int i = 0; i < 20 && !dragging; i++) Thread.Sleep(25);
      if (!dragging) throw new Exception("Source did not enter DoDragDrop after mouse down");
      Foreground(target);
      for (int i = 1; i <= 25; i++) {
        if (GetForegroundWindow() != target) Foreground(target);
        Mouse(sx + (tx - sx) * i / 25, sy + (ty - sy) * i / 25, 0);
        Thread.Sleep(25);
      }
      Foreground(target);
      if (!Over(target, tx, ty)) throw new Exception("Drop point is obscured by another window");
      Mouse(tx, ty, 0x4);
      for (int i = 0; i < 40 && dragging; i++) Thread.Sleep(25);
      if (failure != null) throw failure;
      if (dragging) throw new Exception("OLE drag did not finish");
    } finally { Mouse(tx, ty, 0x4); }
  }
  public static void Stop() {
    if (source != null && !source.IsDisposed) source.BeginInvoke((Action)(() => source.Close()));
    if (thread != null && !thread.Join(5000)) throw new Exception("OLE source thread did not stop");
    source = null; thread = null;
  }
}
'@
}

$process = $null
$exitCode = 1
$id = 'dragdrop-' + [guid]::NewGuid().ToString('N')
$scratch = Join-Path ([IO.Path]::GetTempPath()) $id
$profile = Join-Path ([IO.Path]::GetTempPath()) ('ezmark-harness\' + $id)
try {
  $binaryPath = (Resolve-Path -LiteralPath $Binary).Path
  if ($Out) { $Out = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Out) }
  [void][IO.Directory]::CreateDirectory($scratch)
  # Construct Unicode explicitly so the script also works as BOM-less UTF-8 in PS 5.1.
  $title = 'drag ' + [char]0x00e9 + ' notes.md'
  $file = Join-Path $scratch $title
  [IO.File]::WriteAllText($file, "# OLE drag-drop proof`n`nOpened through the desktop.`n", [Text.UTF8Encoding]::new($false))
  $resultPath = Join-Path $scratch 'result.json'
  $start = [Diagnostics.ProcessStartInfo]::new($binaryPath)
  $start.UseShellExecute = $false
  $start.WorkingDirectory = $scratch
  # Isolate from caller harness settings, notably hold, inject, and asset probes.
  foreach ($key in @($start.EnvironmentVariables.Keys)) {
    if ($key -like 'EZMARK_HARNESS_*') { $start.EnvironmentVariables.Remove($key) }
  }
  $start.EnvironmentVariables['EZMARK_HARNESS_ID'] = $id
  $start.EnvironmentVariables['EZMARK_HARNESS_RENDERS'] = '1'
  $start.EnvironmentVariables['EZMARK_HARNESS_MODE'] = 'report'
  $start.EnvironmentVariables['EZMARK_HARNESS_TIMEOUT_SECS'] = '75'
  $start.EnvironmentVariables['EZMARK_HARNESS_OUT'] = $resultPath
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  $process = [Diagnostics.Process]::Start($start)
  $stderr = $process.StandardError.ReadToEndAsync()
  $deadline = [DateTime]::UtcNow.AddSeconds(40)
  do {
    if ($process.HasExited) { throw "ezmark exited before its window appeared: $($stderr.Result)" }
    $process.Refresh()
    if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $deadline)
  $handle = $process.MainWindowHandle
  if ($handle -eq [IntPtr]::Zero) { throw 'ezmark main window did not appear within 40 seconds' }
  # Wait for the real empty-state render, including slow WebView2 startup on CI.
  $deadline = [DateTime]::UtcNow.AddSeconds(20)
  $empty = $false
  while (-not $empty -and [DateTime]::UtcNow -lt $deadline) {
    $lineTask = $process.StandardOutput.ReadLineAsync()
    $remaining = [Math]::Max(1, [int]($deadline - [DateTime]::UtcNow).TotalMilliseconds)
    if (-not $lineTask.Wait($remaining)) { throw 'No empty-state render before the drop' }
    $line = $lineTask.Result
    if ($null -eq $line) { throw 'Harness stdout closed before the empty-state render' }
    if ($line.StartsWith('EZMARK_HARNESS_EVENT ')) {
      $event = $line.Substring('EZMARK_HARNESS_EVENT '.Length) | ConvertFrom-Json
      $empty = $event.data.kind -ceq 'empty'
    }
  }
  if (-not $empty) { throw 'No empty-state render before the drop' }
  $stdout = $process.StandardOutput.ReadToEndAsync()
  [EzmarkOleDrag]::Start($file, $handle)
  for ($attempt = 1; $attempt -le 3; $attempt++) {
    try { [EzmarkOleDrag]::Drop($handle) }
    catch { Write-Warning "Drop attempt $attempt failed: $_" }
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath $resultPath) -and -not $process.HasExited -and [DateTime]::UtcNow -lt $deadline) {
      Start-Sleep -Milliseconds 100
    }
    if ((Test-Path -LiteralPath $resultPath) -or $process.HasExited) { break }
  }
  if (-not $process.WaitForExit(15000)) { throw 'No harness result after three OLE drop attempts' }
  if (-not (Test-Path -LiteralPath $resultPath)) { throw "Harness did not write JSON: $($stderr.Result)" }
  $json = [IO.File]::ReadAllText($resultPath)
  if ($Out) { [IO.File]::WriteAllText($Out, $json, [Text.UTF8Encoding]::new($false)) }
  $result = $json | ConvertFrom-Json
  if ($result.timedOut -ne $false) { throw "Harness timed out: $json" }
  $matching = @($result.opens | Where-Object {
    $_.origin -ceq 'drag-drop' -and [string]::Equals($_.path, $file, [StringComparison]::OrdinalIgnoreCase)
  })
  if ($matching.Count -eq 0) { throw "No drag-drop open for the Unicode file: $json" }
  if ($result.view.kind -cne 'document' -or $result.view.title -cne $title) { throw "Wrong rendered view: $json" }
  if ($result.page.renders[0].kind -cne 'empty') { throw "Window did not start in the empty state: $json" }
  if ($process.ExitCode -ne 0) { throw "Harness exit code $($process.ExitCode): $json" }
  Write-Output "PASS: real OLE drop opened '$title', origin=drag-drop, timedOut=false"
  $exitCode = 0
} catch {
  [Console]::Error.WriteLine("FAIL: $_")
} finally {
  try { [EzmarkOleDrag]::Stop() } catch { [Console]::Error.WriteLine("Cleanup failed: $_"); $exitCode = 1 }
  if ($null -ne $process) {
    if (-not $process.HasExited) {
      & "$env:SystemRoot\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null
      if (-not $process.WaitForExit(5000)) {
        $process.Kill()
        if (-not $process.WaitForExit(5000)) { throw 'Could not terminate the harness process' }
      }
    }
    $process.Dispose()
  }
  # Only these two GUID-scoped directories belong to this invocation.
  foreach ($path in @($scratch, $profile)) {
    $full = [IO.Path]::GetFullPath($path)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $full.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($full) -cne $id) {
      [Console]::Error.WriteLine("Refusing cleanup outside the invocation directory: $full"); $exitCode = 1; continue
    }
    for ($retry = 0; $retry -lt 20 -and (Test-Path -LiteralPath $full); $retry++) {
      try { Remove-Item -LiteralPath $full -Recurse -Force } catch { Start-Sleep -Milliseconds 250 }
    }
    if (Test-Path -LiteralPath $full) { [Console]::Error.WriteLine("Could not clean up $full"); $exitCode = 1 }
  }
}
exit $exitCode
