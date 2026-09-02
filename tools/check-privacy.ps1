$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$candidateFiles = @(git -C $repositoryRoot ls-files --cached --others --exclude-standard)
if ($LASTEXITCODE -ne 0) { throw 'Unable to enumerate repository files for privacy scan.' }

$sensitiveExtensions = @('.db', '.sqlite', '.sqlite3', '.ledgerkit-backup', '.xls', '.xlsm', '.xlsb', '.pfx', '.pem', '.key')
$textExtensions = @('.cs', '.css', '.html', '.js', '.json', '.md', '.mjs', '.ps1', '.rs', '.toml', '.ts', '.tsx', '.xml', '.yml', '.yaml')
$violations = [System.Collections.Generic.List[string]]::new()

foreach ($relativePath in $candidateFiles) {
    $normalized = $relativePath.Replace('\', '/')
    $extension = [IO.Path]::GetExtension($relativePath).ToLowerInvariant()
    if ($sensitiveExtensions -contains $extension) {
        $violations.Add("sensitive file type: $normalized")
    }
    if ($extension -eq '.xlsx' -and -not $normalized.StartsWith('fixtures/sanitized/', [StringComparison]::Ordinal)) {
        $violations.Add("workbook outside sanitized fixtures: $normalized")
    }
    if ($textExtensions -notcontains $extension) { continue }
    $path = Join-Path $repositoryRoot $relativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { continue }
    $text = Get-Content -LiteralPath $path -Raw
    $windowsUsers = 'Us' + 'ers'
    $unixHome = 'ho' + 'me'
    $patterns = [ordered]@{
        'private user path' = "(?i)(?:[A-Z]:\\$windowsUsers\\[^\\\s]+|/$windowsUsers/[^/\s]+|/$unixHome/[^/\s]+)"
        'private key' = '-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----'
        'access token' = '(?i)(?:sk-[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9]{30,})'
        'email address' = '(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b'
    }
    foreach ($entry in $patterns.GetEnumerator()) {
        if ($text -match $entry.Value) { $violations.Add("$($entry.Key): $normalized") }
    }
}

if ($violations.Count -gt 0) {
    throw "Privacy scan failed:`n$($violations -join "`n")"
}

Write-Output "PRIVACY_CHECK=PASS files=$($candidateFiles.Count)"
