param(
    [switch]$SkipRestore
)

$ErrorActionPreference = 'Stop'
$spikeRoot = Split-Path -Parent $PSScriptRoot
$repositoryRoot = (Resolve-Path -LiteralPath (Join-Path $spikeRoot '..\..')).Path
$solution = Join-Path $spikeRoot 'LedgerKit.AvaloniaSpike.slnx'

Push-Location $spikeRoot
try {
    if (-not $SkipRestore) {
        dotnet restore $solution --locked-mode
        if ($LASTEXITCODE -ne 0) { throw 'dotnet restore failed' }
    }
    dotnet format $solution --verify-no-changes --no-restore
    if ($LASTEXITCODE -ne 0) { throw 'dotnet format verification failed' }
    dotnet build $solution -c Release --no-restore
    if ($LASTEXITCODE -ne 0) { throw 'dotnet build failed' }
    dotnet run --project 'checks\LedgerKit.AvaloniaSpike.Checks\LedgerKit.AvaloniaSpike.Checks.csproj' -c Release --no-build
    if ($LASTEXITCODE -ne 0) { throw 'Avalonia spike checks failed' }
    pwsh -NoProfile -File (Join-Path $repositoryRoot 'tools\check-m0-fixtures.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'M0 fixture validation failed' }
}
finally {
    Pop-Location
}
