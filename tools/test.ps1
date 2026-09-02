$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$appRoot = Join-Path $repositoryRoot 'app'

& npm --prefix $appRoot ci
if ($LASTEXITCODE -ne 0) { throw 'npm ci failed.' }
& (Join-Path $PSScriptRoot 'check-m0-fixtures.ps1')
& npm --prefix $appRoot test
if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed.' }
& cargo test --manifest-path (Join-Path $appRoot 'src-tauri/Cargo.toml') --all-targets --all-features
if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }

Write-Output 'TEST=PASS'
