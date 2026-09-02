param(
    [string]$OutputRoot,
    [switch]$AttemptNativeAot
)

$ErrorActionPreference = 'Stop'
$spikeRoot = Split-Path -Parent $PSScriptRoot
$project = Join-Path $spikeRoot 'src\LedgerKit.AvaloniaSpike\LedgerKit.AvaloniaSpike.csproj'
$solution = Join-Path $spikeRoot 'LedgerKit.AvaloniaSpike.slnx'
if (-not $OutputRoot) {
    $OutputRoot = Join-Path $spikeRoot ("artifacts\run-{0}" -f [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
}
$OutputRoot = [IO.Path]::GetFullPath($OutputRoot)
$frameworkOutput = Join-Path $OutputRoot 'framework-dependent'
$trimmedOutput = Join-Path $OutputRoot 'self-contained-trimmed'
$installerOutput = Join-Path $OutputRoot 'LedgerKit-Avalonia-Spike-thin-setup.exe'
New-Item -ItemType Directory -Path $frameworkOutput -Force | Out-Null
New-Item -ItemType Directory -Path $trimmedOutput -Force | Out-Null

dotnet publish $project -c Release -r win-x64 --self-contained false --no-restore -o $frameworkOutput `
    -p:DebugType=None -p:DebugSymbols=false
if ($LASTEXITCODE -ne 0) { throw 'Framework-dependent publish failed' }

dotnet publish $project -c Release -r win-x64 --self-contained true --no-restore -o $trimmedOutput `
    -p:PublishTrimmed=true -p:TrimMode=partial -p:DebugType=None -p:DebugSymbols=false
if ($LASTEXITCODE -ne 0) { throw 'Self-contained trimmed publish failed' }

$nsisCandidates = @(
    (Get-Command makensis.exe -ErrorAction SilentlyContinue).Source,
    (Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe'),
    (Join-Path $env:LOCALAPPDATA 'tauri\NSIS\Bin\makensis.exe')
)
$makeNsis = $nsisCandidates | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
if (-not $makeNsis) { throw 'NSIS makensis.exe was not found' }
& $makeNsis "/DAPP_SOURCE=$frameworkOutput" "/DOUT_FILE=$installerOutput" (Join-Path $spikeRoot 'packaging\installer.nsi')
if ($LASTEXITCODE -ne 0) { throw 'NSIS package build failed' }

$nativeAot = [ordered]@{
    attempted = $false
    succeeded = $false
    output = $null
    output_bytes = $null
    installer = $null
    installer_bytes = $null
    installer_sha256 = $null
    log = $null
}
if ($AttemptNativeAot) {
    $nativeAot.attempted = $true
    $aotOutput = Join-Path $OutputRoot 'native-aot-candidate'
    $aotLog = Join-Path $OutputRoot 'native-aot.log'
    New-Item -ItemType Directory -Path $aotOutput -Force | Out-Null
    dotnet restore $project -r win-x64 -p:PublishAot=true --force-evaluate 2>&1 |
        Tee-Object -FilePath $aotLog
    $aotExitCode = $LASTEXITCODE
    if ($aotExitCode -eq 0) {
        dotnet publish $project -c Release -r win-x64 --self-contained true --no-restore -o $aotOutput `
            -p:PublishAot=true -p:StripSymbols=true -p:DebugType=None -p:DebugSymbols=false 2>&1 |
            Tee-Object -FilePath $aotLog -Append
        $aotExitCode = $LASTEXITCODE
    }
    $nativeAot.succeeded = $aotExitCode -eq 0
    $nativeAot.output = $aotOutput
    $nativeAot.log = $aotLog
    if ($nativeAot.succeeded) {
        $aotInstaller = Join-Path $OutputRoot 'LedgerKit-Avalonia-Spike-aot-setup.exe'
        & $makeNsis "/DAPP_SOURCE=$aotOutput" "/DOUT_FILE=$aotInstaller" (Join-Path $spikeRoot 'packaging\installer.nsi')
        if ($LASTEXITCODE -ne 0) { throw 'Native AOT NSIS package build failed' }
        $nativeAot.output_bytes =
            (Get-ChildItem -LiteralPath $aotOutput -File -Recurse | Measure-Object Length -Sum).Sum
        $nativeAot.installer = $aotInstaller
        $nativeAot.installer_bytes = (Get-Item -LiteralPath $aotInstaller).Length
        $nativeAot.installer_sha256 = (Get-FileHash -LiteralPath $aotInstaller -Algorithm SHA256).Hash
    }
    dotnet restore $solution --force-evaluate
    if ($LASTEXITCODE -ne 0) { throw 'Failed to restore standard package lock state after Native AOT attempt' }
}

$result = [ordered]@{
    output_root = $OutputRoot
    framework_dependent = $frameworkOutput
    framework_dependent_bytes = (Get-ChildItem -LiteralPath $frameworkOutput -File -Recurse | Measure-Object Length -Sum).Sum
    self_contained_trimmed = $trimmedOutput
    self_contained_trimmed_bytes = (Get-ChildItem -LiteralPath $trimmedOutput -File -Recurse | Measure-Object Length -Sum).Sum
    thin_installer = $installerOutput
    thin_installer_bytes = (Get-Item -LiteralPath $installerOutput).Length
    thin_installer_sha256 = (Get-FileHash -LiteralPath $installerOutput -Algorithm SHA256).Hash
    native_aot = $nativeAot
}
$result | ConvertTo-Json -Depth 4
