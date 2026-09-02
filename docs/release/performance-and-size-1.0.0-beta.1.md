# LedgerKit 1.0.0-beta.1 performance and size

> Measurement host: Windows x64, local SSD, pinned Node 24.16.0/npm 11.13.0/Rust 1.98.0. Values below are candidate evidence; private workbook data is never used in a public benchmark.

## Automated 100k synthetic-event gate

Release-mode output:

| Metric | Result | Hard gate |
|---|---:|---:|
| Synthetic import/load | 6,829 ms | ≤ 10,000 ms |
| SQLite database | 90,222,592 bytes | ≤ 100,000,000 bytes |
| Expense cold query | 0 ms | ≤ 150 ms |
| Expense warm P95 | 0 ms | ≤ 50 ms |
| Expense IPC serialization path | 0 ms | ≤ 200 ms |
| Expense response | 2,511 bytes | ≤ 32 KiB |
| Activity filter P95 | 1 ms | ≤ 200 ms |
| Activity page P95 | 1 ms | ≤ 200 ms |
| Activity response | 14,912 bytes | ≤ 32 KiB |
| Regular save P95 | 4 ms | ≤ 200 ms |
| Expense UI render P95, 30 samples | 9.501 ms | ≤ 200 ms |

The test owns every threshold assertion and fails the candidate rather than rounding a failure into the report. Values are wall-clock observations on this host; zero means below the timer's 1 ms resolution.

## Candidate package and runtime

| Metric | Result | Hard gate |
|---|---:|---:|
| Standard thin NSIS | 4,016,436 bytes; SHA-256 `9D77A59E893BE4306E2497815C968B7ED2BAF0CDDEEA3FA6FC5D1550BFD561B0` | ≤ 30 MiB |
| Clean-clone thin NSIS | 4,017,182 bytes; SHA-256 `3AAE5B186BBE17394BE6232D1AC90015EC3D88D230D4F3079D4D5F66C0209016` | ≤ 30 MiB |
| Installed application payload | 15,054,105 bytes | ≤ 75 MiB |
| Application executable | 14,974,976 bytes | Informational |
| Optional WebView2-offline NSIS | 265,918,362 bytes; SHA-256 `C19C8DC8BD5C65256C8E1314BE53AABF41EB608BAB510BDCB1C65F0E57CB02B8` | Report separately |
| Cold start P95, 30 samples | 912.588 ms; max 915.882 ms | ≤ 1,500 ms |
| Five-minute full-tree idle RSS P95, final 30 s | 132,734,976 bytes (126.586 MiB) | ≤ 150,000,000 bytes |
| Peak full-tree RSS | 335,208,448 bytes | Informational; startup is not idle |
| Default remote endpoints | `[]` | 0 |
| LedgerKit foreground samples during idle | 0 | 0 |
| Residual processes 10 s after normal/idle exit | 0 / 0 | 0 |
| Installer lifecycle | install 0, main window ready, normal close, uninstall 0, directory removed | Pass |
| Authenticode | `NotSigned` | Expected Beta/manual certificate gate |
| Clean clone to checks/tests/package | 562.775 s | ≤ 600 s |

Cold readiness requires a responsive native window plus the complete seven-process WebView tree, rather than counting window creation alone. Idle RSS is the sum of the application and every descendant; the process is deliberately unfocused, every sample verifies it did not regain the foreground, and P95 uses only samples timestamped in the final 30 seconds of the five-minute run. The initial peak is retained rather than hidden. TCP evidence includes established non-loopback endpoints owned by the process tree; Tauri's local IPC loopback is not a remote request.

The standard installer was actually installed, launched, closed and uninstalled. The optional offline-runtime package was built and measured but not installed on a pristine offline VM, so no offline installed-footprint claim is made. Rebuilding NSIS may change metadata and therefore the byte hash; size/lifecycle are the reproducibility gates, while each distributed artifact must publish its own hash.

## Budgets

The candidate has 20 direct production dependencies, 0 Tauri plugins, 25 named privileged operations and 92,781 bytes gzip for all first-load HTML/CSS/JS. Static gates require at most 25, 8, 25 and 1.2 MiB respectively. The standard bundle uses system WebView2 and per-user NSIS.

The clean-clone run checked commit `f3e4ba1aeba4ff5372da62d0ac3b3f15c40f70c3`. It recovered locked dependencies, ran the complete check suite including the ignored 100k gate, regenerated both deterministic XLSX fixture sets and built NSIS in 562.775 seconds. Its 100k observation was 7,903 ms load, 90,222,592 database bytes, 0 ms cold query, 0 ms warm P95, 1 ms filter/page P95 and 5 ms save P95. The check pipeline deliberately reuses one Release build for tests, the benchmark, Clippy metadata and fixture generation; it does not remove any gate to meet the 10-minute target.
