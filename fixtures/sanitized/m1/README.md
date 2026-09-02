# M1 shared 10k XLSX fixture

`ledgerkit-known-template-10000.xlsx` contains exactly 10,000 deterministic, synthetic transaction rows. It contains no real names, accounts, merchants, notes, balances, paths, or source-workbook content. Amounts are stored as text so both desktop spikes exercise the same no-binary-float import boundary.

Regenerate from the repository root with `npm --prefix spikes/tauri run generate:fixture`; verify byte-for-byte reproducibility with `npm --prefix spikes/tauri run check:fixture`. `manifest.json` records the committed SHA-256 and contract metadata. The fixture and generator are intentionally shared with the later Avalonia spike.
