using System.Diagnostics;
using System.Text.Json;
using LedgerKit.AvaloniaSpike.Core;

namespace LedgerKit.AvaloniaSpike.Benchmarks;

internal static class Program
{
    private static readonly JsonSerializerOptions IndentedJson = new() { WriteIndented = true };

    public static int Main()
    {
        var repositoryRoot = FindRepositoryRoot();
        var benchmarkRoot = Path.Combine(
            Path.GetTempPath(),
            $"ledgerkit-m1-avalonia-bench-{Environment.ProcessId}-{Guid.NewGuid():N}");
        Directory.CreateDirectory(benchmarkRoot);
        try
        {
            Run(repositoryRoot, benchmarkRoot);
            return 0;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(exception);
            return 1;
        }
        finally
        {
            var resolved = Path.GetFullPath(benchmarkRoot);
            if (resolved.StartsWith(Path.GetFullPath(Path.GetTempPath()), StringComparison.OrdinalIgnoreCase) &&
                Directory.Exists(resolved))
            {
                Directory.Delete(resolved, true);
            }
        }
    }

    private static void Run(string repositoryRoot, string benchmarkRoot)
    {
        // Separate one-time CLR/JIT initialization from the schema-open measurement.
        // Cold application startup is measured independently by tools/measure-runtime.ps1.
        var runtimeWarmupStarted = Stopwatch.StartNew();
        using (LedgerStore.Open(Path.Combine(benchmarkRoot, "runtime-warmup.sqlite")))
        {
        }

        var runtimeWarmupMs = runtimeWarmupStarted.Elapsed.TotalMilliseconds;
        var databasePath = Path.Combine(benchmarkRoot, "benchmark.sqlite");
        var openStarted = Stopwatch.StartNew();
        using var store = LedgerStore.Open(databasePath);
        var migrationOpenMs = openStarted.Elapsed.TotalMilliseconds;
        store.InitializeDemo();

        var fixture = Path.Combine(
            repositoryRoot,
            "fixtures",
            "sanitized",
            "m1",
            "ledgerkit-known-template-10000.xlsx");
        var import = ExcelAdapter.AnalyzeKnownTemplate(fixture);
        var writeSamples = Measure(30, index => store.PostEvent(new PostEventRequest(
            "Expense",
            "2026-02-20",
            "cash-cny-1",
            "1.00",
            "CNY",
            "cat-01",
            Note: $"Synthetic benchmark write {index:00}")));
        var pageSamples = Measure(30, _ => store.GetActivity(1, 20));
        var exportPath = Path.Combine(benchmarkRoot, "standardized.xlsx");
        var exportStarted = Stopwatch.StartNew();
        var export = ExcelAdapter.ExportStandardized(store.GetAllActivity(), exportPath);
        var exportMs = exportStarted.Elapsed.TotalMilliseconds;
        store.CheckpointWal();
        var currentDatabaseBytes = new FileInfo(databasePath).Length;
        var currentExpenseSamples = Measure(30, _ => store.GetExpenseAnalysis("2026-02-01", "2026-02-28"));

        var seedStarted = Stopwatch.StartNew();
        store.SeedPerformanceEvents(100_000);
        store.RebuildExpenseProjection();
        var seed100kMs = seedStarted.Elapsed.TotalMilliseconds;
        var timeline100kSamples = Measure(30, _ => store.GetActivity(1, 50));

        var coldSamples = new List<double>(30);
        ExpenseAnalysisView? coldResult = null;
        for (var index = 0; index < 30; index++)
        {
            using var coldStore = LedgerStore.Open(databasePath);
            var started = Stopwatch.StartNew();
            var result = coldStore.GetExpenseAnalysis("2026-01-01", "2026-12-31");
            coldSamples.Add(started.Elapsed.TotalMilliseconds);
            coldResult ??= result;
        }

        var warmSamples = Measure(30, _ => store.GetExpenseAnalysis("2026-01-01", "2026-12-31"));
        var responseBytes = JsonSerializer.SerializeToUtf8Bytes(coldResult!.QueryResult).Length;
        store.CheckpointWal();
        var resultPayload = new
        {
            runtime_warmup_ms = runtimeWarmupMs,
            migration_open_ms = migrationOpenMs,
            import_10k_ms = import.ElapsedMs,
            import_10k_rows = import.RowCount,
            write_ms_raw = writeSamples,
            write_p95_ms = P95(writeSamples),
            page_ms_raw = pageSamples,
            page_p95_ms = P95(pageSamples),
            export_ms = exportMs,
            export_rows = export.RowCount,
            current_database_bytes = currentDatabaseBytes,
            current_expense_ms_raw = currentExpenseSamples,
            current_expense_p95_ms = P95(currentExpenseSamples),
            seed_100k_ms = seed100kMs,
            timeline_100k_ms_raw = timeline100kSamples,
            timeline_100k_p95_ms = P95(timeline100kSamples),
            query_100k_cold_ms_raw = coldSamples,
            query_100k_cold_p95_ms = P95(coldSamples),
            query_100k_warm_ms_raw = warmSamples,
            query_100k_warm_p95_ms = P95(warmSamples),
            query_100k_response_bytes = responseBytes,
            database_100k_bytes = new FileInfo(databasePath).Length,
            sqlite_version = store.GetStatus().SqliteVersion,
        };
        Console.WriteLine(JsonSerializer.Serialize(resultPayload, IndentedJson));
    }

    private static List<double> Measure<T>(int count, Func<int, T> operation)
    {
        var samples = new List<double>(count);
        for (var index = 0; index < count; index++)
        {
            var started = Stopwatch.StartNew();
            _ = operation(index);
            samples.Add(started.Elapsed.TotalMilliseconds);
        }

        return samples;
    }

    private static double P95(IReadOnlyCollection<double> values)
    {
        var sorted = values.Order().ToArray();
        var index = Math.Max(0, (int)Math.Ceiling(sorted.Length * 0.95) - 1);
        return sorted[index];
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
}
