$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$appRoot = Join-Path $repositoryRoot 'app'

& cargo test --release --manifest-path (Join-Path $appRoot 'src-tauri/Cargo.toml') synthetic_100k_query_meets_latency_and_response_gates -- --ignored --nocapture
if ($LASTEXITCODE -ne 0) { throw '100k Core/SQLite performance gate failed.' }

& npm --prefix $appRoot test -- --run src/ui/OverviewPage.test.tsx --reporter=verbose
if ($LASTEXITCODE -ne 0) { throw 'Expense UI render performance gate failed.' }

Write-Output 'BETA_PERFORMANCE=PASS'
