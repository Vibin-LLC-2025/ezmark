# Network watch on Windows: launch ezmark, follow its whole process tree
# (ezmark.exe plus every msedgewebview2.exe it spawns), and record every TCP
# connection and UDP endpoint any of those processes owns. Needs no elevation.
#
#   powershell -ExecutionPolicy Bypass -File scripts/netwatch-windows.ps1 `
#     -Binary target\release\ezmark.exe -File docs\media\demo\demo.md -Seconds 30 -Out netwatch.json
#
# Optional: -NetLog (signal 2 below), -UserDataFolder <empty dir> (first-run
# behaviour without touching the real profile), -BrowserArgs "<switches>"
# (try WebView2 browser arguments without rebuilding), -IntervalMs,
# -IgnoreRunning.
#
# Exit code: 0 when no remote endpoint outside loopback (127.0.0.0/8, ::1) was
# seen by any signal, 1 when one was, 2 when the measurement itself failed
# (for example the app exited at once because another instance was running).
#
# Signals, and what each can and cannot prove:
#
# 1. Socket table sampling (always on). GetExtendedTcpTable and
#    GetExtendedUdpTable (the API behind Get-NetTCPConnection, called directly
#    because the cmdlets take hundreds of milliseconds per call) are polled in
#    a tight loop on a background thread. The process tree is followed with a
#    Toolhelp snapshot every 100 ms (the same parent pid that
#    Win32_Process.ParentProcessId reports) and with Win32_Process about once
#    a second; socket rows are kept for every pid and matched to the tree at
#    the end, so a child's first sockets are not missed. Proves: any TCP
#    connection that existed at a sample instant, with its remote address.
#    Cannot prove: a connection that opened and closed between two samples
#    (the achieved interval is reported; a TCP handshake to a remote host
#    normally outlives it, a refused or reset connect may not), and it shows
#    no remote address for UDP at all (only that a socket was bound).
#
# 2. Chromium NetLog (-NetLog). Passes --log-net-log to the webview through
#    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS. The network service then logs every
#    URL request, DNS lookup and socket connect it performs, with no sampling
#    gap. With -NetLog a non-loopback TCP connect, UDP send, DNS lookup or URL
#    in the log also fails the run. Proves: everything the Chromium network
#    stack did, including short-lived and UDP (QUIC, DNS) traffic. Cannot
#    prove: anything about ezmark.exe itself (covered only by signal 1) or
#    about sockets that a webview process opens outside the Chromium network
#    stack. The webview browser process does exactly that for Microsoft
#    account sign-in (OneAuth over WinHTTP); only signal 1 sees those. It also
#    changes the command line under test.
#
# 3. DNS client cache diff (always on, advisory only). Names that appeared in
#    the system resolver cache during the run. System-wide, so any other
#    program adds noise, and Chromium can resolve names without touching it.
#    A new name is a lead to investigate, never proof; an empty diff proves
#    little. It does not affect the exit code. On some Windows builds the
#    cache reads as empty without elevation, which makes this signal void.
#
# Blind spots that no unelevated signal here closes:
# - traffic that shared system services make on the app's behalf (the DNS
#   Client service, WinHTTP proxy discovery, BITS, Delivery Optimization, the
#   Edge updater service) runs in processes outside the tree;
# - connections shorter than the sampling interval made by ezmark.exe itself;
# - anything after the watch ends. The webview runs timers: its component
#   updater first fires 60 seconds after start, so a run shorter than about
#   90 seconds cannot see it, and slower timers (hours, days) are out of reach;
# - behaviour that varies from run to run. The sign-in connections appeared
#   in most runs but not all, anywhere from 2 to 60 seconds after start, so
#   one clean run proves little: repeat it, and use a fresh -UserDataFolder;
# - behaviour that depends on the machine (a Microsoft account signed in to
#   Windows, proxy auto-detection switched on, group policy);
# - raw sockets and kernel-level traffic.
# An elevated ETW trace (Microsoft-Windows-Kernel-Network) or a packet capture
# would close most of these. This script does not attempt either.
param(
  [Parameter(Mandatory = $true)][string]$Binary,
  [string]$File = '',
  [int]$Seconds = 30,
  [string]$Out = '',
  # Record a Chromium NetLog and fold its socket-level events into the verdict.
  [switch]$NetLog,
  # Extra WebView2 browser arguments for the child (WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS).
  [string]$BrowserArgs = '',
  # Use this WebView2 user data folder (WEBVIEW2_USER_DATA_FOLDER). Point it at
  # an empty directory to measure first-run behaviour without deleting anything.
  [string]$UserDataFolder = '',
  # Pause between samples in milliseconds. 0 is a tight loop on one core.
  [int]$IntervalMs = 0,
  # Launch even though a process with the binary's name is already running.
  # ezmark is single-instance: a second launch hands its file to the running
  # instance and exits, which both voids the measurement and disturbs that
  # instance. Only for a build with a different app identifier.
  [switch]$IgnoreRunning
)
$ErrorActionPreference = 'Stop'

Add-Type -Language CSharp @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Net;
using System.Runtime.InteropServices;
using System.Threading;

namespace EzmarkNetwatch {
  public class Proc {
    public int Pid; public int ParentPid; public string Name; public double FirstSeenMs;
  }
  public class Row {
    public string Process; public int Pid; public string Protocol;
    public string Local; public string Remote; public string State;
    public bool RemoteIsLoopback; public bool HasRemote;
    public double FirstSeenMs; public double LastSeenMs; public int Samples;
  }

  public class Sampler {
    [DllImport("iphlpapi.dll", SetLastError = true)]
    static extern uint GetExtendedTcpTable(IntPtr table, ref int size, bool order, int af, int tableClass, int reserved);
    [DllImport("iphlpapi.dll", SetLastError = true)]
    static extern uint GetExtendedUdpTable(IntPtr table, ref int size, bool order, int af, int tableClass, int reserved);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint pid);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    static extern bool Process32FirstW(IntPtr snapshot, ref PROCESSENTRY32 entry);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    static extern bool Process32NextW(IntPtr snapshot, ref PROCESSENTRY32 entry);
    [DllImport("kernel32.dll")]
    static extern bool CloseHandle(IntPtr handle);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct PROCESSENTRY32 {
      public uint dwSize; public uint cntUsage; public uint th32ProcessID; public IntPtr th32DefaultHeapID;
      public uint th32ModuleID; public uint cntThreads; public uint th32ParentProcessID; public int pcPriClassBase;
      public uint dwFlags;
      [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string szExeFile;
    }

    const int AF_INET = 2, AF_INET6 = 23;
    const int TCP_TABLE_OWNER_PID_ALL = 5, UDP_TABLE_OWNER_PID = 1;
    static readonly string[] TcpStates = { "?", "Closed", "Listen", "SynSent", "SynReceived", "Established",
      "FinWait1", "FinWait2", "CloseWait", "Closing", "LastAck", "TimeWait", "DeleteTcb" };

    readonly object gate = new object();
    readonly Dictionary<int, Proc> tree = new Dictionary<int, Proc>();
    readonly Dictionary<string, Row> rows = new Dictionary<string, Row>();
    readonly Stopwatch clock = new Stopwatch();
    Thread thread; volatile bool stop; int intervalMs;
    IntPtr buffer = IntPtr.Zero; int bufferSize = 0;
    double lastTreeMs; public int TreeScans;
    public int SampleCount; public double MaxGapMs; public double TotalMs; public string Error;
    public List<double> Gaps = new List<double>();

    public double ElapsedMs { get { return clock.Elapsed.TotalMilliseconds; } }
    public void StartClock() { clock.Start(); }

    public void Start(int rootPid, string rootName, int interval) {
      intervalMs = interval;
      lock (gate) { tree[rootPid] = new Proc { Pid = rootPid, ParentPid = 0, Name = rootName, FirstSeenMs = ElapsedMs }; }
      thread = new Thread(Loop); thread.IsBackground = true; thread.Priority = ThreadPriority.AboveNormal; thread.Start();
    }
    public void Stop() { stop = true; if (thread != null) thread.Join(); TotalMs = ElapsedMs; }

    // A process found some other way (for example by its command line).
    public void AddProcess(int pid, int parentPid, string name) {
      lock (gate) { if (!tree.ContainsKey(pid)) tree[pid] = new Proc { Pid = pid, ParentPid = parentPid, Name = name, FirstSeenMs = ElapsedMs }; }
    }
    public Proc[] Processes() { lock (gate) { return new List<Proc>(tree.Values).ToArray(); } }
    // Rows are recorded for every pid on the machine and filtered here, so a
    // socket opened before its process was discovered is still attributed.
    public Row[] Rows() {
      lock (gate) {
        List<Row> owned = new List<Row>();
        foreach (Row row in rows.Values) {
          Proc owner;
          if (tree.TryGetValue(row.Pid, out owner)) { row.Process = owner.Name; owned.Add(row); }
        }
        return owned.ToArray();
      }
    }
    public int LiveCount() {
      int live = 0;
      foreach (Proc p in Processes()) { try { Process.GetProcessById(p.Pid).Dispose(); live++; } catch (ArgumentException) { } }
      return live;
    }

    void Loop() {
      double last = ElapsedMs;
      try {
        while (!stop) {
          SampleOnce();
          double now = ElapsedMs; double gap = now - last; last = now;
          if (SampleCount > 0) { Gaps.Add(gap); if (gap > MaxGapMs) MaxGapMs = gap; }
          SampleCount++;
          if (intervalMs > 0) Thread.Sleep(intervalMs);
        }
      } catch (Exception e) { Error = e.ToString(); }
      finally { if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer); buffer = IntPtr.Zero; }
    }

    void SampleOnce() {
      double now = ElapsedMs;
      ReadTable(true, AF_INET, now); ReadTable(true, AF_INET6, now);
      ReadTable(false, AF_INET, now); ReadTable(false, AF_INET6, now);
      // The process snapshot costs several times more than the socket tables,
      // so it runs at most every 100 ms. Nothing is lost: socket rows are kept
      // for all pids and matched to the tree when the results are read.
      if (now - lastTreeMs < 100 && lastTreeMs > 0) return;
      lastTreeMs = now; TreeScans++;
      IntPtr snapshot = CreateToolhelp32Snapshot(2, 0);
      if (snapshot != new IntPtr(-1)) {
        try {
          PROCESSENTRY32 entry = new PROCESSENTRY32();
          entry.dwSize = (uint)Marshal.SizeOf(typeof(PROCESSENTRY32));
          List<PROCESSENTRY32> all = new List<PROCESSENTRY32>();
          if (Process32FirstW(snapshot, ref entry)) { do { all.Add(entry); } while (Process32NextW(snapshot, ref entry)); }
          lock (gate) {
            bool grew = true;
            while (grew) {
              grew = false;
              foreach (PROCESSENTRY32 e in all) {
                int pid = (int)e.th32ProcessID, parent = (int)e.th32ParentProcessID;
                if (pid != 0 && !tree.ContainsKey(pid) && tree.ContainsKey(parent)) {
                  tree[pid] = new Proc { Pid = pid, ParentPid = parent, Name = e.szExeFile, FirstSeenMs = now };
                  grew = true;
                }
              }
            }
          }
        } finally { CloseHandle(snapshot); }
      }
    }

    void ReadTable(bool tcp, int af, double now) {
      uint result = 0;
      for (int attempt = 0; attempt < 4; attempt++) {
        int size = bufferSize;
        result = tcp ? GetExtendedTcpTable(buffer, ref size, false, af, TCP_TABLE_OWNER_PID_ALL, 0)
                     : GetExtendedUdpTable(buffer, ref size, false, af, UDP_TABLE_OWNER_PID, 0);
        if (result != 122) break; // ERROR_INSUFFICIENT_BUFFER
        if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
        bufferSize = size + 4096; buffer = Marshal.AllocHGlobal(bufferSize);
      }
      if (result != 0) throw new InvalidOperationException("socket table read failed with error " + result);
      int count = Marshal.ReadInt32(buffer);
      bool v6 = af == AF_INET6;
      int rowSize = tcp ? (v6 ? 56 : 24) : (v6 ? 28 : 12);
      lock (gate) {
        for (int i = 0; i < count; i++) {
          IntPtr row = IntPtr.Add(buffer, 4 + i * rowSize);
          int pid; string local, remote = ""; string state = ""; IPAddress remoteAddress = null; int remotePort = 0;
          if (tcp && !v6) {
            pid = Marshal.ReadInt32(row, 20);
            state = StateName(Marshal.ReadInt32(row, 0));
            local = Endpoint(Address(row, 4, 4), Port(row, 8));
            remoteAddress = Address(row, 12, 4); remotePort = Port(row, 16);
          } else if (tcp) {
            pid = Marshal.ReadInt32(row, 52);
            state = StateName(Marshal.ReadInt32(row, 48));
            local = Endpoint(Address(row, 0, 16), Port(row, 20));
            remoteAddress = Address(row, 24, 16); remotePort = Port(row, 44);
          } else if (!v6) {
            pid = Marshal.ReadInt32(row, 8);
            local = Endpoint(Address(row, 0, 4), Port(row, 4));
          } else {
            pid = Marshal.ReadInt32(row, 24);
            local = Endpoint(Address(row, 0, 16), Port(row, 20));
          }
          bool hasRemote = false, loopback = false;
          if (tcp) {
            // A listening socket reports an all-zero remote address and port 0.
            hasRemote = !(remotePort == 0 && (remoteAddress.Equals(IPAddress.Any) || remoteAddress.Equals(IPAddress.IPv6Any)));
            if (hasRemote) { remote = Endpoint(remoteAddress, remotePort); loopback = IsLoopback(remoteAddress); }
          }
          string protocol = (tcp ? "TCP" : "UDP") + (v6 ? "6" : "4");
          string key = pid + "|" + protocol + "|" + local + "|" + remote + "|" + state;
          Row found;
          if (!rows.TryGetValue(key, out found)) {
            found = new Row { Pid = pid, Protocol = protocol, Local = local, Remote = remote,
              State = state, HasRemote = hasRemote, RemoteIsLoopback = loopback, FirstSeenMs = now };
            rows[key] = found;
          }
          found.LastSeenMs = now; found.Samples++;
        }
      }
    }

    static string StateName(int state) { return state > 0 && state < TcpStates.Length ? TcpStates[state] : "State" + state; }
    static IPAddress Address(IntPtr row, int offset, int length) {
      byte[] bytes = new byte[length]; Marshal.Copy(IntPtr.Add(row, offset), bytes, 0, length); return new IPAddress(bytes);
    }
    static int Port(IntPtr row, int offset) { return (Marshal.ReadByte(row, offset) << 8) | Marshal.ReadByte(row, offset + 1); }
    static string Endpoint(IPAddress address, int port) {
      return (address.AddressFamily == System.Net.Sockets.AddressFamily.InterNetworkV6 ? "[" + address + "]" : address.ToString()) + ":" + port;
    }
    public static bool IsLoopback(IPAddress address) {
      if (address.IsIPv4MappedToIPv6) address = address.MapToIPv4();
      return IPAddress.IsLoopback(address);
    }
  }
}
'@

function Get-DnsCacheNames {
  # Advisory signal only. Returns $null when the cache cannot be read.
  try {
    $names = @(Get-DnsClientCache -ErrorAction Stop | ForEach-Object { $_.Entry.ToLowerInvariant() } | Sort-Object -Unique)
    return , $names
  }
  catch { return $null }
}

function Test-LoopbackHost([string]$hostName) {
  $name = $hostName.Trim('[', ']').ToLowerInvariant()
  if ($name -eq 'localhost' -or $name.EndsWith('.localhost')) { return $true }
  $address = $null
  if ([Net.IPAddress]::TryParse($name, [ref]$address)) { return [EzmarkNetwatch.Sampler]::IsLoopback($address) }
  return $false
}

function Read-NetLog([string]$path) {
  # The NetLog is one JSON document with one event per line. It is read line
  # by line so a log cut short by a killed process still parses.
  $result = [ordered]@{ path = $path; present = $false; events = 0; proxyAutoDetect = $false
    remoteTcp = @(); remoteUdpSent = @(); remoteUdpConnectOnly = @(); remoteDns = @(); remoteUrls = @()
    loopbackTcp = @(); localUrls = @() }
  if (-not (Test-Path -LiteralPath $path)) { return $result }
  $result.present = $true
  $lines = [IO.File]::ReadAllLines($path)
  $types = @{}
  $match = [regex]::Match($lines[0], '"logEventTypes"\s*:\s*\{([^}]*)\}')
  if ($match.Success) {
    foreach ($pair in [regex]::Matches($match.Groups[1].Value, '"([A-Z0-9_]+)"\s*:\s*(\d+)')) { $types[[int]$pair.Groups[2].Value] = $pair.Groups[1].Value }
  }
  $remoteTcp = @{}; $loopbackTcp = @{}; $udpAddress = @{}; $udpBytes = @{}; $dns = @{}; $remoteUrls = @{}; $localUrls = @{}
  foreach ($line in $lines) {
    if (-not $line.StartsWith('{"') -or $line.StartsWith('{"constants"')) { continue }
    $entry = $null
    try { $entry = $line.TrimEnd().TrimEnd(',') | ConvertFrom-Json } catch { continue }
    $type = $types[[int]$entry.type]
    if (-not $type) { continue }
    $result.events++
    $params = $entry.params
    $source = [string]$entry.source.id
    if ($type -like 'WPAD_*' -or $type -like 'PAC_FILE_DECIDER*') { $result.proxyAutoDetect = $true }
    if (-not $params) { continue }
    if ($type -eq 'TCP_CONNECT_ATTEMPT' -and $entry.phase -eq 1 -and $params.address) {
      $hostPart = $params.address -replace ':\d+$', ''
      if (Test-LoopbackHost $hostPart) { $loopbackTcp[$params.address] = 1 + [int]$loopbackTcp[$params.address] }
      else { $remoteTcp[$params.address] = 1 + [int]$remoteTcp[$params.address] }
    }
    elseif ($type -eq 'UDP_CONNECT' -and $params.address) { $udpAddress[$source] = $params.address }
    elseif ($type -eq 'UDP_BYTES_SENT') { $udpBytes[$source] = [long]$udpBytes[$source] + [long]$params.byte_count }
    elseif ($type -in 'HOST_RESOLVER_MANAGER_REQUEST', 'HOST_RESOLVER_MANAGER_JOB', 'HOST_RESOLVER_DNS_TASK' -and $params.host) {
      $name = $params.host -replace '^[a-z]+://', '' -replace ':\d+$', ''
      if (-not (Test-LoopbackHost $name)) { $dns[$name] = $true }
    }
    if ($params.url) {
      $uri = $null
      if ([Uri]::TryCreate([string]$params.url, 'Absolute', [ref]$uri) -and $uri.Scheme -in 'http', 'https', 'ws', 'wss' -and -not (Test-LoopbackHost $uri.Host)) {
        $remoteUrls[[string]$params.url] = $true
      } else { $localUrls[[string]$params.url] = $true }
    }
  }
  $udpSent = @{}; $udpIdle = @{}
  foreach ($source in $udpAddress.Keys) {
    $address = $udpAddress[$source]
    if (Test-LoopbackHost ($address -replace ':\d+$', '')) { continue }
    if ([long]$udpBytes[$source] -gt 0) { $udpSent[$address] = [long]$udpSent[$address] + [long]$udpBytes[$source] }
    else { $udpIdle[$address] = 1 + [int]$udpIdle[$address] }
  }
  $result.remoteTcp = @($remoteTcp.Keys | Sort-Object | ForEach-Object { "$_ (x$($remoteTcp[$_]))" })
  $result.loopbackTcp = @($loopbackTcp.Keys | Sort-Object | ForEach-Object { "$_ (x$($loopbackTcp[$_]))" })
  $result.remoteUdpSent = @($udpSent.Keys | Sort-Object | ForEach-Object { "$_ ($($udpSent[$_]) bytes)" })
  $result.remoteUdpConnectOnly = @($udpIdle.Keys | Sort-Object | ForEach-Object { "$_ (x$($udpIdle[$_]))" })
  $result.remoteDns = @($dns.Keys | Sort-Object)
  $result.remoteUrls = @($remoteUrls.Keys | Sort-Object)
  $result.localUrls = @($localUrls.Keys | Sort-Object)
  return $result
}

$binaryPath = (Resolve-Path -LiteralPath $Binary).Path
$filePath = ''
if ($File) { $filePath = (Resolve-Path -LiteralPath $File).Path }

$already = @(Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($binaryPath)) -ErrorAction SilentlyContinue)
if ($already.Count -and -not $IgnoreRunning) {
  Write-Output "netwatch: refusing to launch, already running: pid $(($already | ForEach-Object Id) -join ', '). Close it, or pass -IgnoreRunning for a build with a different identifier."
  exit 2
}

$netLogPath = ''
$browserArguments = $BrowserArgs
if ($NetLog) {
  $netLogPath = Join-Path ([IO.Path]::GetTempPath()) ('ezmark-netlog-' + [guid]::NewGuid().ToString('N') + '.json')
  $browserArguments = ('--log-net-log="' + $netLogPath + '" ' + $browserArguments).Trim()
}

$start = New-Object Diagnostics.ProcessStartInfo
$start.FileName = $binaryPath
$start.UseShellExecute = $false
$start.WorkingDirectory = (Get-Location).Path
if ($filePath) { $start.Arguments = '"' + $filePath + '"' }
if ($browserArguments) { $start.EnvironmentVariables['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS'] = $browserArguments }
if ($UserDataFolder) {
  [void](New-Item -ItemType Directory -Force -Path $UserDataFolder)
  $start.EnvironmentVariables['WEBVIEW2_USER_DATA_FOLDER'] = (Resolve-Path -LiteralPath $UserDataFolder).Path
}

$dnsBefore = Get-DnsCacheNames
$sampler = New-Object EzmarkNetwatch.Sampler
$startedAt = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
$sampler.StartClock()
$process = [Diagnostics.Process]::Start($start)
$sampler.Start($process.Id, [IO.Path]::GetFileName($binaryPath), $IntervalMs)

$commandLines = @{}
$earlyExit = $false
try {
  $deadline = $Seconds * 1000
  while ($sampler.ElapsedMs -lt $deadline) {
    Start-Sleep -Milliseconds 500
    if ($process.HasExited) { $earlyExit = $true; break }
    # Slow path, about once a second: Win32_Process gives command lines, which
    # both document the browser arguments in effect and catch any webview
    # process that is not a descendant by parent pid.
    $known = @{}
    foreach ($p in $sampler.Processes()) { $known[$p.Pid] = $true }
    foreach ($p in Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, CommandLine) {
      $id = [int]$p.ProcessId
      if ($known.ContainsKey($id) -or $known.ContainsKey([int]$p.ParentProcessId)) {
        $sampler.AddProcess($id, [int]$p.ParentProcessId, $p.Name)
        if ($p.CommandLine) { $commandLines[$id] = $p.CommandLine }
      }
    }
  }
}
finally {
  # Close politely so the webview can flush its NetLog, and keep sampling
  # through shutdown: browsers sometimes send on exit.
  if (-not $process.HasExited) {
    [void]$process.CloseMainWindow()
    $closeBy = $sampler.ElapsedMs + 8000
    while ($sampler.LiveCount() -gt 0 -and $sampler.ElapsedMs -lt $closeBy) { Start-Sleep -Milliseconds 100 }
  }
  $forced = @()
  foreach ($p in $sampler.Processes()) {
    $running = Get-Process -Id $p.Pid -ErrorAction SilentlyContinue
    if ($running -and $running.ProcessName -eq [IO.Path]::GetFileNameWithoutExtension($p.Name)) {
      $forced += $p.Pid
      Stop-Process -Id $p.Pid -Force -ErrorAction SilentlyContinue
    }
  }
  $sampler.Stop()
}
if ($sampler.Error) { Write-Error "sampler failed: $($sampler.Error)"; exit 2 }

$dnsAfter = Get-DnsCacheNames
$dnsNew = $null
if ($null -ne $dnsBefore -and $null -ne $dnsAfter) { $dnsNew = @($dnsAfter | Where-Object { $dnsBefore -notcontains $_ }) }

$gaps = @($sampler.Gaps | Sort-Object)
$percentile = { param($q) if ($gaps.Count) { [math]::Round($gaps[[math]::Min($gaps.Count - 1, [int]($gaps.Count * $q))], 2) } else { 0 } }
$sampling = [ordered]@{
  samples       = $sampler.SampleCount
  watchedMs     = [math]::Round($sampler.TotalMs)
  meanMs        = if ($gaps.Count) { [math]::Round(($gaps | Measure-Object -Average).Average, 2) } else { 0 }
  medianMs      = & $percentile 0.5
  p99Ms         = & $percentile 0.99
  maxMs         = [math]::Round($sampler.MaxGapMs, 2)
}

$processes = @($sampler.Processes() | Sort-Object FirstSeenMs, Pid | ForEach-Object {
  $role = ''
  $line = [string]$commandLines[$_.Pid]
  if ($line -match '--type=([a-z-]+)') { $role = $Matches[1] }
  if ($line -match '--utility-sub-type=([A-Za-z0-9._]+)') { $role += ' (' + $Matches[1] + ')' }
  if (-not $role -and $_.Name -like 'msedgewebview2*' -and $line) { $role = 'browser' }
  [pscustomobject][ordered]@{ pid = $_.Pid; parentPid = $_.ParentPid; name = $_.Name; role = $role
    firstSeenMs = [math]::Round($_.FirstSeenMs); commandLine = $line }
})
$roles = @{}
foreach ($p in $processes) { $roles[$p.pid] = $p.role }

$endpoints = @($sampler.Rows() | Sort-Object FirstSeenMs, Pid, Protocol, Local | ForEach-Object {
  [pscustomobject][ordered]@{ process = $_.Process; pid = $_.Pid; role = $roles[$_.Pid]; protocol = $_.Protocol
    local = $_.Local; remote = $_.Remote; state = $_.State
    scope = if (-not $_.HasRemote) { 'none' } elseif ($_.RemoteIsLoopback) { 'loopback' } else { 'REMOTE' }
    firstSeenMs = [math]::Round($_.FirstSeenMs); lastSeenMs = [math]::Round($_.LastSeenMs); samples = $_.Samples }
})
$remoteRows = @($endpoints | Where-Object { $_.scope -eq 'REMOTE' })

$netLogResult = $null
if ($NetLog) {
  $netLogResult = Read-NetLog $netLogPath
  Remove-Item -LiteralPath $netLogPath -ErrorAction SilentlyContinue
}
$netLogRemote = $false
if ($netLogResult -and ($netLogResult.remoteTcp.Count -or $netLogResult.remoteUdpSent.Count -or $netLogResult.remoteDns.Count -or $netLogResult.remoteUrls.Count)) { $netLogRemote = $true }
$netLogMissing = [bool]($NetLog -and -not $netLogResult.present)

$verdict = if ($earlyExit) { 'invalid: the app exited before the watch ended (is another instance running?)' }
  elseif ($netLogMissing) { 'invalid: -NetLog was requested but the webview wrote no log' }
  elseif ($remoteRows.Count -or $netLogRemote) { 'non-loopback network activity observed' }
  else { 'no non-loopback network activity observed' }

$report = [ordered]@{
  tool          = 'netwatch-windows.ps1'
  startedAtUtc  = $startedAt
  binary        = $binaryPath
  file          = $filePath
  seconds       = $Seconds
  browserArgs   = $browserArguments
  userDataFolder = $UserDataFolder
  elevated      = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  sampling      = $sampling
  forcedKill    = $forced
  processes     = $processes
  endpoints     = $endpoints
  netLog        = $netLogResult
  dnsCacheNew   = $dnsNew
  verdict       = $verdict
}

Write-Output ''
Write-Output "netwatch: $binaryPath $filePath"
Write-Output ("sampling: {0} samples over {1} ms; interval mean {2} ms, median {3} ms, p99 {4} ms, max {5} ms" -f `
  $sampling.samples, $sampling.watchedMs, $sampling.meanMs, $sampling.medianMs, $sampling.p99Ms, $sampling.maxMs)
Write-Output ''
Write-Output "process tree ($($processes.Count) processes):"
$processes | Format-Table pid, parentPid, name, role, firstSeenMs -AutoSize | Out-String -Width 200 | Write-Output
Write-Output "TCP connections and UDP endpoints owned by the tree ($($endpoints.Count) distinct):"
if ($endpoints.Count) {
  $endpoints | Format-Table process, pid, role, protocol, local, remote, state, scope, firstSeenMs, lastSeenMs, samples -AutoSize | Out-String -Width 240 | Write-Output
} else { Write-Output '  (none)'; Write-Output '' }
if ($netLogResult) {
  if ($netLogResult.present) {
    $list = { param($items) if ($items.Count) { $items -join '  ' } else { '(none)' } }
    Write-Output "NetLog ($($netLogResult.events) events from the Chromium network stack):"
    Write-Output ("  non-loopback TCP connect attempts:   " + (& $list $netLogResult.remoteTcp))
    Write-Output ("  non-loopback UDP with data sent:     " + (& $list $netLogResult.remoteUdpSent))
    Write-Output ("  non-loopback DNS lookups:            " + (& $list $netLogResult.remoteDns))
    Write-Output ("  non-loopback URLs:                   " + (& $list $netLogResult.remoteUrls))
    Write-Output ("  proxy auto-detection (WPAD) ran:     " + $netLogResult.proxyAutoDetect)
    Write-Output ("  UDP connect with no data (no packet; Chromium's IPv6 route probe): " + (& $list $netLogResult.remoteUdpConnectOnly))
    Write-Output ("  loopback TCP connect attempts:       " + (& $list $netLogResult.loopbackTcp))
    Write-Output ("  local URLs:                          " + (& $list $netLogResult.localUrls))
  } else { Write-Output 'NetLog: requested, but no log file was written' }
}
Write-Output ("DNS cache names new since start (system-wide, advisory): " + $(if ($null -eq $dnsNew) { '(cache not readable)' } elseif ($dnsNew.Count) { $dnsNew -join ' ' } else { "(none; cache held $($dnsBefore.Count) names before, $($dnsAfter.Count) after)" }))
if ($forced.Count) { Write-Output "note: had to force-kill pids $($forced -join ', ') after the close request" }
Write-Output "verdict: $verdict"

if ($Out) {
  $report | ConvertTo-Json -Depth 6 | Out-File -FilePath $Out -Encoding utf8
  Write-Output "wrote $Out"
}

if ($earlyExit -or $netLogMissing) { exit 2 }
if ($remoteRows.Count -or $netLogRemote) { exit 1 }
exit 0
