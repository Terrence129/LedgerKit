# Known issues — 1.0.0-beta.1

- Windows binaries are intentionally unsigned for this Beta, so SmartScreen or the installer may show an unknown-publisher warning. A code-signing certificate is a release gate for a broadly promoted stable build.
- The standard thin installer requires a working system Evergreen WebView2 runtime. An optional offline-runtime installer may be much larger and is measured separately.
- Updates are manual. The application does not check the network, download packages or self-update.
- The importer accepts only the versioned normalized workbook contract. A real v1.3.0 workbook requires a private, repository-external normalization and reconciliation step; arbitrary workbook inference is not supported.
- Automatic market prices/rates, cloud synchronization, mobile/web clients, managed attachment storage, SQLCipher and a total-fees expense view are outside P0.
- Locale scope is exactly `zh-CN` and `en-US`.
- Final private v1.3.0 reconciliation/cut-over, at least four weeks of dual entry including a full month cycle, and owner approval remain manual gates. They are not defects that may be bypassed by this Beta artifact.
