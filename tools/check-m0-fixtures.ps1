$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$fixturesRoot = Join-Path $repositoryRoot 'fixtures/sanitized'
$schemasRoot = Join-Path $fixturesRoot 'schemas'
$schemaByFile = @{
    'metadata.json' = 'metadata.schema.json'
    'input.json' = 'input.schema.json'
    'normalized-events.json' = 'normalized-events.schema.json'
    'expected-postings.json' = 'expected-postings.schema.json'
    'expected-projection.json' = 'expected-projection.schema.json'
    'expected-errors.json' = 'expected-errors.schema.json'
}

$acceptedAdrs = @(
    'ADR-0002-modular-monolith-local-sqlite.md'
    'ADR-0003-typed-events-postings-projections.md'
    'ADR-0004-decimal-rounding-contract.md'
    'ADR-0005-moving-weighted-average.md'
    'ADR-0006-revision-reversal-semantics.md'
    'ADR-0011-history-cutover-migration.md'
    'ADR-0012-market-data-revisions-as-of.md'
    'ADR-0014-expense-analysis-contract.md'
)

$documentationRoot = Join-Path $repositoryRoot 'docs'
$validatedAdrCount = 0
if (Test-Path -LiteralPath $documentationRoot -PathType Container) {
    $adrRoot = Join-Path $documentationRoot 'adr'
    foreach ($adrName in $acceptedAdrs) {
        $adrPath = Join-Path $adrRoot $adrName
        $adrText = Get-Content -LiteralPath $adrPath -Raw
        if ($adrText -notmatch '(?m)^> 状态：Accepted$' -or
            $adrText -notmatch '(?m)^> 决策者：项目所有者$' -or
            $adrText -notmatch '(?m)^> 授权：项目所有者') {
            throw "Accepted ADR metadata is incomplete: $adrPath"
        }
    }

    $adrIndexText = Get-Content -LiteralPath (Join-Path $adrRoot 'README.md') -Raw
    foreach ($adrNumber in @('0002', '0003', '0004', '0005', '0006', '0011', '0012', '0014')) {
        if ($adrIndexText -notmatch "(?m)^\| \[ADR-$adrNumber\]\([^\r\n]+\) \|[^\r\n]+\| Accepted \|$") {
            throw "ADR index does not mark ADR-$adrNumber as linked and Accepted."
        }
    }

    $financialRulesText = Get-Content -LiteralPath (Join-Path $documentationRoot 'financial-rules.md') -Raw
    if ($financialRulesText -match '（Proposed）') {
        throw 'financial-rules.md still contains Proposed financial rules after the M0 decision baseline.'
    }

    $agentContextText = Get-Content -LiteralPath (Join-Path $documentationRoot 'agent-context.md') -Raw
    if ($agentContextText -notmatch '(?m)^> M0 状态：完成$') {
        throw 'agent-context.md does not retain the completed M0 milestone.'
    }
    $validatedAdrCount = $acceptedAdrs.Count
} else {
    Write-Output 'M0_DOCUMENTATION_CHECK=SKIPPED local documentation is not published'
}

$fixtureDirectories = @(Get-ChildItem -LiteralPath $fixturesRoot -Directory | Where-Object Name -Match '^[0-9]{2}-' | Sort-Object Name)
if ($fixtureDirectories.Count -ne 31) {
    throw "Expected 31 numbered fixture groups; found $($fixtureDirectories.Count)."
}

$validated = 0
foreach ($directory in $fixtureDirectories) {
    foreach ($entry in $schemaByFile.GetEnumerator()) {
        $jsonPath = Join-Path $directory.FullName $entry.Key
        $schemaPath = Join-Path $schemasRoot $entry.Value
        if (-not (Test-Path -LiteralPath $jsonPath -PathType Leaf)) {
            throw "Missing fixture file: $jsonPath"
        }
        if (-not (Test-Json -LiteralPath $jsonPath -SchemaFile $schemaPath -ErrorAction Stop)) {
            throw "JSON Schema validation failed: $jsonPath"
        }
        $validated += 1
    }
}

& node (Join-Path $PSScriptRoot 'generate-m0-fixtures.mjs') --check
if ($LASTEXITCODE -ne 0) {
    throw 'Generated fixtures do not match the deterministic generator.'
}

& node (Join-Path $PSScriptRoot 'validate-m0-fixtures.mjs')
if ($LASTEXITCODE -ne 0) {
    throw 'M0 fixture semantic validation failed.'
}

Write-Output "Validated $validated JSON files against schemas."
Write-Output "Validated $validatedAdrCount M0 Accepted ADRs and the retained M0 milestone when local documentation is available."
