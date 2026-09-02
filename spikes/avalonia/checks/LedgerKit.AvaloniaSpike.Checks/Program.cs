using System.Reflection;
using System.Text;
using System.Text.Json.Nodes;
using LedgerKit.AvaloniaSpike.Core;
using Microsoft.Data.Sqlite;

namespace LedgerKit.AvaloniaSpike.Checks;

internal static class Program
{
    private static readonly List<string> Passed = [];

    public static int Main()
    {
        var repositoryRoot = FindRepositoryRoot();
        try
        {
            Run("decimal and canonical contract", TestDecimalAndCanonical);
            Run("M0 fixture 01 normal/boundary/failure", () => TestM0Fixture01(repositoryRoot));
            Run("transaction rollback", TestTransactionRollback);
            Run("expense query hash and rebuild", TestExpenseQuery);
            Run("shared 10k XLSX contract", () => TestSharedWorkbook(repositoryRoot));
            Run("standardized XLSX export", TestExport);
            Run("one-use file authorization", TestFileAuthorization);
            Run("encrypted backup and safe restore", TestBackupRestore);
            Run("application facade privilege boundary", TestFacadeBoundary);
            Run("source boundary and UI worker", () => TestSourceBoundary(repositoryRoot));
            Console.WriteLine($"CHECKS_PASS={Passed.Count}");
            return 0;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine($"CHECKS_FAIL={Passed.Count + 1}");
            Console.Error.WriteLine(exception);
            return 1;
        }
    }

    private static void TestDecimalAndCanonical()
    {
        Equal("DECIMAL_INVALID", CaptureCode(() => DecimalContract.ValidatePositiveAmount("1e2", false)));
        Equal(
            "DECIMAL_SCALE_EXCEEDED",
            CaptureCode(() => DecimalContract.ValidatePositiveAmount("1.000000000", true)));
        Equal(
            "CURRENCY_PRECISION_CONFIRMATION_REQUIRED",
            CaptureCode(() => DecimalContract.ValidatePositiveAmount("0.00000001", false)));
        Equal("0.00000001", DecimalContract.ValidatePositiveAmount("0.00000001", true).Text);
        var value = new JsonObject { ["b"] = 2, ["a"] = 1 };
        Equal("{\"a\":1,\"b\":2}", Encoding.UTF8.GetString(CanonicalJson.Bytes(value)));
    }

    private static void TestM0Fixture01(string repositoryRoot)
    {
        var fixtureRoot = Path.Combine(repositoryRoot, "fixtures", "sanitized", "01-cny-income-expense");
        var expectedEvents = JsonNode.Parse(File.ReadAllBytes(Path.Combine(fixtureRoot, "normalized-events.json")))!;
        var expectedPostings = JsonNode.Parse(File.ReadAllBytes(Path.Combine(fixtureRoot, "expected-postings.json")))!;
        var expectedProjections = JsonNode.Parse(File.ReadAllBytes(Path.Combine(fixtureRoot, "expected-projection.json")))!;

        using (var scope = TempScope.Create())
        using (var store = LedgerStore.Open(Path.Combine(scope.Path, "normal.sqlite")))
        {
            var income = store.PostEventForCheck(
                new PostEventRequest(
                    "Income",
                    "2026-01-05",
                    "cash-cny-1",
                    "100.00",
                    "CNY",
                    "cat-salary"),
                "evt-01-normal-01");
            var expense = store.PostEventForCheck(
                new PostEventRequest(
                    "Expense",
                    "2026-01-06",
                    "cash-cny-1",
                    "25.50",
                    "CNY",
                    "cat-food"),
                "evt-01-normal-02");
            var postings = new JsonArray(income.Posting.ToJson(), expense.Posting.ToJson());
            var expectedScenario = FindScenario(expectedPostings, "normal");
            DeepEqual(expectedScenario["postings"]!, postings);
            Equal(expectedScenario["sequence_hash"]!.GetValue<string>(), CanonicalJson.Hash(postings));

            var events = new JsonArray(ToNormalizedEvent(income.Event), ToNormalizedEvent(expense.Event));
            DeepEqual(FindScenario(expectedEvents, "normal")["events"]!, events);
            var actualProjection = store.GetFixtureProjection("100.00", "25.50");
            var expectedProjection = FindScenario(expectedProjections, "normal").DeepClone().AsObject();
            expectedProjection.Remove("scenario_id");
            DeepEqual(expectedProjection, actualProjection);
        }

        using (var scope = TempScope.Create())
        using (var store = LedgerStore.Open(Path.Combine(scope.Path, "boundary.sqlite")))
        {
            var response = store.PostEventForCheck(
                new PostEventRequest(
                    "Expense",
                    "2026-01-31",
                    "cash-cny-1",
                    "0.00000001",
                    "CNY",
                    "cat-rounding",
                    true),
                "evt-01-boundary-01");
            var postings = new JsonArray(response.Posting.ToJson());
            var expectedPosting = FindScenario(expectedPostings, "boundary");
            DeepEqual(expectedPosting["postings"]!, postings);
            Equal(expectedPosting["sequence_hash"]!.GetValue<string>(), CanonicalJson.Hash(postings));
            Equal("-0.00000001", store.GetOverview().CashValue);
            Equal("0.00", DecimalContract.RoundHalfUp(
                DecimalContract.ParseStored(store.GetOverview().CashValue),
                2).ToString("0.00", System.Globalization.CultureInfo.InvariantCulture));
        }

        using (var scope = TempScope.Create())
        using (var store = LedgerStore.Open(Path.Combine(scope.Path, "failure.sqlite")))
        {
            Equal(
                "DECIMAL_SCALE_EXCEEDED",
                CaptureCode(() => store.PostEvent(new PostEventRequest(
                    "Income",
                    "2026-01-05",
                    "cash-cny-1",
                    "1.000000000",
                    "CNY",
                    null))));
            Equal(0UL, store.GetStatus().EventWatermark);
        }
    }

    private static void TestTransactionRollback()
    {
        using var scope = TempScope.Create();
        using var store = LedgerStore.Open(Path.Combine(scope.Path, "ledger.sqlite"));
        Equal(
            "SYNTHETIC_FAILPOINT",
            CaptureCode(() => store.PostEventForCheck(
                new PostEventRequest(
                    "Expense",
                    "2026-01-06",
                    "cash-cny-1",
                    "25.50",
                    "CNY",
                    "cat-food"),
                "evt-fail",
                true)));
        Equal(0UL, store.GetStatus().EventWatermark);
        Equal(0UL, store.GetStatus().ProjectionWatermark);
    }

    private static void TestExpenseQuery()
    {
        using var scope = TempScope.Create();
        using var store = LedgerStore.Open(Path.Combine(scope.Path, "ledger.sqlite"));
        store.SeedExpenseFixtureForCheck();
        var first = store.GetExpenseAnalysis("2026-02-01", "2026-02-28").QueryResult;
        Equal(12, first["buckets"]!.AsArray().Count);
        Equal(10, first["top10"]!["items"]!.AsArray().Count);
        Equal("30", first["top10"]!["other"]!["amount"]!.GetValue<string>());
        Equal(
            "sha256:7cd365ef12db020eb178975704fd2388cad37b5a4f378c6debf5e3aef27a8beb",
            first["canonical_hash"]!.GetValue<string>());
        store.RebuildExpenseProjection();
        DeepEqual(first, store.GetExpenseAnalysis("2026-02-01", "2026-02-28").QueryResult);
    }

    private static void TestSharedWorkbook(string repositoryRoot)
    {
        var fixture = Path.Combine(
            repositoryRoot,
            "fixtures",
            "sanitized",
            "m1",
            "ledgerkit-known-template-10000.xlsx");
        var summary = ExcelAdapter.AnalyzeKnownTemplate(fixture);
        Equal(10_000, summary.RowCount);
        Equal(
            "sha256:d7bbf52a86d2655ec09fe82fa42690f1c9e7aad6d323c6f167a86c797c024bd5",
            summary.FileSha256);
        True(summary.FinancialValuesRemainedStrings, "Financial values must remain strings.");
    }

    private static void TestExport()
    {
        using var scope = TempScope.Create();
        using var store = LedgerStore.Open(Path.Combine(scope.Path, "ledger.sqlite"));
        store.PostEventForCheck(
            new PostEventRequest("Income", "2026-01-05", "cash-cny-1", "100.00", "CNY", "cat-salary"),
            "evt-01-normal-01");
        store.PostEventForCheck(
            new PostEventRequest("Expense", "2026-01-06", "cash-cny-1", "25.50", "CNY", "cat-food"),
            "evt-01-normal-02");
        var exportPath = Path.Combine(scope.Path, "standardized.xlsx");
        var summary = ExcelAdapter.ExportStandardized(store.GetAllActivity(), exportPath);
        Equal(2, summary.RowCount);
        var inspected = ExcelAdapter.InspectExport(exportPath);
        True(ExcelAdapter.KnownHeaders.SequenceEqual(inspected.Headers, StringComparer.Ordinal), "Headers differ.");
        Equal(2, inspected.RowCount);
        Equal("InlineString", inspected.FirstAmountType);
    }

    private static void TestFileAuthorization()
    {
        using var scope = TempScope.Create();
        var service = new FileAuthorizationService(Path.Combine(scope.Path, "managed"));
        var source = Path.Combine(scope.Path, "synthetic.RECEIPT.txt");
        File.WriteAllText(source, "synthetic receipt", Encoding.UTF8);
        Equal(
            "FILE_AUTHORIZATION_REJECTED",
            CaptureCode(() => service.CopyAuthorizedAttachment("../../forged")));
        var authorization = service.AuthorizeSelectedFile(source, "attachment");
        var copied = service.CopyAuthorizedAttachment(authorization.AuthorizationToken);
        True(copied.RelativeLocation.StartsWith("attachments/", StringComparison.Ordinal), "Destination escaped.");
        True(!copied.RelativeLocation.Contains("..", StringComparison.Ordinal), "Destination contains traversal.");
        Equal(
            "FILE_AUTHORIZATION_REJECTED",
            CaptureCode(() => service.CopyAuthorizedAttachment(authorization.AuthorizationToken)));
    }

    private static void TestBackupRestore()
    {
        using var scope = TempScope.Create();
        using var facade = new ApplicationFacade(Path.Combine(scope.Path, "app"));
        var originalWatermark = facade.GetLedgerStatus().EventWatermark;
        var backup = facade.CreateBackup("synthetic-password");
        var packagePath = Path.Combine(
            scope.Path,
            "app",
            "backups",
            $"{backup.BackupId}.ledgerkit-backup");
        var packageBytes = File.ReadAllBytes(packagePath);
        True(!Contains(packageBytes, "SQLite format 3\0"), "Backup contains SQLite plaintext header.");
        True(!Contains(packageBytes, "synthetic-password"), "Backup contains password plaintext.");
        facade.PostEvent(new PostEventRequest(
            "Income",
            "2026-02-20",
            "cash-cny-1",
            "10",
            "CNY",
            "cat-income"));
        var changedWatermark = facade.GetLedgerStatus().EventWatermark;
        Equal(
            "BACKUP_AUTHENTICATION_FAILED",
            CaptureCode(() => facade.RestoreBackup(backup.BackupId, "wrong-password")));
        Equal(changedWatermark, facade.GetLedgerStatus().EventWatermark);
        facade.RestoreBackup(backup.BackupId, "synthetic-password");
        Equal(originalWatermark, facade.GetLedgerStatus().EventWatermark);
    }

    private static void TestFacadeBoundary()
    {
        var methods = typeof(ApplicationFacade).GetMethods(BindingFlags.Public | BindingFlags.Instance)
            .Where(method => method.DeclaringType == typeof(ApplicationFacade))
            .ToArray();
        True(methods.Length <= 25, "Application facade exceeds the named operation budget.");
        True(methods.All(method => !method.Name.Contains("Sql", StringComparison.OrdinalIgnoreCase)), "SQL method exposed.");
        True(methods.All(method => !method.Name.Contains("Shell", StringComparison.OrdinalIgnoreCase)), "Shell method exposed.");
        True(methods.SelectMany(method => method.GetParameters()).All(parameter =>
                parameter.ParameterType != typeof(Posting) &&
                parameter.ParameterType != typeof(SqliteConnection) &&
                parameter.ParameterType != typeof(SqliteCommand)),
            "Facade accepts a privileged posting or SQLite object.");
    }

    private static void TestSourceBoundary(string repositoryRoot)
    {
        var spikeRoot = Path.Combine(repositoryRoot, "spikes", "avalonia");
        var sourceFiles = Directory.GetFiles(Path.Combine(spikeRoot, "src"), "*.cs", SearchOption.AllDirectories)
            .Where(path => !path.Contains($"{Path.DirectorySeparatorChar}obj{Path.DirectorySeparatorChar}", StringComparison.Ordinal))
            .ToArray();
        var appSource = File.ReadAllText(Path.Combine(
            spikeRoot,
            "src",
            "LedgerKit.AvaloniaSpike",
            "MainWindow.cs"));
        True(appSource.Contains("Task.Run", StringComparison.Ordinal), "Workbook parsing is not moved off the UI thread.");
        foreach (var sourceFile in sourceFiles)
        {
            var source = File.ReadAllText(sourceFile);
            True(!source.Contains("HttpClient", StringComparison.Ordinal), $"Network API found in {sourceFile}.");
            True(!source.Contains("WebSocket", StringComparison.Ordinal), $"Network API found in {sourceFile}.");
            True(!source.Contains("Process.Start", StringComparison.Ordinal), $"Shell/process API found in {sourceFile}.");
        }
    }

    private static JsonObject ToNormalizedEvent(EventRecord ledgerEvent) => new()
    {
        ["event_id"] = ledgerEvent.EventId,
        ["event_type"] = ledgerEvent.EventType,
        ["effective_date"] = ledgerEvent.EffectiveDate,
        ["sequence"] = ledgerEvent.Sequence,
        ["status"] = "posted",
        ["detail"] = new JsonObject
        {
            ["account_id"] = ledgerEvent.AccountId,
            ["amount"] = ledgerEvent.Amount,
            ["currency"] = ledgerEvent.Currency,
            ["category_id"] = ledgerEvent.CategoryId,
        },
        ["calculation_version"] = LedgerStore.CalculationVersion,
    };

    private static JsonObject FindScenario(JsonNode document, string scenarioId) =>
        document["scenarios"]!.AsArray()
            .Select(node => node!.AsObject())
            .Single(node => node["scenario_id"]!.GetValue<string>() == scenarioId);

    private static string CaptureCode(Action action)
    {
        try
        {
            action();
        }
        catch (SpikeException exception)
        {
            return exception.Code;
        }

        throw new InvalidOperationException("Expected a SpikeException.");
    }

    private static bool Contains(byte[] bytes, string value) =>
        Encoding.UTF8.GetString(bytes).Contains(value, StringComparison.Ordinal);

    private static void Run(string name, Action action)
    {
        action();
        Passed.Add(name);
        Console.WriteLine($"PASS {Passed.Count:00}: {name}");
    }

    private static void Equal<T>(T expected, T actual)
        where T : notnull
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
        {
            throw new InvalidOperationException($"Expected '{expected}', received '{actual}'.");
        }
    }

    private static void True(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private static void DeepEqual(JsonNode expected, JsonNode actual)
    {
        if (!JsonNode.DeepEquals(expected, actual))
        {
            throw new InvalidOperationException(
                $"JSON mismatch.\nExpected: {expected}\nActual: {actual}");
        }
    }

    private static string FindRepositoryRoot()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null)
        {
            if (File.Exists(Path.Combine(directory.FullName, "AGENTS.md")) &&
                Directory.Exists(Path.Combine(directory.FullName, "fixtures", "sanitized")))
            {
                return directory.FullName;
            }

            directory = directory.Parent;
        }

        throw new DirectoryNotFoundException("Repository root was not found.");
    }

    private sealed class TempScope : IDisposable
    {
        private TempScope(string path)
        {
            Path = path;
        }

        public string Path { get; }

        public static TempScope Create()
        {
            var path = System.IO.Path.Combine(
                System.IO.Path.GetTempPath(),
                $"ledgerkit-avalonia-check-{Environment.ProcessId}-{Guid.NewGuid():N}");
            Directory.CreateDirectory(path);
            return new TempScope(path);
        }

        public void Dispose()
        {
            var resolved = System.IO.Path.GetFullPath(Path);
            var temporaryRoot = System.IO.Path.GetFullPath(System.IO.Path.GetTempPath());
            if (resolved.StartsWith(temporaryRoot, StringComparison.OrdinalIgnoreCase) && Directory.Exists(resolved))
            {
                Directory.Delete(resolved, true);
            }
        }
    }
}
