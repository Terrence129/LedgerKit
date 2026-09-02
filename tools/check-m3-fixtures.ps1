$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $repositoryRoot 'app/src-tauri/Cargo.toml'
$committedRoot = Join-Path $repositoryRoot 'fixtures/sanitized/m3'
$temporaryBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporaryRoot = Join-Path $temporaryBase ("ledgerkit-m3-fixtures-" + [guid]::NewGuid().ToString('N'))
$firstRoot = Join-Path $temporaryRoot 'first'
$secondRoot = Join-Path $temporaryRoot 'second'

try {
    & cargo run --quiet --release --manifest-path $manifest --example generate_m3_fixtures -- $firstRoot
    if ($LASTEXITCODE -ne 0) { throw 'First M3 fixture generation failed.' }
    & cargo run --quiet --release --manifest-path $manifest --example generate_m3_fixtures -- $secondRoot
    if ($LASTEXITCODE -ne 0) { throw 'Second M3 fixture generation failed.' }

    $names = @('cash-import-valid.xlsx', 'cash-import-invalid.xlsx', 'cash-import-modified.xlsx')
    foreach ($name in $names) {
        $firstHash = (Get-FileHash -LiteralPath (Join-Path $firstRoot $name) -Algorithm SHA256).Hash
        $secondHash = (Get-FileHash -LiteralPath (Join-Path $secondRoot $name) -Algorithm SHA256).Hash
        $committedHash = (Get-FileHash -LiteralPath (Join-Path $committedRoot $name) -Algorithm SHA256).Hash
        if ($firstHash -ne $secondHash -or $firstHash -ne $committedHash) {
            throw "M3 fixture drift detected: $name"
        }
    }
    Write-Output 'M3_FIXTURE_CHECK=PASS files=3'
}
finally {
    $resolved = [IO.Path]::GetFullPath($temporaryRoot)
    if ($resolved.StartsWith($temporaryBase, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
