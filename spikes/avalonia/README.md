# LedgerKit Avalonia M1 spike

Disposable .NET 10 + Avalonia vertical spike for the M1 stack comparison. It
is not the production application and does not accept ADR-0001 or ADR-0015.
All sample data is synthetic.

## Prerequisites

- Windows x64
- .NET SDK 10.0.400 (pinned by `global.json`)
- NSIS 3 for per-user installer builds

Package versions and transitive restore results are locked by
`Directory.Packages.props` and the four `packages.lock.json` files.

## Check and run

From this directory:

```powershell
pwsh -NoProfile -File tools/check.ps1
dotnet run --project src/LedgerKit.AvaloniaSpike -c Release
dotnet run --project benchmarks/LedgerKit.AvaloniaSpike.Benchmarks -c Release
```

`tools/check.ps1` verifies formatting, builds with warnings as errors, runs the
spike checks, and then runs the repository M0 fixture validator. The checks use
the repository's unchanged M0 JSON and the shared 10,000-row XLSX.

## Publish and measure

```powershell
pwsh -NoProfile -File tools/build-packages.ps1 -AttemptNativeAot
pwsh -NoProfile -File tools/test-installer.ps1 -Installer <installer.exe>
pwsh -NoProfile -File tools/measure-runtime.ps1 `
  -Executable <ledgerkit-avalonia-spike.exe> -ColdRuns 30 -IdleSeconds 300
pwsh -NoProfile -File tools/test-offline-core.ps1 `
  -Executable <ledgerkit-avalonia-spike.exe>
```

The package script emits and measures:

- a framework-dependent Windows x64 payload and thin NSIS installer;
- a self-contained, partially trimmed Windows x64 payload;
- when `-AttemptNativeAot` is supplied, a Native AOT payload and NSIS
  installer.

Native `.pdb` files from Skia/HarfBuzz packages are intentionally excluded
from publish payloads; they are debugging artifacts and are not executable
application content. The AOT attempt temporarily evaluates AOT restore assets,
then restores the standard locked dependency state before returning.

## Boundaries

- `LedgerKit.AvaloniaSpike.Core` exclusively owns SQLite, decimal validation,
  posting/projection, XLSX, file authorization, and backup/restore.
- The UI calls 12 named in-process facade operations. It receives view models
  and decimal strings, and has no arbitrary SQL, posting, shell, or network
  interface.
- Financial events, postings, projection changes, and watermarks commit in one
  SQLite transaction. Projection deletion/rebuild is deterministic.
- XLSX selection creates a one-use in-memory authorization. Attachment copies
  are size-bounded and hash-addressed inside managed storage.
- Backups use PBKDF2-HMAC-SHA256 with 600,000 iterations and AES-256-GCM. This
  is a spike choice, not an accepted production cryptography ADR.

Formal results and limitations are recorded in
`../../docs/benchmarks/m1/avalonia.md`.
