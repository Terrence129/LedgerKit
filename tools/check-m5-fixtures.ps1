$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $repositoryRoot 'app/src-tauri/Cargo.toml'
$committedRoot = Join-Path $repositoryRoot 'fixtures/sanitized/m5'
$temporaryBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporaryRoot = Join-Path $temporaryBase ("ledgerkit-m5-fixtures-" + [guid]::NewGuid().ToString('N'))
$firstRoot = Join-Path $temporaryRoot 'first'
$secondRoot = Join-Path $temporaryRoot 'second'

try {
    & cargo run --quiet --release --manifest-path $manifest --example generate_m5_fixtures -- $firstRoot
    if ($LASTEXITCODE -ne 0) { throw 'First M5 fixture generation failed.' }
    & cargo run --quiet --release --manifest-path $manifest --example generate_m5_fixtures -- $secondRoot
    if ($LASTEXITCODE -ne 0) { throw 'Second M5 fixture generation failed.' }

    $names = @('full-import-history.xlsx', 'full-import-cutover.xlsx', 'full-import-invalid.xlsx')
    foreach ($name in $names) {
        $firstHash = (Get-FileHash -LiteralPath (Join-Path $firstRoot $name) -Algorithm SHA256).Hash
        $secondHash = (Get-FileHash -LiteralPath (Join-Path $secondRoot $name) -Algorithm SHA256).Hash
        $committedHash = (Get-FileHash -LiteralPath (Join-Path $committedRoot $name) -Algorithm SHA256).Hash
        if ($firstHash -ne $secondHash -or $firstHash -ne $committedHash) {
            throw "M5 fixture drift detected: $name"
        }
    }
    Write-Output 'M5_FIXTURE_CHECK=PASS files=3'
}
finally {
    $resolved = [IO.Path]::GetFullPath($temporaryRoot)
    if ($resolved.StartsWith($temporaryBase, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
