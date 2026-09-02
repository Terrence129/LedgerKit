using System.Diagnostics;
using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Spreadsheet;

namespace LedgerKit.AvaloniaSpike.Core;

public static class ExcelAdapter
{
    public static readonly string[] KnownHeaders =
    [
        "event_id",
        "effective_date",
        "event_type",
        "account_id",
        "category_id",
        "amount",
        "currency",
        "note",
    ];

    public static ImportSummary AnalyzeKnownTemplate(string path)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        var fullPath = Path.GetFullPath(path);
        var started = Stopwatch.StartNew();
        var bytes = File.ReadAllBytes(fullPath);
        try
        {
            using var document = SpreadsheetDocument.Open(fullPath, false);
            var workbookPart = document.WorkbookPart ??
                               throw ContractMismatch("Workbook part is absent.");
            var workbook = workbookPart.Workbook ?? throw ContractMismatch("Workbook is absent.");
            var sheet = workbook.Sheets?.Elements<Sheet>()
                .SingleOrDefault(candidate => candidate.Name?.Value == "Transactions") ??
                        throw ContractMismatch("Transactions worksheet is absent.");
            var relationshipId = sheet.Id?.Value ?? throw ContractMismatch("Worksheet relation is absent.");
            var worksheetPart = (WorksheetPart)workbookPart.GetPartById(relationshipId);
            var sharedStrings = workbookPart.SharedStringTablePart?.SharedStringTable?
                .Elements<SharedStringItem>()
                .Select(static item => item.InnerText)
                .ToArray();
            var rowCount = 0;
            var sawHeader = false;
            using var reader = OpenXmlReader.Create(worksheetPart);
            while (reader.Read())
            {
                if (reader.ElementType != typeof(Row) || !reader.IsStartElement)
                {
                    continue;
                }

                var row = reader.LoadCurrentElement() as Row ??
                          throw ContractMismatch("Workbook row could not be loaded.");
                var cells = ReadCells(row, sharedStrings);
                if (!sawHeader)
                {
                    if (!KnownHeaders.SequenceEqual(cells, StringComparer.Ordinal))
                    {
                        throw ContractMismatch("Header contract does not match.");
                    }

                    sawHeader = true;
                    continue;
                }

                if (cells.Length != KnownHeaders.Length)
                {
                    throw ContractMismatch("Transaction row width does not match.");
                }

                var amountCell = CellAt(row, 5);
                var amountType = amountCell?.DataType?.Value;
                if (amountCell is null ||
                    !(amountType == CellValues.SharedString ||
                      amountType == CellValues.InlineString ||
                      amountType == CellValues.String))
                {
                    throw ContractMismatch("Amount cell is not stored as text.");
                }

                if (!cells[0].StartsWith("syn-event-", StringComparison.Ordinal) ||
                    !DateOnly.TryParseExact(
                        cells[1],
                        "yyyy-MM-dd",
                        System.Globalization.CultureInfo.InvariantCulture,
                        System.Globalization.DateTimeStyles.None,
                        out _) ||
                    cells[2] is not ("Income" or "Expense") ||
                    cells[6] != "CNY")
                {
                    throw ContractMismatch("Synthetic transaction contract does not match.");
                }

                rowCount++;
            }

            if (!sawHeader || rowCount != 10_000)
            {
                throw ContractMismatch("Workbook must contain exactly 10,000 transaction rows.");
            }

            return new ImportSummary(
                "Transactions",
                rowCount,
                CanonicalJson.Sha256Prefixed(bytes),
                started.Elapsed.TotalMilliseconds,
                true);
        }
        catch (SpikeException)
        {
            throw;
        }
        catch (Exception exception) when (exception is IOException or OpenXmlPackageException)
        {
            throw SpikeException.Wrap("WORKBOOK_OPERATION_FAILED", "Workbook analysis failed.", exception);
        }
    }

    public static ExportSummary ExportStandardized(IReadOnlyList<EventRecord> events, string path)
    {
        ArgumentNullException.ThrowIfNull(events);
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        var fullPath = Path.GetFullPath(path);
        Directory.CreateDirectory(Path.GetDirectoryName(fullPath)!);
        try
        {
            using (var document = SpreadsheetDocument.Create(fullPath, SpreadsheetDocumentType.Workbook))
            {
                var workbookPart = document.AddWorkbookPart();
                workbookPart.Workbook = new Workbook();
                var worksheetPart = workbookPart.AddNewPart<WorksheetPart>();
                var sheetData = new SheetData();
                var sheetViews = new SheetViews(
                    new SheetView(
                        new Pane
                        {
                            VerticalSplit = 1D,
                            TopLeftCell = "A2",
                            ActivePane = PaneValues.BottomLeft,
                            State = PaneStateValues.Frozen,
                        })
                    {
                        WorkbookViewId = 0U,
                    });
                var columns = new Columns(
                    Column(1, 24),
                    Column(2, 14),
                    Column(3, 12),
                    Column(4, 16),
                    Column(5, 16),
                    Column(6, 14),
                    Column(7, 10),
                    Column(8, 42));
                worksheetPart.Worksheet = new Worksheet(sheetViews, columns, sheetData);

                var header = new Row { RowIndex = 1U };
                for (var index = 0; index < KnownHeaders.Length; index++)
                {
                    header.Append(InlineTextCell(KnownHeaders[index], $"{(char)('A' + index)}1"));
                }

                sheetData.Append(header);
                uint rowIndex = 2;
                foreach (var ledgerEvent in events)
                {
                    var row = new Row { RowIndex = rowIndex++ };
                    var values = new[]
                    {
                        ledgerEvent.EventId,
                        ledgerEvent.EffectiveDate,
                        ledgerEvent.EventType,
                        ledgerEvent.AccountId,
                        ledgerEvent.CategoryId ?? string.Empty,
                        ledgerEvent.Amount,
                        ledgerEvent.Currency,
                        ledgerEvent.Note ?? string.Empty,
                    };
                    for (var index = 0; index < values.Length; index++)
                    {
                        row.Append(InlineTextCell(values[index], $"{(char)('A' + index)}{row.RowIndex!.Value}"));
                    }
                    sheetData.Append(row);
                }

                var sheets = workbookPart.Workbook.AppendChild(new Sheets());
                sheets.Append(new Sheet
                {
                    Id = workbookPart.GetIdOfPart(worksheetPart),
                    SheetId = 1U,
                    Name = "Transactions",
                });
                worksheetPart.Worksheet.Save();
                workbookPart.Workbook.Save();
            }

            return new ExportSummary(
                Path.GetFileName(fullPath),
                events.Count,
                CanonicalJson.Sha256Prefixed(File.ReadAllBytes(fullPath)));
        }
        catch (Exception exception) when (exception is IOException or OpenXmlPackageException)
        {
            throw SpikeException.Wrap("WORKBOOK_OPERATION_FAILED", "Workbook export failed.", exception);
        }
    }

    internal static (string[] Headers, int RowCount, string FirstAmountType) InspectExport(string path)
    {
        using var document = SpreadsheetDocument.Open(path, false);
        var workbookPart = document.WorkbookPart ?? throw ContractMismatch("Workbook part is absent.");
        var workbook = workbookPart.Workbook ?? throw ContractMismatch("Workbook is absent.");
        var sheet = workbook.Sheets?.Elements<Sheet>().Single() ??
                    throw ContractMismatch("Worksheet is absent.");
        var worksheetPart = (WorksheetPart)workbookPart.GetPartById(sheet.Id!.Value!);
        var worksheet = worksheetPart.Worksheet ?? throw ContractMismatch("Worksheet data is absent.");
        var rows = worksheet.GetFirstChild<SheetData>()?.Elements<Row>().ToArray() ??
                   throw ContractMismatch("Worksheet rows are absent.");
        var sharedStrings = workbookPart.SharedStringTablePart?.SharedStringTable?
            .Elements<SharedStringItem>()
            .Select(static item => item.InnerText)
            .ToArray();
        var headers = ReadCells(rows[0], sharedStrings);
        var amount = CellAt(rows[1], 5)!;
        var amountType = amount.DataType?.Value;
        var amountTypeName = amountType == CellValues.InlineString
            ? "InlineString"
            : amountType == CellValues.SharedString
                ? "SharedString"
                : amountType == CellValues.String
                    ? "String"
                    : "Number";
        return (headers, rows.Length - 1, amountTypeName);
    }

    private static string[] ReadCells(Row row, IReadOnlyList<string>? sharedStrings)
    {
        var values = new string[KnownHeaders.Length];
        foreach (var cell in row.Elements<Cell>())
        {
            var index = ColumnIndex(cell.CellReference?.Value);
            if (index >= 0 && index < values.Length)
            {
                values[index] = ReadCellText(cell, sharedStrings);
            }
        }

        return values;
    }

    private static Cell? CellAt(Row row, int expectedIndex) => row.Elements<Cell>()
        .FirstOrDefault(cell => ColumnIndex(cell.CellReference?.Value) == expectedIndex);

    private static string ReadCellText(Cell cell, IReadOnlyList<string>? sharedStrings)
    {
        var value = cell.CellValue?.Text ?? string.Empty;
        var dataType = cell.DataType?.Value;
        if (dataType == CellValues.SharedString && sharedStrings is not null)
        {
            return sharedStrings[int.Parse(value, System.Globalization.CultureInfo.InvariantCulture)];
        }

        return dataType == CellValues.InlineString
            ? cell.InlineString?.InnerText ?? string.Empty
            : value;
    }

    private static int ColumnIndex(string? reference)
    {
        if (string.IsNullOrEmpty(reference))
        {
            return -1;
        }

        var index = 0;
        foreach (var character in reference)
        {
            if (!char.IsAsciiLetterUpper(character))
            {
                break;
            }

            index = (index * 26) + (character - 'A' + 1);
        }

        return index - 1;
    }

    private static Cell InlineTextCell(string value, string reference) => new()
    {
        CellReference = reference,
        DataType = CellValues.InlineString,
        InlineString = new InlineString(new Text(value) { Space = SpaceProcessingModeValues.Preserve }),
    };

    private static Column Column(uint index, double width) => new()
    {
        Min = index,
        Max = index,
        Width = width,
        CustomWidth = true,
    };

    private static SpikeException ContractMismatch(string message) =>
        new("WORKBOOK_CONTRACT_MISMATCH", message);
}
