$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$appRoot = Join-Path $repositoryRoot 'app'

& npm --prefix $appRoot ci
if ($LASTEXITCODE -ne 0) { throw 'npm ci failed.' }
& npm --prefix $appRoot run tauri:build
if ($LASTEXITCODE -ne 0) { throw 'LedgerKit production package build failed.' }

$cargoTargetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $appRoot 'src-tauri/target' }
$bundleRoot = Join-Path $cargoTargetRoot 'release/bundle/nsis'
$installer = Get-ChildItem -LiteralPath $bundleRoot -Filter '*-setup.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $installer) { throw 'NSIS installer was not produced.' }

[ordered]@{
    installer = $installer.FullName
    bytes = $installer.Length
    sha256 = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash
} | ConvertTo-Json
Write-Output 'BUILD=PASS'
