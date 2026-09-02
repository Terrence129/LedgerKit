import crypto from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ExcelJS from "exceljs";
import JSZip from "jszip";

const fixedDate = new Date("2026-01-01T00:00:00.000Z");
const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../../..");
const fixtureRoot = path.join(repoRoot, "fixtures", "sanitized", "m1");
const outputPath = path.join(fixtureRoot, "ledgerkit-known-template-10000.xlsx");
const manifestPath = path.join(fixtureRoot, "manifest.json");
const checkOnly = process.argv.includes("--check");

async function buildWorkbook() {
  const workbook = new ExcelJS.Workbook();
  workbook.creator = "LedgerKit synthetic fixture generator";
  workbook.lastModifiedBy = "LedgerKit synthetic fixture generator";
  workbook.created = fixedDate;
  workbook.modified = fixedDate;
  workbook.calcProperties.fullCalcOnLoad = false;
  const sheet = workbook.addWorksheet("Transactions", {
    properties: { defaultRowHeight: 15 },
    views: [{ state: "frozen", ySplit: 1 }],
  });
  sheet.columns = [
    { header: "event_id", key: "event_id", width: 24 },
    { header: "effective_date", key: "effective_date", width: 14 },
    { header: "event_type", key: "event_type", width: 12 },
    { header: "account_id", key: "account_id", width: 16 },
    { header: "category_id", key: "category_id", width: 16 },
    { header: "amount", key: "amount", width: 14 },
    { header: "currency", key: "currency", width: 10 },
    { header: "note", key: "note", width: 36 },
  ];
  sheet.getRow(1).font = { bold: true };
  for (let index = 1; index <= 10_000; index += 1) {
    const day = String(((index - 1) % 28) + 1).padStart(2, "0");
    const category = String(((index - 1) % 12) + 1).padStart(2, "0");
    const eventType = index % 10 === 0 ? "Income" : "Expense";
    const amount = `${(index % 997) + 1}.${String(index % 100).padStart(2, "0")}`;
    sheet.addRow({
      event_id: `syn-event-${String(index).padStart(5, "0")}`,
      effective_date: `2026-02-${day}`,
      event_type: eventType,
      account_id: "cash-cny-1",
      category_id: eventType === "Expense" ? `cat-${category}` : "cat-income",
      amount,
      currency: "CNY",
      note: `Synthetic row ${String(index).padStart(5, "0")}`,
    });
  }
  const initial = Buffer.from(await workbook.xlsx.writeBuffer());
  const archive = await JSZip.loadAsync(initial);
  for (const entry of Object.values(archive.files)) entry.date = fixedDate;
  return archive.generateAsync({
    type: "nodebuffer",
    compression: "DEFLATE",
    compressionOptions: { level: 9 },
    platform: "DOS",
  });
}

const bytes = await buildWorkbook();
const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");
const manifest = `${JSON.stringify({
  contract: "ledgerkit-known-template-xlsx/v1",
  file: path.basename(outputPath),
  rows: 10_000,
  worksheet: "Transactions",
  sha256: `sha256:${sha256}`,
  generated_at: "2026-01-01T00:00:00Z",
  contains_only_synthetic_data: true,
}, null, 2)}\n`;

if (checkOnly) {
  const [existing, existingManifest] = await Promise.all([
    fs.readFile(outputPath),
    fs.readFile(manifestPath, "utf8"),
  ]);
  if (!existing.equals(bytes) || existingManifest !== manifest) {
    throw new Error("10k XLSX fixture is not reproducible; run npm run generate:fixture");
  }
  console.log(`fixture reproducible: sha256:${sha256}`);
} else {
  await fs.mkdir(fixtureRoot, { recursive: true });
  await Promise.all([fs.writeFile(outputPath, bytes), fs.writeFile(manifestPath, manifest)]);
  console.log(`wrote ${outputPath} (${bytes.length} bytes, sha256:${sha256})`);
}
