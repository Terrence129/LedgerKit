# Production dependency inventory

> Baseline: `1.0.0-beta.1` candidate, 2026-09-03. Exact versions are locked in `app/package-lock.json` and `app/src-tauri/Cargo.lock`.

The current application has **20 direct production dependencies** (3 npm + 17 Rust, including Windows-targeted crates), below the hard budget of 25. Build/test-only dependencies are excluded from that number and never ship as Node or Python sidecars. Rust crates are statically linked, so size evidence is recorded at the aggregate binary/package level rather than as misleading per-crate deltas.

The verified Beta Windows x64 release build produced a 14,974,976-byte application executable, a 4,016,436-byte standard thin NSIS installer, and a 15,054,105-byte installed application payload. First-load HTML/CSS/JS is 92,781 gzip bytes. The installer remains below the M1 hard budget and continues to reuse the system Evergreen WebView2 runtime; the separately measured optional offline-runtime NSIS is 265,918,362 bytes and never substitutes for the thin metric.

| Dependency | Purpose and boundary | Size evidence | License | Security and maintenance | Cost of not using it |
|---|---|---|---|---|---|
| `@tauri-apps/api 2.11.1` | Typed client for the allowlisted IPC boundary | Included in first-load gzip baseline | Apache-2.0 OR MIT | Official Tauri API; upgrade with Rust side and capability tests | Hand-maintained IPC transport |
| `react 19.2.8` | Setup/catalog page rendering and local UI state | Included in first-load gzip baseline | MIT | Mature UI runtime; no domain state | Custom renderer and lifecycle |
| `react-dom 19.2.8` | Local DOM rendering | Included in first-load gzip baseline | MIT | Paired with exact React version | Custom DOM integration |
| `tauri 2.11.5` | Desktop shell, local WebView and named IPC | Dominant part of measured aggregate app | Apache-2.0 OR MIT | Official stable line; every upgrade reruns M1 runtime gates | Build native shell/security boundary ourselves |
| `serde 1.0.229` | Strict IPC and settings DTO serialization | Aggregate Rust binary only | Apache-2.0 OR MIT | `deny_unknown_fields` on inbound DTOs | Error-prone manual parsing |
| `serde_json 1.0.151` | Strict settings/IPC JSON and canonical value tree before constrained serialization | Aggregate Rust binary only | Apache-2.0 OR MIT | Inbound DTOs deny unknown fields; canonical layer rejects unsupported JSON numbers | Custom JSON parser and serializer |
| `getrandom 0.4.3` | OS randomness for UUIDv7 plus independent backup salt, nonces, and data keys | Aggregate Rust binary only | Apache-2.0 OR MIT | Thin OS CSPRNG adapter with no custom entropy source | Unsafe timestamp/counter-only IDs or custom entropy code |
| `rusqlite 0.40.2` | Bundled SQLite driver, explicit transactions, Online Backup restore and controlled migration | Aggregate Rust binary only | MIT | Core-only parameterized access; no frontend SQL plugin | Hand-written SQLite FFI and consistency sequencing |
| `rust_decimal 1.42.1` | ADR-0004 checked Decimal coefficient arithmetic behind LedgerKit validation | Aggregate Rust binary only | MIT | No binary float; wrapper enforces scale/precision/error order | Implement and audit arbitrary-precision decimal arithmetic |
| `sha2 0.11.0` | SHA-256 for canonical posting and schema hashes | Aggregate Rust binary only | Apache-2.0 OR MIT | RustCrypto implementation; only non-secret hashing here | Hand-written cryptographic primitive |
| `unicode-normalization 0.1.25` | Unicode NFC normalization for canonical JSON v1 | Aggregate Rust binary only | Apache-2.0 OR MIT | Narrow canonicalization boundary | Cross-stack hashes differ for canonically equivalent text |
| `webview2-com 0.38.2` | Windows-only low-memory target adapter | Aggregate Rust binary only | MIT | One reviewed unsafe block; WebView upgrades require remeasurement | Lose measured RSS control |
| `windows-core 0.61.2` | Windows COM interface support for the adapter | Aggregate Rust binary only | Apache-2.0 OR MIT | Isolated under `platform`; no Domain dependency | Hand-written COM ABI |
| `calamine 0.36.1` | Read-only parser for the allowlisted `.xlsx` template behind the Infrastructure adapter | Included in measured M3 aggregate delta | MIT | No formula execution; strict file/sheet/row/column/cell limits, macro/external-link rejection, and cached-value validation | Hand-written ZIP/XML parser with a larger audit surface |
| `rust_xlsxwriter 0.99.0` | Deterministic synthetic fixtures and standalone normalized XLSX export | Included in measured aggregate app | MIT | Export writes user text as strings rather than formulas; fixtures remain byte-stable | Maintain raw OOXML generation and packaging ourselves |
| `rfd 0.17.2` | One-use native import, backup, restore, and export pickers on Tauri's blocking pool | Included in measured aggregate app | MIT | Paths remain inside Rust and are revalidated by Core; WebView has no filesystem capability | Add a broader filesystem/dialog plugin or platform-specific dialog code |
| `argon2 0.6.0` | ADR-0008 fixed Argon2id v19 password KDF for portable backups | Included in measured M6 aggregate delta | Apache-2.0 OR MIT | Parameters are allowlisted before allocation; password-derived key is zeroized | Implement or vendor a password KDF |
| `aes-gcm 0.11.1` | AES-256-GCM authenticated wrapping and payload encryption | Included in measured M6 aggregate delta | Apache-2.0 OR MIT | RustCrypto primitive with independent random nonces and header AAD | Implement an unauditable encryption primitive |
| `base64 0.23.1` | Standard binary encoding inside the versioned JSON backup envelope | Included in measured M6 aggregate delta | Apache-2.0 OR MIT | Strict decoded lengths and authenticated content | Create a bespoke binary-to-text format |
| `zeroize 1.9.0` | Clears backup passwords, KEKs and data keys on every drop/error path | Included in measured M6 aggregate delta | Apache-2.0 OR MIT | Narrow secret-buffer lifecycle; no secret persistence | Leave secret buffers to ordinary allocator reuse |

## Build and test dependencies

`tauri-build 2.6.3`, Tauri CLI 2.11.4, Vite 8.2.2, TypeScript 7.0.2, Vitest 4.1.11, the React Vite plugin and type packages are locked development/build inputs. They are maintained only for the build pipeline and are not runtime services. CI runs `npm ci`, Rust lockfile resolution, strict TypeScript, rustfmt, Clippy with warnings denied, unit tests, privacy checks and the dependency budget.

## Accepted adapters

All dependencies currently authorized by Accepted P0 ADRs are present and used. Any additional runtime dependency still requires a manifest diff, license/security review, updated aggregate package measurement and this inventory update.
