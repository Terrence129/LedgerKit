# LedgerKit Tauri M1 vertical spike

This directory is a disposable, evidence-producing M1 spike. It is not the
production LedgerKit application and does not accept ADR-0001.

The spike exercises one local-first vertical slice:

- a Rust-owned SQLite schema migration and high-level event transaction;
- deterministic posting and projection refresh using decimal strings;
- the shared 10,000-row synthetic XLSX contract;
- a paged activity list, native net-worth bar, and Top 10 + Other expense bars
  with a semantic table sourced from the same Rust query result;
- one-time native file authorization and hash-addressed attachment copy;
- standardized XLSX export;
- Argon2id/AES-256-GCM password-encrypted backup and restore; and
- a per-user NSIS package in thin and WebView2-runtime-included forms.

The measured result is an **M1-A hard-gate pass**. The final five-minute run
measured idle full-tree RSS P95 of 147,173,376 bytes and zero established remote
endpoints. The 100k expense cold/warm P95 values are 2.6740/2.1098 ms. This does
not select the production stack or accept the Proposed projection ADR; see the
full method, raw data, limitations, and gate matrix in
[`../../docs/benchmarks/m1/tauri.md`](../../docs/benchmarks/m1/tauri.md).

Two platform details are part of the measured result and must not be silently
removed during experimentation:

- inactive/hidden/minimized WebView2 instances receive a low memory target and
  return to normal on focus; and
- the packaged WebView2 arguments disable the traced `msOneAuthWAM` background
  connection. This is runtime-version-sensitive and requires a new five-minute
  network trace whenever WebView2 changes.

The 10k XLSX file picker is asynchronous and parsing runs on the blocking worker
pool. The expense query reads a rebuildable derived projection; authoritative
events/postings remain the source of truth, and projection deletion/rebuild is
covered by Rust tests.

## Toolchain

The checked-in locks require Rust 1.98.0, Tauri crate 2.11.5 / CLI 2.11.4,
Node 24 or newer, npm, and the MSVC x64 desktop workload with Windows SDK.
Rust and npm caches, generated `dist`, Cargo `target`, databases, backups, and
installers are intentionally ignored.

From this directory:

```powershell
npm ci
npm run check
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets --all-features
npm run tauri:build:standard
```

The runtime-included package is generated separately because it is much larger:

```powershell
npm run tauri:build:offline
```

Regenerate or verify the shared fixture from the repository root with:

```powershell
npm --prefix spikes/tauri run generate:fixture
npm --prefix spikes/tauri run check:fixture
```

Run the repeatable core and runtime measurements with:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --release --example core_bench
pwsh -NoProfile -File tools/measure-runtime.ps1 `
  -Executable src-tauri/target/release/ledgerkit-tauri-spike.exe `
  -OutputPath tmp-runtime.json
```

All fixture and benchmark data is deterministic and synthetic. Do not point
the spike at the private source workbook or a real ledger.
