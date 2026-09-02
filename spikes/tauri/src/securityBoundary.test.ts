import fs from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const root = path.resolve(__dirname, "..");

describe("Tauri privilege boundary", () => {
  test("allows only the twelve named application commands", () => {
    const capability = JSON.parse(
      fs.readFileSync(path.join(root, "src-tauri/capabilities/default.json"), "utf8"),
    ) as { permissions: string[] };
    const applicationPermissions = capability.permissions.filter((permission) =>
      permission.startsWith("allow-"),
    );
    expect(applicationPermissions).toEqual([
      "allow-get-ledger-status",
      "allow-post-event",
      "allow-get-activity",
      "allow-get-overview",
      "allow-get-expense-analysis",
      "allow-analyze-import",
      "allow-export-data",
      "allow-authorize-attachment",
      "allow-copy-attachment",
      "allow-create-backup",
      "allow-restore-backup",
      "allow-mark-frontend-ready",
    ]);
    expect(applicationPermissions).not.toContain("allow-execute-sql");
    expect(applicationPermissions).not.toContain("allow-shell");
  });

  test("has no remote window URL, SQL/shell plugin, or frontend network API", () => {
    const config = JSON.parse(
      fs.readFileSync(path.join(root, "src-tauri/tauri.conf.json"), "utf8"),
    ) as {
      app: {
        windows: Array<{ url?: string; additionalBrowserArgs?: string }>;
        security: { csp: string };
      };
    };
    const cargo = fs.readFileSync(path.join(root, "src-tauri/Cargo.toml"), "utf8");
    const frontend = fs.readFileSync(path.join(root, "src/App.tsx"), "utf8");
    expect(config.app.windows.every((window) => window.url === undefined)).toBe(true);
    expect(config.app.security.csp).not.toMatch(/https?:\/\/(?!ipc\.localhost)/);
    expect(config.app.windows[0]?.additionalBrowserArgs).toContain("--disable-background-networking");
    expect(config.app.windows[0]?.additionalBrowserArgs).toContain("msOneAuthWAM");
    expect(cargo).not.toMatch(/tauri-plugin-(sql|shell|fs)/);
    expect(frontend).not.toMatch(/\b(fetch|XMLHttpRequest|WebSocket)\b/);
    expect(frontend).not.toMatch(/https?:\/\//);
  });

  test("keeps native import selection and parsing off the UI thread", () => {
    const backend = fs.readFileSync(
      path.join(root, "src-tauri/src/application.rs"),
      "utf8",
    );
    expect(backend).toMatch(/pub async fn analyze_import\(\)/);
    expect(backend).toContain("rfd::AsyncFileDialog");
    expect(backend).toContain("tauri::async_runtime::spawn_blocking");
  });
});
