import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { gzipSync } from "node:zlib";

const repositoryRoot = resolve(import.meta.dirname, "..");
const appRoot = join(repositoryRoot, "app");
const failures = [];
const expectedVersion = "1.0.0-beta.2";

function fail(message) { failures.push(message); }
function read(path) { return readFileSync(path, "utf8"); }
function json(path) { return JSON.parse(read(path)); }
function walk(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? walk(path) : [path];
  });
}

const packageJson = json(join(appRoot, "package.json"));
const packageLock = json(join(appRoot, "package-lock.json"));
const tauriConfig = json(join(appRoot, "src-tauri/tauri.conf.json"));
const cargoToml = read(join(appRoot, "src-tauri/Cargo.toml"));
const cargoLock = read(join(appRoot, "src-tauri/Cargo.lock"));
const versions = [
  ["package.json", packageJson.version],
  ["package-lock root", packageLock.version],
  ["package-lock package", packageLock.packages?.[""]?.version],
  ["tauri.conf.json", tauriConfig.version],
  ["Cargo.toml", cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1]],
  ["Cargo.lock", cargoLock.match(/name = "ledgerkit-desktop"\r?\nversion = "([^"]+)"/)?.[1]],
];
for (const [source, version] of versions) if (version !== expectedVersion) fail(`${source} version is ${version ?? "missing"}`);

if (tauriConfig.bundle?.targets?.length !== 1 || tauriConfig.bundle.targets[0] !== "nsis") fail("Beta package target must be NSIS only");
if (tauriConfig.bundle?.windows?.nsis?.installMode !== "currentUser") fail("NSIS must remain per-user");
if (tauriConfig.bundle?.windows?.webviewInstallMode?.type !== "skip") fail("standard installer must reuse system WebView2");
if (tauriConfig.app?.windows?.some((window) => "url" in window)) fail("production window must not load remote content");
if (tauriConfig.plugins?.updater || tauriConfig.bundle?.createUpdaterArtifacts) fail("updater configuration found");
if (/^https?:/i.test(tauriConfig.build?.frontendDist ?? "")) fail("production frontend must be a bundled local directory");

const healthHome = read(join(appRoot, "src/ui/HealthHome.tsx"));
const expectedNavigation = '["overview", "activity", "assets", "quality", "settings"]';
if (!healthHome.includes(expectedNavigation)) fail("the exact five reviewed top-level entries are not present");
const css = read(join(appRoot, "src/ui/styles.css"));
for (const marker of [":focus-visible", "@media (max-width:", "@media (prefers-reduced-motion: reduce)", "@media (forced-colors: active)"]) {
  if (!css.includes(marker)) fail(`accessibility/responsive CSS marker missing: ${marker}`);
}

const en = json(join(appRoot, "src/ui/i18n/resources/en-US.json"));
const zh = json(join(appRoot, "src/ui/i18n/resources/zh-CN.json"));
for (const view of ["overview", "activity", "assets", "quality", "settings"]) {
  if (!en[`nav.${view}`] || !zh[`nav.${view}`]) fail(`bilingual navigation resource missing for ${view}`);
}
if (JSON.stringify(Object.keys(en).sort()) !== JSON.stringify(Object.keys(zh).sort())) fail("locale resource key sets differ");

const schema = read(join(appRoot, "src-tauri/src/infrastructure/sqlite/schema.rs"));
if (!schema.includes("pub const SCHEMA_VERSION: u32 = 7;")) fail("Beta schema must be v7");
for (const redundant of ["idx_business_events_activity", "idx_cash_event_fees_event", "idx_income_expense_category"]) {
  if (schema.includes(redundant)) fail(`redundant 100k index remains in current schema: ${redundant}`);
}

const capability = json(join(appRoot, "src-tauri/capabilities/main.json"));
const privileged = capability.permissions.filter((permission) => permission.startsWith("allow-"));
if (privileged.length !== 25) fail(`reviewed privileged operation count changed: ${privileged.length}/25`);
const allSource = walk(join(appRoot, "src-tauri/src")).filter((path) => path.endsWith(".rs")).map((path) => read(path)).join("\n");
const securityEvidence = [
  "command_dtos_reject_arbitrary_paths_and_numeric_financial_values",
  "portable_backup_round_trip_rejects_wrong_password_tamper_and_unknown_contracts",
  "standalone_exports_are_complete_formula_safe_and_diagnostics_are_redacted",
  "local_data_policy_rejects_relative_and_synchronized_roots",
  "confirmation_and_batch_authorization_fail_closed",
];
for (const test of securityEvidence) if (!allSource.includes(test)) fail(`security negative test missing: ${test}`);
for (const forbidden of [[/\bTcpListener\b/, "listener"], [/\bCommand::new\b/, "shell command"], [/\btauri_plugin_sql\b/, "arbitrary SQL plugin"]]) {
  if (forbidden[0].test(allSource)) fail(`forbidden runtime surface found: ${forbidden[1]}`);
}

const requiredDocs = [
  "docs/user-guide.md",
  "docs/import-guide.md",
  "docs/operations/backup-restore.md",
  "docs/operations/upgrade-rollback.md",
  "docs/release/beta-audit-1.0.0-beta.1.md",
  "docs/release/performance-and-size-1.0.0-beta.1.md",
  `docs/release/known-issues-${expectedVersion}.md`,
  `docs/release/${expectedVersion}.md`,
  "docs/release/sbom.cdx.json",
  "docs/release/licenses.md",
];
if (existsSync(join(repositoryRoot, "docs"))) {
  for (const path of requiredDocs) if (!existsSync(join(repositoryRoot, path))) fail(`release artifact missing: ${path}`);
}

const distFiles = walk(join(appRoot, "dist")).filter((path) => /\.(?:html|css|js)$/.test(path));
const firstLoadGzipBytes = distFiles.reduce((total, path) => total + gzipSync(readFileSync(path)).length, 0);
if (firstLoadGzipBytes > 1.2 * 1024 * 1024) fail(`first-load gzip budget exceeded: ${firstLoadGzipBytes}`);

if (failures.length) {
  console.error(failures.join("\n"));
  process.exitCode = 1;
} else {
  console.log(`BETA_CHECK=PASS version=${expectedVersion} schema=7 locales=2 nav=5 ipc=${privileged.length} first_load_gzip_bytes=${firstLoadGzipBytes}`);
}
