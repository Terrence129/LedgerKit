param(
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [int]$ColdStarts = 30,
    [int]$IdleSeconds = 300
)

$ErrorActionPreference = 'Stop'
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
if ($ColdStarts -lt 1) { throw 'ColdStarts must be positive.' }
if ($IdleSeconds -lt 1) { throw 'IdleSeconds must be positive.' }
if (Get-Process ledgerkit-desktop -ErrorAction SilentlyContinue) { throw 'Close every running LedgerKit process before measuring.' }

$localDataRoot = Join-Path $env:LOCALAPPDATA 'com.ledgerkit.desktop'
$configRoot = Join-Path $env:APPDATA 'com.ledgerkit.desktop'
foreach ($root in @($localDataRoot, $configRoot)) {
    if (Test-Path -LiteralPath (Join-Path $root 'ledger.sqlite3')) {
        throw 'Refusing to run the synthetic runtime measurement against an existing LedgerKit database.'
    }
}

function Get-ProcessTree([int]$RootId) {
    $all = @(Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name)
    $ids = [Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add($RootId)
    do {
        $changed = $false
        foreach ($item in $all) {
            if ($ids.Contains([int]$item.ParentProcessId) -and $ids.Add([int]$item.ProcessId)) { $changed = $true }
        }
    } while ($changed)
    return @($all | Where-Object { $ids.Contains([int]$_.ProcessId) })
}

function Wait-ForWindow([Diagnostics.Process]$Process, [int]$TimeoutSeconds = 20) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    do {
        Start-Sleep -Milliseconds 20
        $Process.Refresh()
        $treeCount = if ($Process.HasExited) { 0 } else { @(Get-ProcessTree -RootId $Process.Id).Count }
        $ready = -not $Process.HasExited -and $Process.MainWindowHandle -ne 0 -and $Process.Responding -and $treeCount -ge 7
    } while (-not $ready -and $timer.Elapsed.TotalSeconds -lt $TimeoutSeconds)
    if (-not $ready) { throw 'LedgerKit did not expose a responsive main window and complete WebView tree.' }
    return [math]::Round($timer.Elapsed.TotalMilliseconds, 3)
}

function Stop-Normally([Diagnostics.Process]$Process) {
    $tree = @(Get-ProcessTree -RootId $Process.Id)
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    $lastHandle = 0
    $lastClose = $false
    do {
        $Process.Refresh()
        if ($Process.HasExited) { break }
        $lastHandle = $Process.MainWindowHandle
        $lastClose = $Process.CloseMainWindow()
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    $Process.Refresh()
    if (-not $Process.HasExited) { throw "LedgerKit did not exit within ten seconds (handle=$lastHandle close=$lastClose)." }
    return @($tree.ProcessId)
}

function Percentile95([double[]]$Samples) {
    $sorted = @($Samples | Sort-Object)
    $index = [math]::Max(0, [math]::Ceiling($sorted.Count * 0.95) - 1)
    return [math]::Round($sorted[$index], 3)
}

function Get-ForegroundProcessId {
    $handle = [LedgerKit.Measurement.Window]::GetForegroundWindow()
    [uint32]$owner = 0
    if ($handle -ne [IntPtr]::Zero) {
        [void][LedgerKit.Measurement.Window]::GetWindowThreadProcessId($handle, [ref]$owner)
    }
    return [int]$owner
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace LedgerKit.Measurement {
    public static class Window {
        public delegate bool EnumWindowsProc(IntPtr handle, IntPtr parameter);
        [DllImport("user32.dll")]
        public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr parameter);
        [DllImport("user32.dll")]
        public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);
        [DllImport("user32.dll")]
        public static extern bool ShowWindow(IntPtr handle, int command);
        [DllImport("user32.dll")]
        public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")]
        public static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")]
        public static extern bool SetForegroundWindow(IntPtr handle);
        public static int ShowProcessWindows(int processId, int command) {
            int count = 0;
            EnumWindows((handle, parameter) => {
                GetWindowThreadProcessId(handle, out uint owner);
                if (owner == processId) { ShowWindow(handle, command); count++; }
                return true;
            }, IntPtr.Zero);
            return count;
        }
        public static int CloseProcessWindows(int processId) {
            int count = 0;
            EnumWindows((handle, parameter) => {
                GetWindowThreadProcessId(handle, out uint owner);
                if (owner == processId && PostMessage(handle, 0x10, IntPtr.Zero, IntPtr.Zero)) { count++; }
                return true;
            }, IntPtr.Zero);
            return count;
        }
    }
}
'@

$previousForeground = [LedgerKit.Measurement.Window]::GetForegroundWindow()
[uint32]$returnFocusProcessId = 0
if ($previousForeground -ne [IntPtr]::Zero) {
    [void][LedgerKit.Measurement.Window]::GetWindowThreadProcessId($previousForeground, [ref]$returnFocusProcessId)
}
if ($returnFocusProcessId -eq 0) {
    $returnFocusProcessId = [uint32]((Get-Process | Where-Object { $_.MainWindowHandle -ne 0 -and $_.ProcessName -ne 'ledgerkit-desktop' } | Select-Object -First 1).Id)
}
$codexHost = Get-Process ChatGPT -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if ($codexHost) { $returnFocusProcessId = [uint32]$codexHost.Id }
$startup = [Collections.Generic.List[double]]::new()
$normalExitTreeIds = @()
for ($index = 0; $index -lt $ColdStarts; $index++) {
    $process = Start-Process -FilePath $executablePath -PassThru
    try {
        $startup.Add((Wait-ForWindow -Process $process))
        Start-Sleep -Milliseconds 250
        $closedIds = @(Stop-Normally -Process $process)
        if ($index -eq $ColdStarts - 1) { $normalExitTreeIds = $closedIds }
    }
    finally {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    }
}
Start-Sleep -Seconds 10
$normalExitResidual = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $normalExitTreeIds -contains $_.Id } | Select-Object Id,ProcessName)

$idleProcess = Start-Process -FilePath $executablePath -PassThru
$rssSamples = [Collections.Generic.List[object]]::new()
$processCountSamples = [Collections.Generic.List[double]]::new()
$network = [Collections.Generic.HashSet[string]]::new()
$observedTreeIds = [Collections.Generic.HashSet[int]]::new()
$residual = @()
$idleCleanupForced = $false
$ledgerKitForegroundSamples = 0
try {
    [void](Wait-ForWindow -Process $idleProcess)
    Start-Sleep -Seconds 3
    $automationShell = New-Object -ComObject WScript.Shell
    $focusMoved = $false
    for ($attempt = 0; $attempt -lt 20 -and -not $focusMoved; $attempt++) {
        if ($returnFocusProcessId -ne 0) { [void]$automationShell.AppActivate([int]$returnFocusProcessId) }
        Start-Sleep -Milliseconds 250
        $focusMoved = (Get-ForegroundProcessId) -eq $returnFocusProcessId
    }
    if (-not $focusMoved -and $previousForeground -ne [IntPtr]::Zero) {
        [void][LedgerKit.Measurement.Window]::SetForegroundWindow($previousForeground)
        Start-Sleep -Milliseconds 500
        $focusMoved = (Get-ForegroundProcessId) -notin @($idleProcess.Id, 0)
    }
    if (-not $focusMoved) {
        throw 'The measurement host could not move focus away from LedgerKit.'
    }
    Start-Sleep -Milliseconds 500
    $idleTimer = [Diagnostics.Stopwatch]::StartNew()
    while ($idleTimer.Elapsed.TotalSeconds -lt $IdleSeconds) {
        $tree = @(Get-ProcessTree -RootId $idleProcess.Id)
        $rss = 0L
        foreach ($item in $tree) {
            [void]$observedTreeIds.Add([int]$item.ProcessId)
            $live = Get-Process -Id $item.ProcessId -ErrorAction SilentlyContinue
            if ($live) { $rss += $live.WorkingSet64 }
        }
        $rssSamples.Add([pscustomobject]@{ at_ms = $idleTimer.Elapsed.TotalMilliseconds; rss_bytes = $rss })
        $processCountSamples.Add($tree.Count)
        $foregroundProcessId = Get-ForegroundProcessId
        if ($tree.ProcessId -contains $foregroundProcessId) { $ledgerKitForegroundSamples++ }
        $connections = Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue | Where-Object {
            $observedTreeIds.Contains([int]$_.OwningProcess) -and $_.RemoteAddress -notin @('127.0.0.1','::1')
        }
        foreach ($connection in $connections) {
            [void]$network.Add("$($connection.RemoteAddress):$($connection.RemotePort)")
        }
        Start-Sleep -Milliseconds 500
    }
    $processTreeAtIdleEnd = @(Get-ProcessTree -RootId $idleProcess.Id | ForEach-Object {
        $live = Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
        if ($live) { [pscustomobject]@{ process = $live.ProcessName; working_set_bytes = [int64]$live.WorkingSet64 } }
    })
    $closedTreeIds = @(Stop-Normally -Process $idleProcess)
    foreach ($processId in $closedTreeIds) { [void]$observedTreeIds.Add([int]$processId) }
    Start-Sleep -Seconds 10
    $residual = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $observedTreeIds.Contains([int]$_.Id) } | Select-Object Id,ProcessName)
}
finally {
    if (-not $idleProcess.HasExited) { Stop-Process -Id $idleProcess.Id -Force }
}

$steadyCutoffMs = [math]::Max(0, $IdleSeconds * 1000 - 30000)
$steadyRss = @($rssSamples | Where-Object { $_.at_ms -ge $steadyCutoffMs } | ForEach-Object { [double]$_.rss_bytes })
$allRss = @($rssSamples | ForEach-Object { [double]$_.rss_bytes })
$steadyP95Bytes = Percentile95 -Samples $steadyRss
$result = [ordered]@{
    executable = Split-Path -Leaf $executablePath
    cold_start_samples = $startup.Count
    cold_start_p95_ms = Percentile95 -Samples $startup.ToArray()
    cold_start_max_ms = [math]::Round(($startup | Measure-Object -Maximum).Maximum, 3)
    idle_seconds = $IdleSeconds
    idle_rss_samples = $rssSamples.Count
    final_30s_rss_samples = $steadyRss.Count
    full_tree_idle_final_30s_rss_p95_bytes = [int64]$steadyP95Bytes
    full_tree_idle_final_30s_rss_p95_mib = [math]::Round($steadyP95Bytes / 1MB, 3)
    full_tree_peak_rss_bytes = [int64](($allRss | Measure-Object -Maximum).Maximum)
    full_tree_min_rss_bytes = [int64](($allRss | Measure-Object -Minimum).Minimum)
    full_tree_last_rss_bytes = [int64]$allRss[-1]
    full_tree_process_count_max = [int](($processCountSamples | Measure-Object -Maximum).Maximum)
    process_tree_at_idle_end = $processTreeAtIdleEnd
    ledgerkit_foreground_samples = $ledgerKitForegroundSamples
    return_focus_process_id = $returnFocusProcessId
    default_remote_network_endpoints = @($network | Sort-Object)
    normal_exit_residual_processes_after_10s = @($normalExitResidual)
    idle_cleanup_forced = $idleCleanupForced
    idle_residual_processes_after_10s = @($residual)
}

$result | ConvertTo-Json -Depth 4
if ($result.cold_start_p95_ms -gt 1500) { throw 'Cold-start P95 exceeds 1.5 seconds.' }
if ($result.full_tree_idle_final_30s_rss_p95_bytes -gt 150000000) { throw 'Full-tree final-30-second idle RSS P95 exceeds 150 MB.' }
if ($result.default_remote_network_endpoints.Count -ne 0) { throw 'Default runtime opened a remote network connection.' }
if ($result.ledgerkit_foreground_samples -ne 0) { throw 'LedgerKit regained foreground focus during the idle measurement.' }
if ($result.normal_exit_residual_processes_after_10s.Count -ne 0) { throw 'LedgerKit left residual processes after a normal exit.' }
if ($result.idle_residual_processes_after_10s.Count -ne 0) { throw 'LedgerKit left residual processes after idle cleanup.' }
Write-Output 'RUNTIME_MEASUREMENT=PASS'
