# M1 Tauri vertical spike report

> Measured 2026-09-02 on branch `phase/m1-tauri-spike`, starting from
> `fb411a6613ed76d037a11d0660aedfc75e9e759e` (`m0-baseline`, `origin/main`).
>
> **Overall result: PASS for the Tauri M1-A spike.** Every M1 hard gate measured
> by task 02 passes on this machine. Stretch targets are reported separately and
> are not converted into hard gates. This result does not select the production
> stack, accept ADR-0001/ADR-0015, tag M1, or replace the required same-fixture
> Avalonia comparison.

## Scope and interpretation

This is a disposable, real Tauri 2 + React/TypeScript + Rust Core spike. It
does not accept ADR-0001 and does not alter M0 financial rules or golden
answers. The only authoritative database and financial calculations are in
Rust. The UI receives named view models and decimal strings; it has no general
SQL, posting, shell, remote URL, or filesystem interface.

Three initially failing measurements were corrected without waiving a gate:

1. WebView2's idle tree was explicitly put in the documented low-memory target
   state while the window is inactive/hidden/minimized, and returned to normal
   when focused.
2. Network tracing isolated the unexpected connection to the WebView2
   `msOneAuthWAM` feature. Disabling that feature, together with the existing
   background-service switches and CSP, produced zero established remote
   endpoints during the formal five-minute run. This feature switch is runtime
   version-sensitive and must be retested on every WebView2 upgrade.
3. The 100k expense fact scan failed. A rebuildable daily/category projection
   was then tested under the exception explicitly allowed by ADR-0014. The
   architectural choice remains Proposed in ADR-0015; the spike implementation
   proves feasibility but does not make it authoritative.

## Locked versions

Versions are locked exactly in `Cargo.toml`, `Cargo.lock`, `package.json`, and
`package-lock.json`.

| Component | Locked/measured version | Note |
|---|---:|---|
| Rust / Cargo / rustup | 1.98.0 / 1.98.0 / 1.29.0 | stable MSVC x64; rustfmt 1.9.0, Clippy 0.1.98 |
| Tauri Rust / CLI / JS API | 2.11.5 / 2.11.4 / 2.11.1 | no Tauri plugins |
| React / React DOM | 19.2.8 / 19.2.8 | UI only |
| TypeScript / Vite | 7.0.2 / 8.2.2 | build-time |
| SQLite bundled | 3.53.2 | `rusqlite 0.40.2`, bundled |
| Calamine / rust_xlsxwriter | 0.36.1 / 0.99.0 | Rust import/export candidates |
| ExcelJS / JSZip | 4.4.0 / 3.10.1 | development-only compatibility adapter |
| Node / npm | 24.16.0 / 11.13.0 | build and comparison only; no sidecar |
| WebView2 runtime | 151.0.4129.107 | shared installed runtime used by thin package |

Primary references: [Rust releases](https://forge.rust-lang.org/),
[Tauri releases](https://github.com/tauri-apps/tauri/releases),
[Tauri window configuration](https://v2.tauri.app/reference/config/#windowconfig),
[WebView2 feature flags](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/webview-features-flags),
[React versions](https://react.dev/versions),
[TypeScript releases](https://github.com/microsoft/TypeScript/releases),
[Vite releases](https://github.com/vitejs/vite/releases), and
[SQLite changes](https://sqlite.org/changes.html).

## Machine and method

- Windows 11 Home x64, build 26200, Chinese locale.
- AMD Ryzen 7 H 255, 8 cores / 16 logical processors.
- 32 GiB RAM and YMTC PC411-1TB-B SSD.
- Visual Studio Community 18 with MSVC x64 desktop tools and Windows 11 SDK
  26100 on the development volume.
- Rust homes, npm data, and build caches were kept off the system volume.

Cold start is process launch until the complete React overview writes a ready
signal. Thirty runs used isolated synthetic app data and verified zero residual
processes after each run. The application was launched hidden for automation,
so its WebView2 memory target was Low during the idle measurement. Idle RSS is
the sum of the root application and every descendant, sampled for at least five
minutes; the gate uses all samples whose timestamp falls in the final 30
seconds, not merely the last N samples. Network capture recorded established
TCP connections owned by that same process tree.

Core benchmarks use `src-tauri/examples/core_bench.rs`, release mode,
deterministic synthetic data, indexed SQLite queries, and nearest-rank P95.
Each cold expense sample opens a fresh SQLite connection after the database is
generated; the OS page cache is not flushed. TypeScript XLSX timing uses
`node --expose-gc`. Package sizes are exact bytes; MiB divides by 1,048,576.

The clean build used a never-before-created Cargo target directory and `npm ci`,
then fixture verification, TypeScript build/tests, Rust fmt/Clippy/tests, and a
standard NSIS build. Its timer includes dependency installation and all checks.

## Implemented slice and safety checks

- Schema v2 is opened through a controlled Rust API. A pre-migration backup is
  created before migration; WAL, foreign keys, integrity checks, schema version,
  projection version, and watermarks are explicit.
- A high-level `post_event` command validates the request, derives posting and
  projection changes, and commits event, posting, projection, and watermarks in
  one SQLite transaction. A failpoint proves complete rollback.
- The expense daily/category projection stores canonical decimal strings. It is
  derived only from authoritative facts, can be deleted, and deterministically
  rebuilds to the same query result and canonical hash.
- `rust_decimal` and canonical decimal strings carry authoritative amounts.
  JavaScript does not recalculate financial totals; integer basis points from
  Rust drive bar widths.
- Expense KPI, 12 buckets, Top 10 + Other, semantic table, and drill-down
  contexts come from one Rust query result and one canonical result hash.
- The 10,000-row XLSX analysis command uses an async native file dialog and
  moves parsing to Tauri's blocking worker pool, so the UI event loop remains
  responsive. A source-level regression test enforces this boundary.
- Native file selection creates a one-use in-memory authorization. Copying is
  limited to a regular file of at most 5 MiB and writes a hash-addressed name
  inside the managed attachment directory. IPC never accepts a destination.
- Backups use SQLite Online Backup, Argon2id key derivation, and AES-256-GCM.
  Tests reject a wrong password, prove plaintext is absent from the envelope,
  and exercise restore rollback.
- CSP permits only packaged app/asset origins and Tauri IPC. The capability
  grants exactly 12 named commands to `main`. Source tests reject remote APIs,
  SQL/shell/general-filesystem plugins, and missing network/memory safeguards.
- Negative tests cover a forged posting field, path traversal, token reuse,
  wrong backup password, and transaction interruption.

## Correctness and fixture evidence

The unchanged M0 repository check passed:

```text
M0 fixture generation is byte-for-byte reproducible.
Validated 31 fixture groups, 56 financial rules, 211 canonical hashes,
25 expense query results, 186 JSON files against schemas, and 8 Accepted ADRs.
```

The Rust slice reproduces M0 fixture 01 postings and sequence hash
`sha256:ec8b3e18aaedd6f64618e8667b619d7e61de27f68e41e499e325b86cd43936f2`.
Its 12-bucket expense result produces 10 chart items plus Other=`30` and hash
`sha256:7cd365ef12db020eb178975704fd2388cad37b5a4f378c6debf5e3aef27a8beb`.
Deleting and rebuilding the expense projection preserves that exact result and
hash.

The shared XLSX contains exactly 10,000 synthetic rows and no source-workbook
content. Its byte-stable SHA-256 is
`d7bbf52a86d2655ec09fe82fa42690f1c9e7aad6d323c6f167a86c797c024bd5`.
Both adapters preserve the same row count, headers, date strings, currency
codes, and amount strings.

## Raw measurements

### Package and resource sizes

| Measurement | Exact result | Gate result |
|---|---:|---|
| Installed/tested standard thin NSIS | 3,462,816 bytes (3.302 MiB), SHA-256 `88CF51B8C6DA851D6F166E231CEC28CC67713DBB46F2E6E298D1273295057A47` | PASS ≤ 30 MiB; stretch PASS ≤ 20 MiB |
| Clean-run standard thin NSIS | 3,463,049 bytes, SHA-256 `38CA0B1B335689E31EE77E3E72C7EFF974DCC93CE6990271FEE88D7663D0DE43` | corroborating build; NSIS metadata makes byte hash run-specific |
| Installed executable | 12,525,568 bytes | — |
| Installed uninstaller | 79,159 bytes | — |
| Installed application payload | 12,604,727 bytes (12.021 MiB) | PASS ≤ 75 MiB; stretch PASS ≤ 50 MiB |
| Runtime-included NSIS | 265,298,454 bytes (253.008 MiB), SHA-256 `9DC8D245BD698A45D3E76FB8329ABD7F519BFDDE5487108924627C787563AB40` | reported separately; not substituted for thin metric |
| Existing shared WebView2 runtime directory | 894,464,255 bytes (853.028 MiB) | informational; not incremental payload |
| First HTML / CSS / JavaScript gzip | 0.31 / 1.41 / 62.73 KiB | — |
| First-load gzip total | 64.45 KiB | PASS ≤ 1.2 MiB |

The standard installer, installed visible application, normal close, and
uninstaller were executed rather than inferred. Silent install/uninstall both
returned 0. The started window obtained a non-zero main handle and did not exit
early. `CloseMainWindow()` returned true, the process exited within 10 seconds,
the uninstall registry entry count became zero, and the per-user install
directory was absent after uninstall. Packages are unsigned because no
production certificate was provided. The runtime-included package was built and
measured but not installed on a separate pristine offline VM, so no such
installed-footprint claim is made.

### Cold start and rendering (milliseconds)

Cold wall-to-interactive raw values:

```text
1407.898, 1024.195, 666.696, 645.364, 627.712, 639.641, 659.191,
624.796, 644.213, 693.991, 636.600, 667.382, 639.191, 634.552,
627.037, 675.635, 648.761, 639.911, 628.354, 641.770, 639.601,
642.001, 639.448, 631.790, 635.814, 631.691, 635.860, 642.725,
640.149, 633.232
```

P95 is **1,024.195 ms**: hard PASS ≤ 1.5 s; stretch FAIL ≤ 1.0 s.
The first run is retained.

React first-render raw values:

```text
72.6, 127.7, 127.9, 128.6, 127.0, 136.9, 121.7, 129.2, 127.0,
144.6, 126.2, 134.6, 135.1, 129.1, 131.0, 138.8, 126.3, 128.0,
128.6, 124.7, 126.7, 125.8, 123.3, 126.7, 129.1, 124.5, 126.5,
126.1, 127.4, 128.5
```

First-render P95 is **138.8 ms**.

Expense-bars/table first-draw raw values:

```text
42.7, 97.4, 98.2, 99.2, 95.3, 107.6, 93.2, 100.5, 96.6, 114.9,
97.8, 106.1, 105.8, 99.6, 102.2, 110.3, 97.0, 93.3, 99.5, 95.9,
95.5, 93.1, 94.1, 97.7, 99.8, 95.3, 97.3, 97.4, 98.7, 99.3
```

Expense draw P95 is **110.3 ms**. Every cold run left zero descendants.

### Five-minute process-tree and default-network measurement

- Requested idle: 300 s; actual: 301,077.366 ms; samples: 238.
- Final-30-second RSS P95: **147,173,376 bytes = 147.173 MB =
  140.355 MiB** — hard PASS ≤ 150 MB (and also below 150 MiB); stretch FAIL.
- Peak full-tree RSS: **303,431,680 bytes (289.375 MiB)**.
- Full-run observed range: 52,531,200–303,431,680 bytes. The tree trims after
  startup; the hard gate intentionally uses the documented final idle window.
- End tree: application 30,007,296 bytes plus six WebView2 processes totaling
  124,379,136 bytes; final total 154,263,552 bytes.
- Established remote endpoints over the complete run: `[]` — PASS.
- `CloseMainWindow()` returned true; residual count after 10 seconds: 0 — PASS.

The timestamped raw samples in the actual final 30-second window were:

```text
271159.645:147116032, 272384.895:147116032, 273628.342:147116032,
274871.421:147116032, 276111.431:147116032, 277339.815:147116032,
278571.094:147116032, 279833.481:147144704, 281065.098:147140608,
282306.006:147140608, 283540.505:147173376, 284769.828:147140608,
286022.055:147140608, 287269.318:147140608, 288515.325:147140608,
289758.686:147140608, 291002.254:147140608, 292236.131:147140608,
293498.367:147140608, 294871.525:147140608, 296132.623:147140608,
297378.884:147140608, 298609.239:147140608, 299842.115:154263552
```

The network fix is deliberately narrow: an A/B trace identified
`msOneAuthWAM`; the packaged configuration disables it and the source test
locks the setting. The absence of `fetch`, restrictive CSP, or flags alone is
not counted as the pass—the five-minute process-tree trace is the evidence.

An additional offline-core launch mapped all host resolution to failure for
the test process. The packaged binary still loaded the initial SQLite/IPC view
model and wrote ready in 123.7 ms (expense draw 85.0 ms), then exited normally
within 10 seconds. The resolver override is test-only and is not product
configuration.

### Rust Core and SQLite (milliseconds unless noted)

| Measurement | Result | Gate result |
|---|---:|---|
| Empty open + schema migration | 26.0747 | PASS ≤ 200 ms |
| Rust Calamine 10k XLSX import | 66.000 | PASS ≤ 10 s |
| TypeScript ExcelJS 10k import | 464.7339 | PASS ≤ 10 s |
| Standardized XLSX export, 43 rows | 25.6141 | PASS ≤ 200 ms |
| Current synthetic DB size | 102,400 bytes | PASS ≤ 20 MB |
| Generate 100k events + projection | 4,262.8052 | setup metric |
| 100k-event DB size | 54,038,528 bytes (51.535 MiB) | PASS ≤ 100 MB |
| 100k timeline query P95 | 5.5371 | PASS ≤ 1 s |
| 100k expense fresh-connection P95 | 2.6740 | PASS ≤ 150 ms |
| 100k expense warm P95 | 2.1098 | PASS ≤ 50 ms |
| Expense result JSON | 17,698 bytes | PASS ≤ 32 KiB |

Current event-write raw values; P95 **4.0708 ms**:

```text
3.7605, 3.0432, 3.0646, 2.8819, 3.0165, 3.0282, 3.0347, 2.9202,
3.1023, 2.9812, 2.8809, 3.0708, 3.0454, 2.9861, 4.0708, 2.9992,
2.8470, 3.0486, 4.0843, 2.9375, 3.2563, 2.6345, 3.0765, 3.0696,
2.9028, 3.0501, 3.0901, 2.8580, 3.1371, 2.9384
```

Current activity-page raw values; P95 **0.2596 ms**:

```text
0.4569, 0.2596, 0.1063, 0.1266, 0.0914, 0.0827, 0.0917, 0.0820,
0.0811, 0.0783, 0.0790, 0.0832, 0.0795, 0.1309, 0.0791, 0.0790,
0.0768, 0.0777, 0.0850, 0.0773, 0.0826, 0.0776, 0.0778, 0.0793,
0.0772, 0.0766, 0.0769, 0.0757, 0.0809, 0.0929
```

Current expense-query raw values; P95 **1.7235 ms**:

```text
1.7235, 1.2189, 0.9097, 0.7775, 1.1104, 0.9595, 0.7620, 0.7173,
2.1409, 0.8316, 0.7336, 1.0290, 0.8329, 0.7240, 0.7561, 1.6542,
0.8133, 0.9323, 1.0019, 0.7740, 0.6954, 0.9297, 1.2288, 1.2419,
1.3115, 0.9642, 0.7165, 0.7881, 1.2759, 0.8470
```

100k timeline raw values; P95 **5.5371 ms**:

```text
5.2275, 4.8812, 5.0198, 4.7714, 5.0035, 5.2201, 4.3328, 5.0827,
5.9042, 4.8454, 4.3144, 4.4855, 4.4975, 5.4664, 5.2115, 4.7058,
4.7827, 4.2868, 5.5371, 4.8912, 5.2648, 4.8115, 4.8915, 4.4755,
5.0703, 4.4260, 4.8182, 4.6530, 4.0785, 5.3124
```

100k expense fresh-connection raw values; P95 **2.6740 ms**:

```text
2.0682, 2.0229, 2.0460, 2.4104, 2.0329, 1.7870, 2.4274, 2.3175,
1.9896, 2.2464, 2.2631, 2.1497, 1.7987, 2.3676, 2.6740, 2.7074,
1.9045, 2.2273, 1.7638, 2.2955, 2.0640, 2.3397, 2.0756, 2.1059,
2.1328, 2.0115, 1.9765, 1.7383, 2.1136, 1.9024
```

100k expense warm raw values; P95 **2.1098 ms**:

```text
1.7151, 1.9281, 1.4383, 1.9351, 1.5455, 2.0610, 1.5641, 2.1098,
1.7862, 1.5840, 1.6779, 1.6784, 1.7175, 1.5859, 1.9926, 1.6600,
1.9128, 1.5627, 1.8893, 1.6273, 1.8419, 1.4365, 1.8192, 1.5951,
1.7270, 1.6031, 1.8066, 1.5825, 2.1131, 1.4099
```

The pre-projection fact scan measured 1,259.3991 ms cold and 1,326.8799 ms
warm P95. A Rust Decimal aggregate reduced it to approximately 59.76/86.14 ms,
but still missed warm. The tested projection then passed both gates. One
projection-backed aggregation creates the complete canonical result; there is
no per-bucket/N+1 SQL loop.

### TypeScript adapter memory and clean build

- ExcelJS RSS before: 64,086,016 bytes.
- ExcelJS RSS after: 157,257,728 bytes.
- Delta: 93,171,712 bytes.
- Final clean dependency install + all checks + standard package:
  **507,325.970 ms (8 min 27.326 s)** — hard PASS ≤ 10 min; stretch FAIL.
- Fresh Cargo target contents: 6,202,504,504 bytes. This is disposable build
  cache/output, not installed application payload, and was removed afterward.
- `npm ci` emitted deprecation notices from development-only ExcelJS transitive
  packages; `npm audit` examined 157 packages and found 0 vulnerabilities.

## XLSX adapter comparison

| Dimension | Rust: Calamine + rust_xlsxwriter | TypeScript: ExcelJS |
|---|---|---|
| Known-template compatibility | 10,000/10,000 rows; strings preserved | 10,000/10,000 rows; strings preserved |
| Import time | 66.000 ms | 464.7339 ms |
| Memory evidence | inside short Rust benchmark; no isolated delta | +93,171,712 bytes in Node benchmark |
| Trust boundary | linked into trusted Rust application | production use would move workbook parsing into WebView/JS |
| Financial authority | Rust validates strings and owns all rules | may parse cells only; never owns rules/authoritative numbers |
| License | Calamine MIT; rust_xlsxwriter MIT | ExcelJS MIT; JSZip MIT |
| Maintenance | two focused read/write libraries | broad object model plus npm transitive tree/override |
| Size | included in aggregate 12,525,568-byte executable | excluded from production dependencies/bundle |
| Spike conclusion | preferred adapter candidate | retained only as cross-contract benchmark |

This is an adapter conclusion, not a production-stack decision or ADR-0007
acceptance.

## Dependency and boundary budgets

There are **21 direct production dependencies**: 18 Rust target/runtime crates
and 3 npm packages. Rust crates are linked into the aggregate executable, so
the spike does not invent per-crate binary deltas.

| Dependency | Purpose | License | Boundary/alternative note |
|---|---|---|---|
| `aes-gcm 0.11.1` | backup AEAD | Apache-2.0 OR MIT | reviewed primitive; custom crypto rejected |
| `argon2 0.6.0` | password KDF | MIT OR Apache-2.0 | zeroized derived key |
| `base64 0.23.1` | envelope encoding | MIT OR Apache-2.0 | narrow encoding utility |
| `calamine 0.36.1` | known XLSX read | MIT | untrusted parser stays in Rust adapter |
| `getrandom 0.4.3` | salts/nonces/tokens | MIT OR Apache-2.0 | OS entropy |
| `rfd 0.17.2` | native file dialog | MIT | avoids general filesystem plugin |
| `rusqlite 0.40.2` | controlled SQLite | MIT | bundled/backup/functions; no SQL IPC |
| `rust_decimal 1.42.1` | authoritative decimal | MIT | binary float rejected |
| `rust_xlsxwriter 0.99.0` | standardized export | MIT | focused Rust writer |
| `serde 1.0.229` | typed DTOs | MIT OR Apache-2.0 | deny-unknown boundaries |
| `serde_json 1.0.151` | DTO/canonical encoding | MIT OR Apache-2.0 | canonicalizer remains local |
| `sha2 0.11.0` | fixture/file hashes | MIT OR Apache-2.0 | standard digest |
| `tauri 2.11.5` | desktop shell/IPC/WebView | Apache-2.0 OR MIT | dominant platform surface measured here |
| `thiserror 2.0.20` | typed errors | MIT OR Apache-2.0 | derives only |
| `unicode-normalization 0.1.25` | canonical text | MIT OR Apache-2.0 | cross-stack hash stability |
| `zeroize 1.9.0` | clear key material | Apache-2.0 OR MIT | explicit secret hygiene |
| `webview2-com 0.38.2` | memory target API | MIT | Windows-only, narrowly isolated adapter |
| `windows-core 0.61.2` | WebView COM support | MIT OR Apache-2.0 | Windows-only platform dependency |
| `@tauri-apps/api 2.11.1` | named JS invoke | Apache-2.0 OR MIT | no general plugins |
| `react 19.2.8` | view rendering | MIT | no state/chart framework |
| `react-dom 19.2.8` | DOM renderer | MIT | semantic HTML/CSS charts |

Other structural counts:

- Tauri plugins: **0** — PASS ≤ 8.
- Named privileged IPC commands: **12** — PASS ≤ 25.
- Runtime languages: Rust and TypeScript/JavaScript; no Node/Python sidecar.
- Approximate non-generated application/benchmark source: Rust 2,523 lines /
  11 files; TypeScript/TSX 343 lines / 5 files; CSS 51 lines.
- Logical layers: React view → named Tauri IPC → Rust application facade →
  domain/infrastructure adapters. SQLite never crosses the facade.
- No dedicated production dependency was added for expense charts; they use
  native HTML/CSS and a semantic table. Because no pre-expense package snapshot
  exists, the conditional page-increment metric is not established and is not
  needed to claim a hard-gate pass.

## Hard-gate matrix

| Gate | Evidence | Result |
|---|---|---|
| One installable desktop app; no server/service/daemon | per-user NSIS; app + WebView children; no listener/service/sidecar | PASS |
| Core functions usable offline | resolver-failure launch loaded SQLite/IPC view and exited normally | PASS |
| Thin package ≤ 30 MiB | 3.302 MiB | PASS |
| Installed payload ≤ 75 MiB | 12.021 MiB | PASS |
| Runtime-included package separately reported | 253.008 MiB | REPORTED |
| Cold start P95 ≤ 1.5 s | 1.024195 s | PASS |
| Idle full-tree RSS P95 ≤ 150 MB | 147,173,376 bytes / 147.173 MB | PASS |
| Residual tree after 10 s = 0 | 0 | PASS |
| Normal save/filter/page P95 ≤ 200 ms | write 4.0708; page 0.2596; query 1.7235; draw 110.3 | PASS |
| 100k account/timeline P95 ≤ 1 s | 5.5371 ms | PASS |
| 10k known-template import ≤ 10 s and UI does not freeze | Rust 66 ms; parsing on blocking pool; regression test | PASS |
| Current DB ≤ 20 MB | 102,400 bytes | PASS |
| 100k-event DB ≤ 100 MB | 54,038,528 bytes | PASS |
| Default runtime network = 0 | five-minute full-tree endpoints `[]` | PASS |
| Clean clone to tests + package ≤ 10 min | 8 min 27.326 s | PASS |
| Direct production dependencies ≤ 25 | 21 | PASS |
| First-load gzip ≤ 1.2 MiB | 64.45 KiB | PASS |
| Tauri plugins ≤ 8 | 0 | PASS |
| Named IPC ≤ 25 | 12 | PASS |
| M0 golden subset and canonical hashes | exact Rust tests + full M0 repository check | PASS |
| Install/start/close/uninstall actual | exit 0; process/registry/install dir cleared | PASS |
| 100k expense cold P95 ≤ 150 ms | 2.6740 ms | PASS |
| 100k expense warm P95 ≤ 50 ms | 2.1098 ms | PASS |
| Expense response ≤ 32 KiB; no N+1 | 17,698 bytes; one aggregation | PASS |

## Commands actually completed

```text
pwsh -NoProfile -File tools/check-m0-fixtures.ps1
npm ci
npm audit
npm run check
npm run bench:ts-xlsx
cargo fmt --manifest-path spikes/tauri/src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path spikes/tauri/src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path spikes/tauri/src-tauri/Cargo.toml --all-targets --all-features
cargo run --manifest-path spikes/tauri/src-tauri/Cargo.toml --release --example core_bench
pwsh -NoProfile -File spikes/tauri/tools/measure-runtime.ps1 ...
npm run tauri:build:standard
npm run tauri:build:offline
```

Final clean verification: Rust tests 10 passed; Vitest 2 files / 4 tests
passed; Clippy returned 0 under `-D warnings`; fixture reproducibility passed;
NSIS completed. MSVC printed a localized import-library creation line that
rustc labels `linker_messages`; it is informational linker stdout, not a source
warning or failed strict check.

## Remaining decisions and limitations

- M1-A passes, but the stack remains undecided until the Avalonia spike and
  selection task run under the same fixture and gates.
- ADR-0015 remains Proposed. A future Accepted decision is required before the
  expense projection can become production architecture.
- The `msOneAuthWAM` WebView2 feature switch and low-memory API must be retested
  whenever the WebView2 runtime or Tauri/Wry integration changes.
- Runtime-included installation on a pristine offline VM, signed package
  upgrade behavior, and code-signing operations are later release evidence;
  they are not silently claimed here.

Task 02 therefore qualifies for its protocol commit/fast-forward merge/push,
without a stack decision or M1 tag. Task 03 remains a separate serial stage.
