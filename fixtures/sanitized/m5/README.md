# M5 synthetic full-migration fixtures

These workbooks are deterministic, synthetic, and safe for the public repository. They contain no data derived from a private ledger.

- `full-import-history.xlsx` proves full-history cash and investment replay, including an explicit opening-balance event, a disabled account with preserved history, price/FX selection, holding metrics, checks, and the expense reference bridge.
- `full-import-cutover.xlsx` proves end-of-day explicit cut-over: rows dated on or before the cut-over are evidence-only, opening cash/position/performance events preserve a zero position and portfolio-level expense, and later events are replayed normally.
- `full-import-invalid.xlsx` proves missing per-account/per-portfolio policy and an inconsistent check block the candidate switch.

The full contract extends the cash sheets with portfolios, instruments, security prices, investment activity, holding baselines, checks, and expense-analysis evidence. The check matrix includes source/master-data/event/currency counts and valued net assets for an explicit as-of date. Formula output remains evidence-only; Core recomputes every posting and reconciliation value.

Regenerate and compare byte-for-byte from the repository root:

```powershell
pwsh -NoProfile -File tools/check-m5-fixtures.ps1
```
