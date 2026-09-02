# M1 Avalonia vertical spike report

> Measured 2026-09-02 on branch `phase/m1-avalonia-spike`, starting from
> `7315391ed2f190e4b481b8353fd61bb3a3007da5` (`origin/main`).
>
> **Overall result: PASS for the Avalonia M1-B spike using the deployable
> Native AOT candidate.** Every M1 hard gate measured by task 03 passes on this
> machine. The framework-dependent runtime is retained as a failed cold-start
> comparison. This result does not select the production stack, accept
> ADR-0001/ADR-0015, tag M1, or remove either spike.

## Scope and interpretation

This is a disposable .NET 10 + Avalonia sample with a single authoritative C#
Core and one desktop process. It has no server, sidecar, daemon, WebView, chart
library, ORM, database plugin, or additional state framework. The UI receives
named view models and decimal strings through 12 in-process facade operations;
it cannot submit SQL, postings, shell commands, remote URLs, or arbitrary file
destinations.

The first framework-dependent measurement failed the 1.5-second cold-start
gate at 2,845.878 ms P95. No baseline was changed. A true Native AOT publish was
then compiled and measured at 849.659 ms P95, while remaining below the package
and installed-payload gates. The report therefore treats AOT as the passing
deployable Avalonia candidate and preserves the framework-dependent result as
comparison evidence.

## Locked versions and official basis

Versions are centrally pinned in `Directory.Packages.props`, restored with
lock files, and compiled using `global.json`.

| Component | Locked/measured version | Note |
|---|---:|---|
| .NET SDK / runtime | 10.0.400 / 10.0.11 | Windows x64; C# 14 |
| Avalonia Desktop / Fluent theme | 12.1.1 / 12.1.1 | one native desktop UI process |
| Microsoft.Data.Sqlite | 10.0.11 | bundled SQLite provider; SQLite 3.53.3 measured |
| DocumentFormat.OpenXml | 3.5.1 | known-template read and standardized export |
| NSIS | 3.11 | current-user installer, unsigned |

The locked versions match the stable NuGet releases for
[Avalonia.Desktop](https://www.nuget.org/packages/Avalonia.Desktop/),
[Microsoft.Data.Sqlite](https://www.nuget.org/packages/Microsoft.Data.SQLite/),
and [DocumentFormat.OpenXml](https://www.nuget.org/packages/documentformat.openxml/).
Avalonia's Windows deployment guidance permits framework-dependent,
self-contained, trimmed, and Native AOT deployment, while noting the runtime
trade-offs; this spike measures each applicable shape rather than treating
them as interchangeable. See [Avalonia Windows deployment](https://docs.avaloniaui.net/xpf/deployment/windows),
[.NET publishing overview](https://learn.microsoft.com/en-us/dotnet/core/deploying/),
[trimming](https://learn.microsoft.com/en-us/dotnet/core/deploying/trimming/trim-self-contained),
and [Native AOT](https://learn.microsoft.com/en-us/dotnet/core/deploying/native-aot/).

## Machine and method

- Windows 11 Home Chinese x64, build 26200.
- AMD Ryzen 7 H 255, 8 cores / 16 logical processors.
- 32 GiB RAM and YMTC PC411-1TB-B SSD.
- Installed .NET 10.0.11 runtime used by the framework-dependent candidate.

Cold start is process launch until the complete Avalonia overview writes a
ready signal. Thirty isolated synthetic-data runs retain the first value and
verify zero residual processes. Idle RSS is the root plus every descendant,
sampled for at least five minutes; the gate uses nearest-rank P95 over the
actual final 30 seconds. Established TCP connections are sampled for that same
process tree.

Core benchmarks run Release code against deterministic synthetic data and use
nearest-rank P95. The measured empty migration is preceded by one separately
reported CLR/JIT warm-up database, because cold application/JIT cost is already
captured by the 30-process startup metric. Each cold expense query opens a new
SQLite connection; the OS cache is not flushed. Package sizes are exact bytes;
MiB divides by 1,048,576.

The clean build ran after `dotnet clean` and includes locked restore, format
verification, warnings-as-errors build, 10 spike checks, the complete M0
fixture validator, framework-dependent publish, self-contained trimmed
publish, true Native AOT compilation, and both NSIS packages.

## Implemented slice and safety evidence

- Schema v2 opens through the Core only. A pre-migration backup is created,
  WAL/foreign keys/integrity are checked, and projection versions/watermarks
  are explicit.
- A high-level event command validates decimal strings and derives event,
  posting, balance/expense projection, and watermarks in one SQLite
  transaction. A failpoint proves complete rollback.
- The expense daily/category projection is rebuildable from facts and yields
  one canonical query result for KPI, 12 buckets, Top 10 + Other, bars, table,
  and drill-down context.
- The UI performs XLSX analysis through `Task.Run`; Open XML reads the shared
  template forward-only and preserves date/currency/amount strings. Microsoft's
  [large spreadsheet guidance](https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/how-to-parse-and-read-a-large-spreadsheet)
  supports the SAX-style approach used here.
- File selection creates a one-use in-memory token. Attachment copy accepts a
  regular file of at most 5 MiB, chooses its own hash-addressed managed path,
  and rejects token replay and kind confusion.
- Backup envelopes use built-in PBKDF2-HMAC-SHA256 (600,000 iterations) and
  AES-256-GCM with unique salt/nonce. Tests cover wrong passwords, plaintext
  absence, and rollback-safe restore. This deliberately avoids another spike
  dependency; Argon2id versus platform/built-in KDF remains a later ADR choice.
- Source tests reject `HttpClient`, WebSocket, and process launch APIs. The app
  has no network package or network feature.

## Correctness and shared-fixture evidence

The unchanged M0 repository check passed:

```text
M0 fixture generation is reproducible.
Validated 31 fixture groups, 56 financial rules, 211 canonical hashes and
25 expense query results. Validated 186 JSON files against schemas.
Validated 8 Accepted ADRs and synchronized M0 status documents.
```

The C# slice reproduces M0 fixture 01 normal and boundary events, postings, and
projection fields. Its posting sequence hash is
`sha256:ec8b3e18aaedd6f64618e8667b619d7e61de27f68e41e499e325b86cd43936f2`.
The expense query/rebuild hash is
`sha256:7cd365ef12db020eb178975704fd2388cad37b5a4f378c6debf5e3aef27a8beb`.
Failure fixtures and the synthetic transaction interruption produce no partial
facts, postings, projections, or watermark movement.

The shared XLSX remains exactly 10,000 synthetic rows with SHA-256
`d7bbf52a86d2655ec09fe82fa42690f1c9e7aad6d323c6f167a86c797c024bd5`.
No candidate-private input or expected answer was introduced.

## Raw measurements

### Package and installed sizes

| Measurement | Exact result | Gate result |
|---|---:|---|
| Framework-dependent thin NSIS | 10,451,794 bytes (9.968 MiB), SHA-256 `003562601ECE5846BCAF8ADF2A044D960F1E13C148D52A78E4D4E393CAA779C0` | PASS ≤ 30 MiB; stretch PASS ≤ 20 MiB |
| Framework-dependent published files | 39,139,154 bytes (37.326 MiB) | informational; requires installed .NET 10 runtime |
| Framework-dependent installed payload | 39,177,934 bytes (37.363 MiB) | PASS ≤ 75 MiB; stretch PASS ≤ 50 MiB |
| Self-contained partially trimmed files | 119,339,809 bytes (113.811 MiB) | reported separately; runnable smoke passed |
| Native AOT NSIS | 17,675,474 bytes (16.857 MiB), SHA-256 `B81BDC623A5D8416C9AE129C9D5E3036CBA15ED18CD76E2F27FBC5B6DD7C863C` | PASS ≤ 30 MiB; stretch PASS ≤ 20 MiB |
| Native AOT published files | 67,966,504 bytes (64.818 MiB) | self-contained deployable candidate |
| Native AOT installed payload | 68,005,284 bytes (64.855 MiB) | PASS ≤ 75 MiB; stretch FAIL ≤ 50 MiB |

Both final installers were executed. Silent install/uninstall returned 0,
the app reported ready, `CloseMainWindow()` returned true, the process exited
within 10 seconds, and the per-user install directory was absent after
uninstall. Packages are unsigned because no production certificate was
provided.

Native Skia/HarfBuzz `.pdb` files accounted for 104,951,808 bytes in the first
publish and are excluded from release payloads. They are debugging symbols,
not runtime code or financial functionality.

### Cold start and rendering (Native AOT, milliseconds)

Cold wall-to-interactive raw values:

```text
891.593, 849.659, 709.290, 708.498, 707.485, 704.336, 672.924,
693.811, 696.852, 695.110, 752.689, 707.821, 703.186, 708.302,
703.630, 700.632, 702.799, 706.259, 667.429, 695.272, 699.384,
693.290, 673.413, 670.849, 668.997, 671.033, 681.265, 690.964,
684.983, 706.683
```

P95 is **849.659 ms**: hard PASS ≤ 1.5 s; stretch PASS ≤ 1.0 s. The
first run is retained. Avalonia first-render P95 is **667.381 ms** and the
expense bars/table first-draw P95 is **91.178 ms**. All 30 runs left zero
descendants.

The framework-dependent comparison used the same method and measured
**2,845.878 ms P95 (FAIL)**. Its first-render cost was dominated by CLR/JIT and
framework initialization; it is not used to claim the passing startup gate.

### Five-minute process-tree and default-network measurement (Native AOT)

- Requested idle: 300 s; actual: 300,392.410 ms; samples: 302.
- Final-30-second RSS P95: **95,350,784 bytes = 95.351 MB = 90.934 MiB** —
  hard PASS ≤ 150 MB; stretch PASS ≤ 100 MB.
- Peak process-tree RSS: **95,567,872 bytes (91.141 MiB)**.
- End tree: one Avalonia application process; no child process or service.
- Established remote endpoints over the complete run: `[]` — PASS.
- Residual process count 10 seconds after test termination: 0 — PASS.

The automation window is intentionally removed from the taskbar, so Windows
does not expose it through `Process.MainWindowHandle` and the runtime script's
`CloseMainWindow()` returned false before it used its bounded force cleanup.
Normal graceful close is evidenced separately by both visible installed-app
tests, where `CloseMainWindow()` returned true. This distinction is reported
rather than rewritten as a graceful measurement close.

An additional process-scoped offline test set all standard HTTP(S) proxies to
the dead endpoint `127.0.0.1:1`, loaded the local SQLite view, observed for 10
seconds, and recorded endpoints `[]` and zero residuals. The same smoke passed
for the self-contained trimmed output. The Windows session was not elevated,
so no machine firewall or adapter setting was changed; the no-network source
check and full five-minute endpoint trace are the stronger default-network
evidence.

### C# Core and SQLite (milliseconds unless noted)

| Measurement | Result | Gate result |
|---|---:|---|
| One-time CLR/JIT database warm-up (separate) | 131.5169 | framework-dependent context only |
| Empty open + schema migration after warm-up | 23.4042 | PASS ≤ 200 ms |
| Open XML 10k XLSX import | 1,759.6224 | PASS ≤ 10 s |
| Current event write P95 | 3.9664 | PASS ≤ 200 ms |
| Current activity page P95 | 0.2547 | PASS ≤ 200 ms |
| Standardized XLSX export, 43 rows | 69.8477 | PASS ≤ 200 ms |
| Current expense query P95 | 1.6510 | PASS ≤ 200 ms |
| Current synthetic DB size | 102,400 bytes | PASS ≤ 20 MB |
| Generate 100k events + projection | 4,017.3205 | setup metric |
| 100k-event DB size | 54,104,064 bytes (51.598 MiB) | PASS ≤ 100 MB |
| 100k timeline query P95 | 5.6459 | PASS ≤ 1 s |
| 100k expense fresh-connection P95 | 2.6989 | PASS ≤ 150 ms |
| 100k expense warm P95 | 1.8187 | PASS ≤ 50 ms |
| Expense result JSON | 17,698 bytes | PASS ≤ 32 KiB |

One projection-backed aggregation creates the complete expense result. There
is no per-bucket/N+1 query. Projection deletion and rebuild preserve the exact
canonical hash.

### Clean build and dependencies

- Final clean restore + all checks + FDD/trimmed/AOT + two NSIS packages:
  **242,271.743 ms (4 min 2.272 s)** — hard PASS ≤ 10 min; stretch PASS ≤ 5 min.
- Direct production packages: **4** — PASS ≤ 25.
- `dotnet list package --vulnerable --include-transitive`: no vulnerable
  packages reported by the configured sources on 2026-09-02.
- Non-generated C#/project source: 3,657 lines / 18 files across application,
  checks, and benchmarks.
- Web frontend/first-load gzip: not applicable; the native Avalonia process has
  no HTML/CSS/JavaScript bundle. No web payload is hidden from the package
  sizes above.

| Direct dependency | Purpose | License | Boundary/alternative note |
|---|---|---|---|
| `Avalonia.Desktop 12.1.1` | native desktop platform/UI | MIT | dominant UI/runtime surface measured here |
| `Avalonia.Themes.Fluent 12.1.1` | built-in visual theme | MIT | no additional design system/state framework |
| `Microsoft.Data.Sqlite 10.0.11` | controlled SQLite access | MIT | only Core references it; no SQL UI surface |
| `DocumentFormat.OpenXml 3.5.1` | XLSX read/write | MIT | untrusted parser isolated behind Core adapter |

Final trimmed and AOT publishes emitted no warnings. The first true AOT
attempt correctly failed warnings-as-errors with IL2026/IL3050 for generic
`JsonArray.Add<T>` dynamic-code paths. Those calls were changed to explicit
`JsonNode` overloads; all golden checks then passed and the next AOT build
reached `Generating native code` with no warning suppression. Open XML is the
largest reflection-sensitive surface; the normal Core import/export tests and
trimmed app smoke pass, but production AOT upgrades must continue running
trimmer/AOT analysis and XLSX fixture coverage.

## Hard-gate matrix

| Gate | Evidence | Result |
|---|---|---|
| One installable desktop app; no server/service/daemon | per-user NSIS; one application process; no listener/sidecar | PASS |
| Core functions usable offline | dead-proxy local launch + no network API + five-minute endpoints `[]` | PASS |
| Thin package ≤ 30 MiB | FDD 9.968 MiB; AOT 16.857 MiB | PASS |
| Installed payload ≤ 75 MiB | FDD 37.363 MiB; AOT 64.855 MiB | PASS |
| Runtime-included package separately reported | trimmed 113.811 MiB; AOT 64.818 MiB | REPORTED |
| Cold start P95 ≤ 1.5 s | AOT 0.849659 s; FDD 2.845878 s retained as fail comparison | PASS (AOT) |
| Idle full-tree RSS P95 ≤ 150 MB | 95,350,784 bytes / 95.351 MB | PASS |
| Residual tree after 10 s = 0 | 0 | PASS |
| Normal save/filter/page P95 ≤ 200 ms | write 3.9664; page 0.2547; query 1.6510; draw 91.178 | PASS |
| 100k account/timeline P95 ≤ 1 s | 5.6459 ms | PASS |
| 10k known-template import ≤ 10 s and UI does not freeze | 1,759.6224 ms; worker-thread boundary source test | PASS |
| Current DB ≤ 20 MB | 102,400 bytes | PASS |
| 100k-event DB ≤ 100 MB | 54,104,064 bytes | PASS |
| Default runtime network = 0 | five-minute full-tree endpoints `[]` | PASS |
| Clean clone to tests + package ≤ 10 min | 4 min 2.272 s | PASS |
| Direct production dependencies ≤ 25 | 4 | PASS |
| First-load gzip ≤ 1.2 MiB | no web bundle / 0 web bytes | PASS by architecture |
| Named privileged operations ≤ 25 | 12 in-process facade methods | PASS |
| M0 golden subset and canonical hashes | exact C# checks + full M0 repository check | PASS |
| Install/start/close/uninstall actual | both NSIS variants exit 0; close true; directories removed | PASS |
| 100k expense cold P95 ≤ 150 ms | 2.6989 ms | PASS |
| 100k expense warm P95 ≤ 50 ms | 1.8187 ms | PASS |
| Expense response ≤ 32 KiB; no N+1 | 17,698 bytes; one aggregation | PASS |

## Commands actually completed

```text
pwsh -NoProfile -File tools/check.ps1
dotnet run --project benchmarks/LedgerKit.AvaloniaSpike.Benchmarks -c Release --no-build
dotnet list ... package --include-transitive
dotnet list ... package --vulnerable --include-transitive
pwsh -NoProfile -File tools/build-packages.ps1 -AttemptNativeAot
pwsh -NoProfile -File tools/test-installer.ps1 -Installer <FDD NSIS>
pwsh -NoProfile -File tools/test-installer.ps1 -Installer <AOT NSIS>
pwsh -NoProfile -File tools/measure-runtime.ps1 -Executable <FDD exe> -ColdRuns 30 -IdleSeconds 300
pwsh -NoProfile -File tools/measure-runtime.ps1 -Executable <AOT exe> -ColdRuns 30 -IdleSeconds 300
pwsh -NoProfile -File tools/test-offline-core.ps1 -Executable <AOT/trimmed exe>
```

Final source verification built with 0 warnings and 0 errors, passed 10/10
spike checks, and passed the full M0 fixture/ADR synchronization check.

## Comparison implications and limitations

- Avalonia's single C# language across UI and Core reduces cross-language DTO,
  tooling, and ownership overhead. It also avoids a WebView runtime and its
  version-sensitive network/memory controls.
- The passing startup result depends on Native AOT. Framework-dependent Avalonia
  failed cold start, while AOT adds compiler/linker constraints and makes
  reflection-heavy dependencies such as Open XML an ongoing verification cost.
- The AOT installed payload (64.855 MiB) passes the hard gate but is materially
  larger than the Tauri thin payload; the framework-dependent option instead
  depends on a separately installed matching .NET runtime.
- Avalonia preserves a macOS/Linux path, but this spike's RID, installer, and
  automation are Windows-specific. Reversing to other platforms requires
  platform packaging, signing, file-dialog, font/rendering, and Native AOT
  validation; one language does not remove those costs.
- Native AOT XLSX operations were covered by compile-time analysis and the
  shared normal Core suite, not by UI automation that clicks a file picker in
  the AOT binary. This should be added before an AOT production release.
- Signed installation, upgrade/rollback, pristine offline VM deployment, and
  production backup KDF choice remain later release/ADR evidence.
- ADR-0015 remains Proposed. The spike proves a rebuildable expense projection
  is feasible but does not make it authoritative.

Task 03 therefore qualifies for its protocol commit/fast-forward merge/push.
Task 04 must compare this report with `tauri.md` and make the explicit stack
decision; this report alone does not accept ADR-0001 or tag M1.
