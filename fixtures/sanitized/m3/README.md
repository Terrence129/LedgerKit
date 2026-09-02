# M3 synthetic Excel migration fixtures

These workbooks are generated, synthetic, and safe for the public repository. They do not derive from or contain any private ledger data.

- `cash-import-valid.xlsx` covers the eight supported cash-migration sheets, a formula-backed authoritative input with a cached value, derived/status/display formulas, explicit per-account cut-over policies, FX, income, expense, fee, transfer, and exchange rows.
- `cash-import-invalid.xlsx` covers a missing/error formula cache, invalid date/reference, duplicate ID, category-direction mismatch, and missing FX.
- `cash-import-modified.xlsx` differs from the valid fixture by one formula result so that it must produce a distinct source hash and candidate.

Regenerate from the repository root:

```powershell
pwsh -NoProfile -File tools/check-m3-fixtures.ps1
```

The generator pins workbook document time and metadata. Running it twice must produce byte-identical files. The application reads `.xlsx` only, never evaluates formulas, and treats every formula as evidence whose cached value is validated according to the column role.
