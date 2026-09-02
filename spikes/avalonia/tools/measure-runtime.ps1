param(
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [int]$ColdRuns = 30,
    [int]$IdleSeconds = 300,
    [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
$resolvedExecutable = (Resolve-Path -LiteralPath $Executable).Path
$measurementRoot = Join-Path ([IO.Path]::GetTempPath()) ("ledgerkit-m1-avalonia-runtime-{0}-{1}" -f $PID, [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
$dataRoot = Join-Path $measurementRoot 'data'
New-Item -ItemType Directory -Path $dataRoot -Force | Out-Null

function Get-ProcessTreeIds {
    param([int]$RootId)
    $known = [Collections.Generic.HashSet[int]]::new()
    [void]$known.Add($RootId)
    do {
        $changed = $false
        foreach ($process in Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId) {
            if ($known.Contains([int]$process.ParentProcessId) -and $known.Add([int]$process.ProcessId)) {
                $changed = $true
            }
        }
    } while ($changed)
    return @($known)
}

function Get-TreeRssBytes {
    param([int]$RootId)
    $sum = 0L
    foreach ($id in Get-ProcessTreeIds -RootId $RootId) {
        $process = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($null -ne $process) { $sum += [int64]$process.WorkingSet64 }
    }
    return $sum
}

function Get-TreeSnapshot {
    param([int]$RootId)
    return @(Get-ProcessTreeIds -RootId $RootId | ForEach-Object {
        $process = Get-Process -Id $_ -ErrorAction SilentlyContinue
        if ($null -ne $process) {
            [ordered]@{
                process_id = $process.Id
                process_name = $process.ProcessName
                working_set_bytes = [int64]$process.WorkingSet64
            }
        }
    })
}

function Get-Percentile95 {
    param([double[]]$Values)
    $sorted = @($Values | Sort-Object)
    $index = [Math]::Max(0, [Math]::Ceiling($sorted.Count * 0.95) - 1)
    return [double]$sorted[$index]
}

$cold = @()
$frontend = @()
$residualAfterCold = @()
$env:LEDGERKIT_SPIKE_DATA_DIR = $dataRoot
$env:LEDGERKIT_SPIKE_AUTOCLOSE = '1'
$env:LEDGERKIT_SPIKE_AUTOMATION_HIDDEN = '1'
try {
    for ($run = 1; $run -le $ColdRuns; $run++) {
        $readyFile = Join-Path $measurementRoot ("ready-{0:D2}.json" -f $run)
        $env:LEDGERKIT_SPIKE_READY_FILE = $readyFile
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Process -FilePath $resolvedExecutable -WindowStyle Hidden -PassThru
        while (-not (Test-Path -LiteralPath $readyFile) -and $timer.Elapsed.TotalSeconds -lt 10 -and -not $process.HasExited) {
            Start-Sleep -Milliseconds 20
            $process.Refresh()
        }
        if (-not (Test-Path -LiteralPath $readyFile)) { throw "Cold run $run did not report ready" }
        $cold += [Math]::Round($timer.Elapsed.TotalMilliseconds, 3)
        $frontend += Get-Content -Raw -LiteralPath $readyFile | ConvertFrom-Json
        if (-not $process.WaitForExit(10000)) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            throw "Cold run $run did not close"
        }
        Start-Sleep -Milliseconds 250
        $residualAfterCold += @(Get-ProcessTreeIds -RootId $process.Id | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }).Count
    }

    $env:LEDGERKIT_SPIKE_AUTOCLOSE = $null
    $idleReadyFile = Join-Path $measurementRoot 'idle-ready.json'
    $env:LEDGERKIT_SPIKE_READY_FILE = $idleReadyFile
    $idleProcess = Start-Process -FilePath $resolvedExecutable -WindowStyle Hidden -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath $idleReadyFile) -and [DateTime]::UtcNow -lt $deadline -and -not $idleProcess.HasExited) {
        Start-Sleep -Milliseconds 20
        $idleProcess.Refresh()
    }
    if (-not (Test-Path -LiteralPath $idleReadyFile)) { throw 'Idle run did not report ready' }

    $rssSamples = @()
    $networkEndpoints = [Collections.Generic.HashSet[string]]::new()
    $idleTimer = [Diagnostics.Stopwatch]::StartNew()
    while ($idleTimer.Elapsed.TotalSeconds -lt $IdleSeconds) {
        if ($idleProcess.HasExited) { throw "Idle process exited at $($idleTimer.Elapsed.TotalSeconds) seconds" }
        $rssSamples += [ordered]@{
            at_ms = [Math]::Round($idleTimer.Elapsed.TotalMilliseconds, 3)
            rss_bytes = Get-TreeRssBytes -RootId $idleProcess.Id
        }
        try {
            $ids = Get-ProcessTreeIds -RootId $idleProcess.Id
            foreach ($connection in Get-NetTCPConnection -State Established -ErrorAction Stop | Where-Object { $ids -contains $_.OwningProcess }) {
                $owner = Get-Process -Id $connection.OwningProcess -ErrorAction SilentlyContinue
                [void]$networkEndpoints.Add("$($connection.OwningProcess):$($owner.ProcessName)->$($connection.RemoteAddress):$($connection.RemotePort)")
            }
        }
        catch {
            [void]$networkEndpoints.Add("MEASUREMENT_ERROR:$($_.Exception.GetType().Name)")
        }
        Start-Sleep -Milliseconds 500
        $idleProcess.Refresh()
    }

    $actualIdleMs = $idleTimer.Elapsed.TotalMilliseconds
    $steadyCutoffMs = [Math]::Max(0, $actualIdleMs - 30000)
    $steadySamples = @($rssSamples | Where-Object { $_.at_ms -ge $steadyCutoffMs } | ForEach-Object { $_.rss_bytes })
    $processTreeAtEnd = Get-TreeSnapshot -RootId $idleProcess.Id
    $closedNormally = $idleProcess.CloseMainWindow()
    if (-not $idleProcess.WaitForExit(10000)) { Stop-Process -Id $idleProcess.Id -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 10
    $residualAfterClose = @(Get-ProcessTreeIds -RootId $idleProcess.Id | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }).Count

    $result = [ordered]@{
        executable = $resolvedExecutable
        cold_start_ms_raw = $cold
        cold_start_p95_ms = Get-Percentile95 -Values $cold
        frontend_first_render_ms_raw = @($frontend | ForEach-Object { $_.firstRenderMs })
        frontend_expense_render_ms_raw = @($frontend | ForEach-Object { $_.expenseRenderMs })
        cold_residual_process_counts = $residualAfterCold
        requested_idle_seconds = $IdleSeconds
        actual_idle_ms = [Math]::Round($actualIdleMs, 3)
        rss_sample_count = $rssSamples.Count
        rss_samples_raw = $rssSamples
        rss_bytes_raw = @($rssSamples | ForEach-Object { $_.rss_bytes })
        idle_final_30s_rss_p95_bytes = Get-Percentile95 -Values $steadySamples
        peak_tree_rss_bytes = ($rssSamples | ForEach-Object { $_.rss_bytes } | Measure-Object -Maximum).Maximum
        process_tree_at_idle_end = $processTreeAtEnd
        established_remote_endpoints = @($networkEndpoints)
        close_main_window_returned = $closedNormally
        residual_process_count_after_10s = $residualAfterClose
    }
    $json = $result | ConvertTo-Json -Depth 6
    $json
    if ($OutputPath) {
        $outputFullPath = [IO.Path]::GetFullPath($OutputPath)
        $outputDirectory = Split-Path -Parent $outputFullPath
        if ($outputDirectory) { New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null }
        [IO.File]::WriteAllText($outputFullPath, $json, [Text.UTF8Encoding]::new($false))
    }
}
finally {
    $env:LEDGERKIT_SPIKE_READY_FILE = $null
    $env:LEDGERKIT_SPIKE_DATA_DIR = $null
    $env:LEDGERKIT_SPIKE_AUTOCLOSE = $null
    $env:LEDGERKIT_SPIKE_AUTOMATION_HIDDEN = $null
    $resolvedMeasurementRoot = (Resolve-Path -LiteralPath $measurementRoot -ErrorAction SilentlyContinue).Path
    if ($resolvedMeasurementRoot -and $resolvedMeasurementRoot.StartsWith([IO.Path]::GetTempPath(), [StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $resolvedMeasurementRoot -Recurse -Force
    }
}
