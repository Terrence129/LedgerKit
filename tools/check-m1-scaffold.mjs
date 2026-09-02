import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { gzipSync } from "node:zlib";

const repositoryRoot = resolve(import.meta.dirname, "..");
const appRoot = join(repositoryRoot, "app");
const failures = [];

function fail(message) {
  failures.push(message);
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function walkFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? walkFiles(path) : [path];
  });
}

const enUS = readJson(join(appRoot, "src/ui/i18n/resources/en-US.json"));
const zhCN = readJson(join(appRoot, "src/ui/i18n/resources/zh-CN.json"));
const enKeys = Object.keys(enUS).sort();
const zhKeys = Object.keys(zhCN).sort();
if (JSON.stringify(enKeys) !== JSON.stringify(zhKeys)) {
  fail("zh-CN and en-US resource keys differ");
}
for (const [locale, resources] of [["en-US", enUS], ["zh-CN", zhCN]]) {
  for (const [key, value] of Object.entries(resources)) {
    if (typeof value !== "string" || value.trim() === "") fail(`${locale}:${key} is blank`);
  }
}

const packageJson = readJson(join(appRoot, "package.json"));
const npmProduction = Object.keys(packageJson.dependencies ?? {});
const cargoMetadata = JSON.parse(execFileSync(
  "cargo",
  ["metadata", "--format-version", "1", "--no-deps", "--manifest-path", join(appRoot, "src-tauri/Cargo.toml")],
  { encoding: "utf8" },
));
const rustPackage = cargoMetadata.packages.find((item) => item.name === "ledgerkit-desktop");
if (!rustPackage) fail("ledgerkit-desktop Cargo package is missing");
const rustProduction = rustPackage?.dependencies
  .filter((item) => item.kind === null)
  .map((item) => item.name)
  .sort() ?? [];
const productionDependencies = [...npmProduction, ...rustProduction];
if (productionDependencies.length > 25) {
  fail(`direct production dependency budget exceeded: ${productionDependencies.length}/25`);
}
const forbiddenDependencies = ["tauri-plugin-sql", "chart.js", "recharts", "redux", "zustand", "axios"];
for (const dependency of forbiddenDependencies) {
  if (productionDependencies.includes(dependency)) fail(`forbidden production dependency: ${dependency}`);
}
const pluginCount = productionDependencies.filter((name) => name.startsWith("tauri-plugin-")).length;
if (pluginCount > 8) fail(`Tauri plugin budget exceeded: ${pluginCount}/8`);

const capability = readJson(join(appRoot, "src-tauri/capabilities/main.json"));
const privilegedPermissions = capability.permissions.filter((item) => item.startsWith("allow-"));
if (privilegedPermissions.length > 25) fail(`privileged IPC budget exceeded: ${privilegedPermissions.length}/25`);
const expectedPermissions = [
  "allow-create-ledger",
  "allow-get-activity",
  "allow-get-expense-analysis",
  "allow-get-ledger-status",
  "allow-open-ledger",
  "allow-post-event",
  "allow-preview-event",
  "allow-reverse-event",
  "allow-revise-event",
  "allow-save-cash-account",
  "allow-save-category",
  "allow-save-fx-revision",
  "allow-save-institution",
  "allow-save-instrument",
  "allow-save-portfolio",
  "allow-save-price-revision",
  "allow-update-settings",
];
if (JSON.stringify(privilegedPermissions.sort()) !== JSON.stringify(expectedPermissions)) {
  fail("M2 cash capability set differs from the seventeen reviewed operations");
}

const sourceFiles = walkFiles(join(appRoot, "src")).filter((path) => /\.(ts|tsx)$/.test(path));
const rustFiles = walkFiles(join(appRoot, "src-tauri/src")).filter((path) => path.endsWith(".rs"));
const forbiddenSourcePatterns = [
  [/\bfetch\s*\(/, "fetch"],
  [/\bTcpListener\b/, "TCP listener"],
  [/\bCommand::new\b/, "process launch"],
  [/\btauri_plugin_sql\b/, "general SQL plugin"],
];
for (const path of [...sourceFiles, ...rustFiles]) {
  const text = readFileSync(path, "utf8");
  for (const [pattern, label] of forbiddenSourcePatterns) {
    if (pattern.test(text)) fail(`${relative(repositoryRoot, path)} contains forbidden ${label}`);
  }
  if (/\bunsafe\s*\{/.test(text) && !path.endsWith(join("platform", "webview.rs"))) {
    fail(`${relative(repositoryRoot, path)} contains unsafe outside the reviewed platform adapter`);
  }
}

const commandClientSources = walkFiles(join(appRoot, "src/command-client")).map((path) => readFileSync(path, "utf8")).join("\n");
if (/from\s+["']\.\.\/ui\//.test(commandClientSources)) fail("command client depends on the UI layer");

const webviewSource = readFileSync(join(appRoot, "src-tauri/src/platform/webview.rs"), "utf8");
if (!/\bunsafe\s*\{/.test(webviewSource) || !webviewSource.includes("// SAFETY:")) {
  fail("reviewed WebView2 adapter must retain one documented unsafe boundary");
}
const tauriConfig = readJson(join(appRoot, "src-tauri/tauri.conf.json"));
if (tauriConfig.bundle?.windows?.webviewInstallMode?.type !== "skip") fail("standard package must use system WebView2");
if (!tauriConfig.app?.security?.csp?.includes("connect-src ipc: http://ipc.localhost")) fail("IPC-only connect CSP is missing");
if (!tauriConfig.app?.windows?.[0]?.additionalBrowserArgs?.includes("msOneAuthWAM")) fail("measured WebView2 network control is missing");

const domainSources = walkFiles(join(appRoot, "src-tauri/src/domain")).map((path) => readFileSync(path, "utf8")).join("\n");
if (/\b(?:tauri|serde|infrastructure|application)::/.test(domainSources)) fail("Domain has an outward dependency");
const applicationSources = walkFiles(join(appRoot, "src-tauri/src/application")).map((path) => readFileSync(path, "utf8")).join("\n");
if (/\b(?:tauri|infrastructure)::/.test(applicationSources)) fail("Application depends on UI or Infrastructure");

const acceptedImplementationAdrs = [
  "ADR-0001-tauri-react-rust.md",
  "ADR-0007-rust-xlsx-adapter.md",
  "ADR-0008-live-database-and-portable-backup-encryption.md",
  "ADR-0009-system-webview2-thin-package.md",
  "ADR-0010-p0-manual-update-and-unsigned-beta.md",
  "ADR-0015-rebuildable-expense-daily-projection.md",
];
const adrIndex = readFileSync(join(repositoryRoot, "docs/adr/README.md"), "utf8");
for (const adrName of acceptedImplementationAdrs) {
  const adrPath = join(repositoryRoot, "docs/adr", adrName);
  const adrText = readFileSync(adrPath, "utf8");
  if (!adrText.includes("> 状态：Accepted") || !adrText.includes("> 决策者：项目所有者") || !adrText.includes("> 授权：")) {
    fail(`${adrName} is missing Accepted owner authorization metadata`);
  }
  if (!adrIndex.includes(`](${adrName})`) || !adrIndex.match(new RegExp(`\\(${adrName.replaceAll(".", "\\.")}\\) \\|[^\\n]+\\| Accepted \\|`))) {
    fail(`${adrName} is not linked as Accepted in the ADR index`);
  }
}
const agentContext = readFileSync(join(repositoryRoot, "docs/agent-context.md"), "utf8");
if (!agentContext.includes("> M1 状态：Tauri 2 + React/TypeScript + Rust Core 已选择并建立生产骨架") || !agentContext.includes("> M0 状态：完成")) {
  fail("agent context does not retain the completed M0 and M1 milestones");
}
if (!existsSync(join(repositoryRoot, "docs/benchmarks/m1/selection.md"))) fail("M1 selection report is missing");
if (existsSync(join(repositoryRoot, "spikes/tauri/package.json")) || existsSync(join(repositoryRoot, "spikes/avalonia/global.json"))) {
  fail("disposable spike source remains in the current tree");
}
for (const path of ["tools/check.ps1", "tools/test.ps1", "tools/build.ps1", ".github/workflows/ci.yml", "app/package-lock.json", "app/src-tauri/Cargo.lock"]) {
  if (!existsSync(join(repositoryRoot, path))) fail(`${path} is missing`);
}

const distFiles = walkFiles(join(appRoot, "dist")).filter((path) => /\.(html|css|js)$/.test(path));
const firstLoadGzipBytes = distFiles.reduce((sum, path) => sum + gzipSync(readFileSync(path)).length, 0);
if (firstLoadGzipBytes > 1.2 * 1024 * 1024) fail(`first-load gzip budget exceeded: ${firstLoadGzipBytes} bytes`);

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exitCode = 1;
} else {
  console.log(`M1_SCAFFOLD_CHECK=PASS locales=2 ipc=${privilegedPermissions.length} plugins=${pluginCount} direct_production_dependencies=${productionDependencies.length} first_load_gzip_bytes=${firstLoadGzipBytes}`);
}
