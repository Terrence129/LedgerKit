param(
    [Parameter(Mandatory = $true)]
    [string]$Installer
)

$ErrorActionPreference = 'Stop'
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$displayName = 'LedgerKit'
$uninstallRoots = @(
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
)

function Find-LedgerKitInstallation {
    foreach ($root in $uninstallRoots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        foreach ($entry in Get-ChildItem -LiteralPath $root) {
            $properties = Get-ItemProperty -LiteralPath $entry.PSPath
            if ($properties.DisplayName -eq $displayName) { return $properties }
        }
    }
    return $null
}

$existingInstallation = Find-LedgerKitInstallation
if ($existingInstallation) { throw 'Refusing to replace an existing LedgerKit installation during smoke testing.' }

$applicationProcess = $null
$installRoot = $null
$uninstall = $null
try {
    $install = Start-Process -FilePath $installerPath -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait
    if ($install.ExitCode -ne 0) { throw 'Silent install failed.' }
    $installed = Find-LedgerKitInstallation
    if (-not $installed -or -not $installed.InstallLocation) { throw 'Installed LedgerKit registration was not found.' }
    $installRoot = $installed.InstallLocation.Trim('"').TrimEnd('\')
    $application = Join-Path $installRoot 'ledgerkit-desktop.exe'
    if (-not (Test-Path -LiteralPath $application -PathType Leaf)) { throw 'Installed LedgerKit executable was not found.' }

    $applicationProcess = Start-Process -FilePath $application -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        Start-Sleep -Milliseconds 100
        $applicationProcess.Refresh()
    } while ($applicationProcess.MainWindowHandle -eq 0 -and [DateTime]::UtcNow -lt $deadline -and -not $applicationProcess.HasExited)
    if ($applicationProcess.HasExited -or $applicationProcess.MainWindowHandle -eq 0) { throw 'Installed LedgerKit did not show its main window.' }
    $closedNormally = $applicationProcess.CloseMainWindow()
    if (-not $closedNormally -or -not $applicationProcess.WaitForExit(10000)) { throw 'Installed LedgerKit did not close normally.' }
    $payloadBytes = (Get-ChildItem -LiteralPath $installRoot -File -Recurse | Measure-Object Length -Sum).Sum
}
finally {
    if ($applicationProcess -and -not $applicationProcess.HasExited) { Stop-Process -Id $applicationProcess.Id -Force }
    $installedForCleanup = Find-LedgerKitInstallation
    if ($installedForCleanup -and $installedForCleanup.InstallLocation) {
        $cleanupRoot = $installedForCleanup.InstallLocation.Trim('"').TrimEnd('\')
        $uninstaller = Join-Path $cleanupRoot 'uninstall.exe'
        if (Test-Path -LiteralPath $uninstaller -PathType Leaf) {
            $uninstall = Start-Process -FilePath $uninstaller -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait
        }
    }
}

if (-not $uninstall -or $uninstall.ExitCode -ne 0) { throw 'Silent uninstall failed.' }
Start-Sleep -Seconds 2
if (Test-Path -LiteralPath $installRoot) { throw 'Install directory remains after uninstall.' }

[ordered]@{
    install_exit_code = $install.ExitCode
    main_window_ready = $true
    close_main_window_returned = $closedNormally
    installed_payload_bytes = $payloadBytes
    uninstall_exit_code = $uninstall.ExitCode
    install_directory_removed = $true
} | ConvertTo-Json
Write-Output 'INSTALL_SMOKE=PASS'
