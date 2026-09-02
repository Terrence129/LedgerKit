# Production dependency inventory

> Baseline: M1 selected Tauri scaffold, 2026-09-02. Exact versions are locked in `app/package-lock.json` and `app/src-tauri/Cargo.lock`.

The current scaffold has **8 direct production dependencies** (3 npm + 5 Rust, including Windows-targeted crates), below the hard budget of 25. Build/test-only dependencies are excluded from that number and never ship as Node or Python sidecars. Rust crates are statically linked, so the M1 measurements report aggregate binary/package size rather than misleading per-crate deltas.

| Dependency | Purpose and boundary | Size evidence | License | Security and maintenance | Cost of not using it |
|---|---|---|---|---|---|
| `@tauri-apps/api 2.11.1` | Typed client for two allowlisted IPC calls | Included in first-load gzip baseline | Apache-2.0 OR MIT | Official Tauri API; upgrade with Rust side and capability tests | Hand-maintained IPC transport |
| `react 19.2.8` | Health-page rendering and local UI state | Included in first-load gzip baseline | MIT | Mature UI runtime; no domain state | Custom renderer and lifecycle |
| `react-dom 19.2.8` | Local DOM rendering | Included in first-load gzip baseline | MIT | Paired with exact React version | Custom DOM integration |
| `tauri 2.11.5` | Desktop shell, local WebView and named IPC | Dominant part of measured aggregate app | Apache-2.0 OR MIT | Official stable line; every upgrade reruns M1 runtime gates | Build native shell/security boundary ourselves |
| `serde 1.0.229` | Strict IPC and settings DTO serialization | Aggregate Rust binary only | Apache-2.0 OR MIT | `deny_unknown_fields` on inbound DTOs | Error-prone manual parsing |
| `serde_json 1.0.151` | Minimal versioned settings file | Aggregate Rust binary only | Apache-2.0 OR MIT | Only non-financial settings in M1 | Custom JSON codec |
| `webview2-com 0.38.2` | Windows-only low-memory target adapter | Aggregate Rust binary only | MIT | One reviewed unsafe block; WebView upgrades require remeasurement | Lose measured RSS control |
| `windows-core 0.61.2` | Windows COM interface support for the adapter | Aggregate Rust binary only | Apache-2.0 OR MIT | Isolated under `platform`; no Domain dependency | Hand-written COM ABI |

## Build and test dependencies

`tauri-build 2.6.3`, Tauri CLI 2.11.4, Vite 8.2.2, TypeScript 7.0.2, Vitest 4.1.11, the React Vite plugin and type packages are locked development/build inputs. They are maintained only for the build pipeline and are not runtime services. CI runs `npm ci`, Rust lockfile resolution, strict TypeScript, rustfmt, Clippy with warnings denied, unit tests, privacy checks and the dependency budget.

## Accepted but deferred adapters

The following exact dependencies are approved by Accepted ADRs but are deliberately absent until the stage that implements their port. They do not count in the current 8-dependency baseline:

- ADR-0007: `calamine 0.36.1`, `rust_xlsxwriter 0.99.0`.
- ADR-0008: `argon2 0.6.0`, `aes-gcm 0.11.1`, `getrandom 0.4.3`, `zeroize 1.9.0`, `base64 0.23.1`.
- M0/M2 requirements, already measured in the Tauri spike but still subject to the implementing stage: `rusqlite 0.40.2`, `rust_decimal 1.42.1`, `sha2 0.11.0`, `unicode-normalization 0.1.25`.

Adding any of these still requires a manifest diff, license/security review, updated aggregate package measurement and this inventory update. Approval is not permission to introduce an unused dependency.
