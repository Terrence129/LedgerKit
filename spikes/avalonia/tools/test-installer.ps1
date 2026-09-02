param(
    [Parameter(Mandatory = $true)]
    [string]$Installer
)

$ErrorActionPreference = 'Stop'
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$installRoot = Join-Path $env:LOCALAPPDATA 'Programs\LedgerKit Avalonia Spike'
$application = Join-Path $installRoot 'ledgerkit-avalonia-spike.exe'
$uninstaller = Join-Path $installRoot 'Uninstall.exe'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ("ledgerkit-avalonia-install-smoke-{0}-{1}" -f $PID, [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
try {
    $install = Start-Process -FilePath $installerPath -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait
    if ($install.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $application)) { throw 'Silent install failed' }
    $env:LEDGERKIT_SPIKE_DATA_DIR = Join-Path $testRoot 'data'
    $env:LEDGERKIT_SPIKE_READY_FILE = Join-Path $testRoot 'ready.json'
    $applicationProcess = Start-Process -FilePath $application -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (-not (Test-Path -LiteralPath $env:LEDGERKIT_SPIKE_READY_FILE) -and [DateTime]::UtcNow -lt $deadline -and -not $applicationProcess.HasExited) {
        Start-Sleep -Milliseconds 50
        $applicationProcess.Refresh()
    }
    if (-not (Test-Path -LiteralPath $env:LEDGERKIT_SPIKE_READY_FILE)) { throw 'Installed app did not become ready' }
    $closedNormally = $applicationProcess.CloseMainWindow()
    if (-not $applicationProcess.WaitForExit(10000)) { Stop-Process -Id $applicationProcess.Id -Force; throw 'Installed app did not close' }
    if (-not $closedNormally) { throw 'CloseMainWindow returned false' }
    $payloadBytes = (Get-ChildItem -LiteralPath $installRoot -File -Recurse | Measure-Object Length -Sum).Sum
    $uninstall = Start-Process -FilePath $uninstaller -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait
    if ($uninstall.ExitCode -ne 0) { throw 'Silent uninstall failed' }
    Start-Sleep -Seconds 2
    if (Test-Path -LiteralPath $installRoot) { throw 'Install directory remains after uninstall' }
    [ordered]@{
        install_exit_code = $install.ExitCode
        ready = $true
        close_main_window_returned = $closedNormally
        installed_payload_bytes = $payloadBytes
        uninstall_exit_code = $uninstall.ExitCode
        install_directory_removed = $true
    } | ConvertTo-Json
}
finally {
    $env:LEDGERKIT_SPIKE_DATA_DIR = $null
    $env:LEDGERKIT_SPIKE_READY_FILE = $null
    if (Test-Path -LiteralPath $uninstaller) {
        Start-Process -FilePath $uninstaller -ArgumentList '/S' -WindowStyle Hidden -Wait
    }
    $resolvedTestRoot = (Resolve-Path -LiteralPath $testRoot -ErrorAction SilentlyContinue).Path
    if ($resolvedTestRoot -and $resolvedTestRoot.StartsWith([IO.Path]::GetTempPath(), [StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
    }
}
