$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$appRoot = Join-Path $repositoryRoot 'app'

function Invoke-Checked([scriptblock]$Command, [string]$Failure) {
    & $Command
    if ($LASTEXITCODE -ne 0) { throw $Failure }
}

$nodeVersion = (& node --version).TrimStart('v')
$npmVersion = (& npm --version).Trim()
$rustVersion = ((& rustc --version) -split ' ')[1]
if ($nodeVersion -ne '24.16.0') { throw "Node 24.16.0 is required; found $nodeVersion" }
if ($npmVersion -ne '11.13.0') { throw "npm 11.13.0 is required; found $npmVersion" }
if ($rustVersion -ne '1.98.0') { throw "Rust 1.98.0 is required; found $rustVersion" }

Invoke-Checked { npm --prefix $appRoot ci } 'npm ci failed.'
& (Join-Path $PSScriptRoot 'check-m0-fixtures.ps1')
Invoke-Checked { npm --prefix $appRoot run check } 'Frontend build or tests failed.'
Invoke-Checked { cargo fmt --manifest-path (Join-Path $appRoot 'src-tauri/Cargo.toml') --all -- --check } 'rustfmt check failed.'
Invoke-Checked { cargo test --release --manifest-path (Join-Path $appRoot 'src-tauri/Cargo.toml') --all-targets --all-features -- --include-ignored --nocapture } 'Rust tests or the 100k performance gate failed.'
Invoke-Checked { cargo clippy --release --manifest-path (Join-Path $appRoot 'src-tauri/Cargo.toml') --all-targets --all-features --no-deps -- -D warnings } 'Clippy failed.'
& (Join-Path $PSScriptRoot 'check-m3-fixtures.ps1')
& (Join-Path $PSScriptRoot 'check-m5-fixtures.ps1')
Invoke-Checked { node (Join-Path $PSScriptRoot 'check-m1-scaffold.mjs') } 'M1 scaffold contract check failed.'
Invoke-Checked { node (Join-Path $PSScriptRoot 'check-beta.mjs') } 'Beta release contract check failed.'
& (Join-Path $PSScriptRoot 'check-privacy.ps1')
Invoke-Checked { git -C $repositoryRoot diff --check } 'git diff --check failed.'

Write-Output 'CHECK=PASS'
