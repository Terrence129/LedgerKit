# LedgerKit 1.0.0-beta.1 delivery audit

> Scope: automated P0 candidate audit. Real private-data reconciliation, code signing, the four-week dual-entry period/full-month cycle, and the owner cut-over decision remain manual release gates.

## P0 matrix

| Capability | Implementation evidence | Automated evidence | Result |
|---|---|---|---|
| Local ledger and catalog | Core-owned Schema v7, local-data policy, settings/catalog commands | SQLite migration/schema and facade tests | Pass |
| Cash events and immutable correction | Typed Income/Expense/Adjustment/Transfer/FX, revisions and reversals | Domain, cash-store, canonical and rebuild tests | Pass |
| Expense analysis | One named Core query, daily rebuildable projection, same response for chart/table | M0 golden suite, 100k release benchmark, bilingual component tests | Pass |
| Investments and valuation | Typed trades/dividends/fees/opening facts, MWA and as-of evidence | Investment, valuation, M5 golden/reconciliation tests | Pass |
| Excel migration | Strict 15-sheet contract, staging, reconciliation, atomic candidate switch | M3/M5 deterministic fixture and importer failure tests | Pass |
| Overview, activity, assets, quality, settings | Exactly five top-level entries; native HTML/CSS chart | TypeScript build and 38 component/contract tests | Pass |
| Backup, restore and export | Encrypted portable package, verified restore, retention, redacted diagnostics | Portable backup/restore/export negative and round-trip tests | Pass |
| Offline/privacy boundary | No backend, updater, listener, arbitrary SQL, path IPC or remote content | Static release/privacy checks and DTO/path negative tests | Pass |

## Test inventory

- `tools/check.ps1`: pinned toolchains, clean `npm ci`, M0 golden contracts, TypeScript production build, UI tests, rustfmt, release-mode tests for all Rust targets/features including the ignored 100k gate, Clippy with warnings denied, M3/M5 deterministic workbook contracts, dependency/capability/assets, Beta release contract, privacy scan, and whitespace check.
- `tools/beta-performance.ps1`: ignored release-mode 100k Core/SQLite benchmark plus 30-sample server-render expense UI P95.
- Installer and runtime measurement are recorded in [`performance-and-size-1.0.0-beta.1.md`](performance-and-size-1.0.0-beta.1.md).

## Security negative audit

| Threat | Enforced boundary and evidence | Result |
|---|---|---|
| Forged posting/numeric payload | IPC accepts typed high-level business requests only; unknown path and numeric financial fields are rejected | Pass |
| Arbitrary SQL | No SQL plugin or query command; SQLite is Infrastructure-only and parameterized | Pass |
| Path escape/synchronized live DB | Native one-use picker plus Core absolute-path and local-root validation | Pass |
| Remote content/network | Packaged window has no remote URL, CSP is local/IPC-only, updater absent, default runtime network measured | Pass |
| Unauthorized window/command/shell | One `main` capability, 25 named allowlisted operations, no process launcher/listener | Pass |
| Malicious XLSX | Macro/external-link/contract/resource limits, formula evidence rules, staging fail-closed | Pass |
| Log/diagnostic leak | Privacy scanner plus diagnostics allowlist and redaction test | Pass |

## Bilingual and accessibility audit

The five navigation resources exist in both exact locales and the complete resource key sets match. Component tests render both locales for navigation, activity, assets, quality, settings/safety and authoritative expense results. Locale persistence/fallback tests confirm restart behavior; canonical Core fixtures confirm locale does not enter financial data or hashes.

All interactive controls are native semantic elements with visible `:focus-visible`; tables retain header scope and the decorative expense bars are `aria-hidden`. `aria-live` announces loading/errors. CSS includes narrow/zoom reflow, Windows forced-colors and reduced-motion modes. Keyboard and semantic structure are covered by source audit plus server-render assertions; a final installer UI walkthrough remains part of the owner acceptance gate, not an automated financial gate.
