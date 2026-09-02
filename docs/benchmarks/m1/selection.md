# M1 stack selection and production-scaffold baseline

> Decision date: 2026-09-02
>
> **Selected: Tauri 2 + React/TypeScript + Rust Core.** Both candidates passed every hard gate. The project-owner rule therefore selects Tauri; Avalonia Native AOT remains the measured reversal baseline.

## Evidence and method

This report compares the already published [`tauri.md`](tauri.md) and [`avalonia.md`](avalonia.md) without changing their gates or choosing only favorable measurements. Both spikes consumed the same 31 M0 fixture groups and the same 10,000-row synthetic XLSX (`sha256:d7bbf52a86d2655ec09fe82fa42690f1c9e7aad6d323c6f167a86c797c024bd5`). Both spike commits are ancestors of `main`, and annotated tag `m0-baseline` resolves to `fb411a6613ed76d037a11d0660aedfc75e9e759e`.

Before deleting the disposable sources, task 04 reran:

```text
pwsh -NoProfile -File tools/check-m0-fixtures.ps1
npm --prefix spikes/tauri ci
npm --prefix spikes/tauri run check
cargo fmt/clippy/test --manifest-path spikes/tauri/src-tauri/Cargo.toml
pwsh -NoProfile -File spikes/avalonia/tools/check.ps1
```

Results were Tauri 2/2 Vitest files (4 tests) and 10/10 Rust tests passed; Avalonia 10/10 checks passed; both paths passed the unchanged M0 generation, schema and semantic checks. No missing metric was found, so the 30-start and five-minute network/RSS experiments were not rerun merely to create different samples.

The locally retained installer artifacts were checked against their reports immediately before spike deletion:

| Artifact | Bytes | SHA-256 verification |
|---|---:|---|
| Tauri thin NSIS | 3,462,816 | `88CF51B8C6DA851D6F166E231CEC28CC67713DBB46F2E6E298D1273295057A47` — match |
| Avalonia framework-dependent NSIS | 10,451,794 | `003562601ECE5846BCAF8ADF2A044D960F1E13C148D52A78E4D4E393CAA779C0` — match |
| Avalonia Native AOT NSIS | 17,675,474 | `B81BDC623A5D8416C9AE129C9D5E3036CBA15ED18CD76E2F27FBC5B6DD7C863C` — match |

Build artifacts remain ignored and are not repository deliverables.

## Same-gate comparison

| Hard gate | Tauri 2 / Rust / React | Avalonia / C# | Decision use |
|---|---|---|---|
| One installable app; no server/service/daemon | PASS; app + WebView children, no listener/sidecar | PASS; one app process, no listener/sidecar | Equal pass |
| Offline core | PASS; resolver-failure local SQLite/IPC launch | PASS; dead-proxy local launch and source check | Equal pass |
| Standard thin package ≤ 30 MiB | 3.302 MiB | FDD 9.968 MiB; AOT 16.857 MiB | Both pass; Tauri smaller |
| Installed payload ≤ 75 MiB | 12.021 MiB | FDD 37.363 MiB; AOT 64.855 MiB | Both pass; Tauri more margin |
| Runtime-included package reported separately | 253.008 MiB NSIS | trimmed 113.811 MiB files; AOT 64.818 MiB files | Reported, not substituted |
| Cold start P95 ≤ 1.5 s | 1.024195 s | AOT 0.849659 s; FDD 2.845878 s FAIL retained | Both deployable choices pass; Avalonia depends on AOT |
| Idle full-tree RSS P95 ≤ 150 MB | 147,173,376 bytes | 95,350,784 bytes | Both pass; Tauri has less margin |
| Residual tree after 10 s = 0 | 0 | 0 | Equal pass |
| Save/filter/page P95 ≤ 200 ms | write 4.0708; page 0.2596; query 1.7235; draw 110.3 ms | write 3.9664; page 0.2547; query 1.6510; draw 91.178 ms | Equal pass |
| 100k account/timeline P95 ≤ 1 s | 5.5371 ms | 5.6459 ms | Equal pass |
| 10k XLSX import ≤ 10 s and UI nonblocking | Rust 66.000 ms; blocking worker | Open XML 1,759.6224 ms; worker boundary | Equal pass; Rust faster |
| Current DB ≤ 20 MB | 102,400 bytes | 102,400 bytes | Equal pass |
| 100k DB ≤ 100 MB | 54,038,528 bytes | 54,104,064 bytes | Equal pass |
| Default runtime network = 0 | five-minute full-tree endpoints `[]` | five-minute process endpoints `[]` | Equal pass |
| Clean checks + package ≤ 10 min | 8 min 27.326 s | 4 min 2.272 s | Both pass; Tauri less margin |
| Direct production dependencies ≤ 25 | 21 | 4 | Both pass; Avalonia simpler |
| First-load gzip ≤ 1.2 MiB | 64.45 KiB | no web payload | Both pass by architecture |
| Tauri plugins ≤ 8 | 0 | not applicable | Pass |
| Named privileged operations ≤ 25 | 12 IPC | 12 in-process facade methods | Equal pass |
| M0 golden subset/canonical hashes | Exact Rust subset + full M0 | Exact C# subset + full M0 | Equal pass |
| Install/start/close/uninstall | Actual PASS | Both NSIS variants actual PASS | Equal pass |
| 100k expense cold/warm ≤ 150/50 ms | 2.6740 / 2.1098 ms | 2.6989 / 1.8187 ms | Equal pass; ADR-0015 still Proposed |
| Expense response ≤ 32 KiB; no N+1 | 17,698 bytes; one aggregation | 17,698 bytes; one aggregation | Equal pass |
| Excel trust boundary | Rust adapter; no XLSX/finance in WebView | C# Core adapter | Both pass; Tauri Rust adapter selected |
| Privilege boundary | Explicit per-command capability; no arbitrary SQL/posting/shell/path/URL | In-process facade; no arbitrary SQL/posting/shell/path/network | Both pass |

## Selection

Tauri passes every hard gate, the known-template Excel test and the explicit privilege boundary. The authorized rule therefore selects Tauri without using Avalonia's lower RSS or single-language advantage to override the rule. ADR-0001 records the consequence and reversal triggers; ADR-0007 through ADR-0010 record XLSX, encryption, WebView2 and update/signing decisions.

The following risks remain visible rather than averaged away:

- Tauri idle RSS is only 2,826,624 bytes below the 150 MB hard ceiling and must be tracked on every WebView2/Tauri update.
- Its zero-network result depends on a runtime-version-sensitive `msOneAuthWAM` feature control; every WebView2 change requires a new five-minute trace.
- Tauri uses Rust plus TypeScript and had the slower clean build. Duplicate financial logic across that boundary is prohibited.
- Avalonia's passing startup depends on Native AOT; framework-dependent startup failed, and the AOT path carries trimming/reflection constraints and a much larger payload.

## Final production-scaffold baseline

The selected spike was not adopted wholesale. `app` contains a deliberately reduced health-only skeleton with `UI → typed IPC → Application → Domain`; Infrastructure implements a locale-settings port, and the measured platform-specific WebView2 memory adapter is isolated. It has no production SQLite schema or financial behavior before M2.

| Baseline | Result | Budget/result |
|---|---:|---|
| Standard per-user NSIS | 1,784,288 bytes; SHA-256 `683604941B3A84BCB3013061831844DEFDCD1B7090961253FFA663E7C5B6D846` | PASS ≤ 30 MiB |
| Installed application payload | 7,789,337 bytes | PASS ≤ 75 MiB |
| First-load HTML/CSS/JS gzip | 63,374 bytes | PASS ≤ 1.2 MiB |
| Direct production dependencies | 8 | PASS ≤ 25 |
| Tauri plugins | 0 | PASS ≤ 8 |
| Named privileged IPC operations | 2 (`get_ledger_status`, `update_settings`) | PASS ≤ 25 |
| Locales/resource keys | `zh-CN` + `en-US`, identical key sets | PASS |
| Installer lifecycle | install 0; main window ready; normal close true; uninstall 0; directory removed | PASS |

The fresh production Release profile compiled in 3 min 19 s on the same development machine; package overhead completed inside the 10-minute hard limit. The localized MSVC linker “creating import library” line is compiler-classified `linker_messages` output rather than a source warning and is explicitly allowed; Clippy still denies all warnings.

`tools/check.ps1`, `tools/test.ps1` and `tools/build.ps1` each bootstrap locked npm dependencies, so a clean checkout needs one command for checks/tests and one for the package. Final task-04 verification records the clean-checkout result after the stage commit, before merging to `main`.

## Retention and exclusions

- Retained: both benchmark reports, this selection report, M0/M1 sanitized fixtures, generator/hash evidence, ADRs and Git history.
- Deleted from the current tree: both disposable spike source trees.
- Not claimed: signed package, GitHub Release, pristine offline-VM runtime installation, real private-workbook reconciliation, or acceptance of Proposed ADR-0015.
