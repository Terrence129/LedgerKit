import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ExcelJS from "exceljs";

const here = path.dirname(fileURLToPath(import.meta.url));
const fixture = path.resolve(here, "../../../fixtures/sanitized/m1/ledgerkit-known-template-10000.xlsx");
if (global.gc) global.gc();
const rssBefore = process.memoryUsage().rss;
const started = performance.now();
const workbook = new ExcelJS.Workbook();
await workbook.xlsx.load(fs.readFileSync(fixture));
const sheet = workbook.getWorksheet("Transactions");
const elapsedMs = performance.now() - started;
if (!sheet || sheet.rowCount !== 10_001 || sheet.getCell("F2").type !== ExcelJS.ValueType.String) {
  throw new Error("TypeScript adapter did not preserve the known template contract");
}
console.log(JSON.stringify({
  adapter: "exceljs",
  rows: sheet.rowCount - 1,
  elapsed_ms: elapsedMs,
  rss_before_bytes: rssBefore,
  rss_after_bytes: process.memoryUsage().rss,
  amount_cells_are_strings: true,
}, null, 2));
