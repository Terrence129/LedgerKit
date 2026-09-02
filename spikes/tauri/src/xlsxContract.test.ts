import fs from "node:fs";
import path from "node:path";
import ExcelJS from "exceljs";
import { describe, expect, test } from "vitest";

describe("TypeScript XLSX candidate", () => {
  test("reads the shared 10k known template without coercing amount strings", async () => {
    const fixture = path.resolve(
      __dirname,
      "../../../fixtures/sanitized/m1/ledgerkit-known-template-10000.xlsx",
    );
    const workbook = new ExcelJS.Workbook();
    await workbook.xlsx.load(fs.readFileSync(fixture));
    const sheet = workbook.getWorksheet("Transactions");
    expect(sheet).toBeDefined();
    expect(sheet?.rowCount).toBe(10_001);
    expect(sheet?.getCell("F2").type).toBe(ExcelJS.ValueType.String);
    expect(sheet?.getCell("F2").text).toBe("2.01");
    expect(sheet?.getCell("A10001").text).toBe("syn-event-10000");
  });
});
