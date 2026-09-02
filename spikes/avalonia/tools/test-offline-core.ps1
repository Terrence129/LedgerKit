param(
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [int]$ObserveSeconds = 10
)

$ErrorActionPreference = 'Stop'
$resolvedExecutable = (Resolve-Path -LiteralPath $Executable).Path
$testRoot = Join-Path ([IO.Path]::GetTempPath()) (
    "ledgerkit-m1-avalonia-offline-{0}-{1}" -f $PID, [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
$readyFile = Join-Path $testRoot 'ready.json'
$process = $null
$previousEnvironment = @{}
$environmentNames = @(
    'LEDGERKIT_SPIKE_DATA_DIR',
    'LEDGERKIT_SPIKE_READY_FILE',
    'LEDGERKIT_SPIKE_AUTOMATION_HIDDEN',
    'HTTP_PROXY',
    'HTTPS_PROXY',
    'ALL_PROXY',
    'NO_PROXY')
foreach ($name in $environmentNames) {
    $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}

try {
    $env:LEDGERKIT_SPIKE_DATA_DIR = Join-Path $testRoot 'data'
    $env:LEDGERKIT_SPIKE_READY_FILE = $readyFile
    $env:LEDGERKIT_SPIKE_AUTOMATION_HIDDEN = '1'
    $env:HTTP_PROXY = 'http://127.0.0.1:1'
    $env:HTTPS_PROXY = 'http://127.0.0.1:1'
    $env:ALL_PROXY = 'http://127.0.0.1:1'
    $env:NO_PROXY = ''

    $timer = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $resolvedExecutable -WindowStyle Hidden -PassThru
    while (-not (Test-Path -LiteralPath $readyFile) -and $timer.Elapsed.TotalSeconds -lt 15 -and -not $process.HasExited) {
        Start-Sleep -Milliseconds 20
        $process.Refresh()
    }
    if (-not (Test-Path -LiteralPath $readyFile)) { throw 'Offline launch did not report ready' }
    $readyMs = $timer.Elapsed.TotalMilliseconds

    $endpoints = [Collections.Generic.HashSet[string]]::new()
    $observe = [Diagnostics.Stopwatch]::StartNew()
    while ($observe.Elapsed.TotalSeconds -lt $ObserveSeconds) {
        if ($process.HasExited) { throw 'Offline launch exited during observation' }
        foreach ($connection in Get-NetTCPConnection -OwningProcess $process.Id -State Established -ErrorAction SilentlyContinue) {
            [void]$endpoints.Add("$($connection.RemoteAddress):$($connection.RemotePort)")
        }
        Start-Sleep -Milliseconds 100
        $process.Refresh()
    }

    Stop-Process -Id $process.Id -Force
    [void]$process.WaitForExit(10000)
    Start-Sleep -Seconds 10
    $residual = @(Get-Process -Id $process.Id -ErrorAction SilentlyContinue).Count
    [ordered]@{
        ready = $true
        ready_ms = [Math]::Round($readyMs, 3)
        dead_proxy = '127.0.0.1:1'
        observed_seconds = $ObserveSeconds
        established_remote_endpoints = @($endpoints)
        residual_process_count_after_10s = $residual
    } | ConvertTo-Json
}
finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
    foreach ($name in $environmentNames) {
        [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], 'Process')
    }
    $resolvedTestRoot = (Resolve-Path -LiteralPath $testRoot -ErrorAction SilentlyContinue).Path
    if ($resolvedTestRoot -and $resolvedTestRoot.StartsWith([IO.Path]::GetTempPath(), [StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
    }
}
