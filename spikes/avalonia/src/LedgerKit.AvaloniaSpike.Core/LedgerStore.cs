using System.Collections.ObjectModel;
using System.Globalization;
using System.Text.Json.Nodes;
using Microsoft.Data.Sqlite;

namespace LedgerKit.AvaloniaSpike.Core;

public sealed class LedgerStore : IDisposable
{
    public const long SchemaVersion = 2;
    public const string CalculationVersion = "ledger-calculation-v1";
    public const string ProjectionVersion = "projection-v1";

    private readonly object syncRoot = new();
    private readonly SqliteConnection connection;
    private bool disposed;

    private LedgerStore(string path, SqliteConnection connection)
    {
        DatabasePath = path;
        this.connection = connection;
    }

    public string DatabasePath { get; }

    public static LedgerStore Open(string path)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        var fullPath = Path.GetFullPath(path);
        Directory.CreateDirectory(Path.GetDirectoryName(fullPath)!);
        var existed = File.Exists(fullPath) && new FileInfo(fullPath).Length > 0;
        var connection = new SqliteConnection(
            new SqliteConnectionStringBuilder
            {
                DataSource = fullPath,
                Mode = SqliteOpenMode.ReadWriteCreate,
                Cache = SqliteCacheMode.Private,
                Pooling = false,
            }.ToString());

        try
        {
            connection.Open();
            ExecuteBatch(connection, "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;");
            var version = ScalarLong(connection, null, "PRAGMA user_version");
            if (version == 0)
            {
                if (existed)
                {
                    CreatePreMigrationBackup(connection, fullPath);
                }

                ApplySchema(connection);
            }
            else if (version == 1)
            {
                CreatePreMigrationBackup(connection, fullPath);
                ApplySchemaV2(connection);
            }
            else if (version != SchemaVersion)
            {
                throw new SpikeException("DATABASE_VERSION_UNSUPPORTED", "Database schema version is unsupported.");
            }

            VerifyConnection(connection);
            return new LedgerStore(fullPath, connection);
        }
        catch
        {
            connection.Dispose();
            throw;
        }
    }

    public void InitializeDemo()
    {
        lock (syncRoot)
        {
            EnsureNotDisposed();
            if (ScalarLong(connection, null, "SELECT COUNT(*) FROM business_events") != 0)
            {
                return;
            }

            SeedExpenseFixtureCore();
            PostEventCore(
                new PostEventRequest(
                    "Income",
                    "2026-02-01",
                    "cash-cny-1",
                    "2000",
                    "CNY",
                    "cat-income",
                    Note: "Synthetic opening income for the disposable spike"),
                null,
                false);
        }
    }

    public LedgerStatus GetStatus()
    {
        lock (syncRoot)
        {
            EnsureNotDisposed();
            var fileBytes = File.Exists(DatabasePath) ? new FileInfo(DatabasePath).Length : 0;
            return new LedgerStatus(
                ScalarLong(connection, null, "PRAGMA user_version"),
                ScalarString(connection, null, "SELECT sqlite_version()"),
                EventWatermark(),
                ProjectionWatermark(),
                fileBytes,
                CalculationVersion,
                false);
        }
    }

    public PostEventResponse PostEvent(PostEventRequest request)
    {
        lock (syncRoot)
        {
            EnsureNotDisposed();
            return PostEventCore(request, null, false);
        }
    }

    public ActivityPage GetActivity(uint page, uint pageSize)
    {
        if (page == 0 || pageSize == 0 || pageSize > 50)
        {
            throw new SpikeException("PAGE_INVALID", "Page request is outside the bounded range.");
        }

        lock (syncRoot)
        {
            EnsureNotDisposed();
            var totalCount = checked((ulong)ScalarLong(connection, null, "SELECT COUNT(*) FROM business_events"));
            var offset = checked((long)((page - 1) * pageSize));
            using var command = CreateCommand(
                connection,
                null,
                """
                SELECT e.event_id, e.event_type, e.effective_date, e.sequence, e.account_id,
                       e.amount, e.signed_amount, e.currency, e.category_id, c.label, e.note,
                       e.event_order
                FROM business_events e
                LEFT JOIN categories c ON c.category_id = e.category_id
                ORDER BY e.effective_date DESC, e.sequence DESC, e.event_id DESC
                LIMIT $page_size OFFSET $offset
                """,
                ("$page_size", pageSize),
                ("$offset", offset));
            using var reader = command.ExecuteReader();
            var items = new List<EventRecord>();
            while (reader.Read())
            {
                items.Add(ReadEvent(reader));
            }

            return new ActivityPage(
                new ReadOnlyCollection<EventRecord>(items),
                page,
                pageSize,
                totalCount,
                checked((ulong)offset) + pageSize < totalCount);
        }
    }

    public IReadOnlyList<EventRecord> GetAllActivity()
    {
        var events = new List<EventRecord>();
        uint page = 1;
        while (true)
        {
            var result = GetActivity(page, 50);
            events.AddRange(result.Items);
            if (!result.HasMore)
            {
                return new ReadOnlyCollection<EventRecord>(events);
            }

            page++;
        }
    }

    public Overview GetOverview()
    {
        lock (syncRoot)
        {
            EnsureNotDisposed();
            var cashValue = ScalarStringOrDefault(
                connection,
                null,
                "SELECT balance FROM cash_balance_projection WHERE account_id = 'cash-cny-1'",
                "0") ?? "0";
            return new Overview("CNY", cashValue, cashValue, "0", 100, EventWatermark());
        }
    }

    public ExpenseAnalysisView GetExpenseAnalysis(string startDate, string endDate)
    {
        ValidateDate(startDate);
        ValidateDate(endDate);
        if (string.CompareOrdinal(startDate, endDate) > 0)
        {
            throw new SpikeException("DATE_INVALID", "Start date must not follow end date.");
        }

        lock (syncRoot)
        {
            EnsureNotDisposed();
            var result = BuildExpenseAnalysis(startDate, endDate);
            var sourceRows = result["top10"]!["items"]!.AsArray().ToList();
            var other = result["top10"]!["other"];
            if (other is not null)
            {
                sourceRows.Add(other);
            }

            var maximum = sourceRows.Count == 0
                ? decimal.One
                : DecimalContract.ParseStored(sourceRows[0]!["amount"]!.GetValue<string>());
            var chartRows = sourceRows.Select(row =>
            {
                var amountText = row!["amount"]!.GetValue<string>();
                var amount = DecimalContract.ParseStored(amountText);
                var basisPoints = checked((uint)Math.Max(
                    200,
                    DecimalContract.RoundHalfUp(amount * 10_000m / maximum, 0)));
                return new ExpenseChartRow(
                    row["bucket_id"]!.GetValue<string>(),
                    row["label"]!.GetValue<string>(),
                    amountText,
                    row["distinct_event_count"]!.GetValue<ulong>(),
                    basisPoints);
            }).ToArray();
            return new ExpenseAnalysisView(result, chartRows);
        }
    }

    public void RebuildExpenseProjection()
    {
        lock (syncRoot)
        {
            EnsureNotDisposed();
            using var transaction = connection.BeginTransaction();
            RebuildExpenseProjectionCore(transaction);
            transaction.Commit();
        }
    }

    internal PostEventResponse PostEventForCheck(
        PostEventRequest request,
        string eventId,
        bool failAfterPosting = false)
    {
        lock (syncRoot)
        {
            EnsureNotDisposed();
            return PostEventCore(request, eventId, failAfterPosting);
        }
    }

    internal void SeedExpenseFixtureForCheck()
    {
        lock (syncRoot)
        {
            EnsureNotDisposed();
            SeedExpenseFixtureCore();
        }
    }

    internal JsonObject GetFixtureProjection(string periodIncome, string periodExpense)
    {
        lock (syncRoot)
        {
            var balance = ScalarStringOrDefault(
                connection,
                null,
                "SELECT balance FROM cash_balance_projection WHERE account_id = 'cash-cny-1'",
                "0");
            return new JsonObject
            {
                ["projection_version"] = ProjectionVersion,
                ["event_watermark"] = EventWatermark(),
                ["state"] = new JsonObject
                {
                    ["account_balances"] = new JsonArray(
                        new JsonObject
                        {
                            ["account_id"] = "cash-cny-1",
                            ["balance"] = balance,
                            ["currency"] = "CNY",
                        }),
                    ["period_income"] = periodIncome,
                    ["period_expense"] = periodExpense,
                },
            };
        }
    }

    internal void BackupDatabase(string destinationPath)
    {
        lock (syncRoot)
        {
            EnsureNotDisposed();
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(destinationPath))!);
            using var destination = new SqliteConnection($"Data Source={destinationPath};Mode=ReadWriteCreate;Pooling=False");
            destination.Open();
            connection.BackupDatabase(destination);
        }
    }

    internal void RestoreDatabase(string sourcePath)
    {
        lock (syncRoot)
        {
            EnsureNotDisposed();
            using var source = new SqliteConnection($"Data Source={sourcePath};Mode=ReadOnly;Pooling=False");
            source.Open();
            source.BackupDatabase(connection);
            VerifyConnection(connection);
            if (ScalarLong(connection, null, "PRAGMA user_version") != SchemaVersion)
            {
                throw new SpikeException("BACKUP_INTEGRITY_FAILED", "Restored schema version is invalid.");
            }
        }
    }

    internal void CheckpointWal()
    {
        lock (syncRoot)
        {
            ExecuteBatch(connection, "PRAGMA wal_checkpoint(TRUNCATE);");
        }
    }

    internal void SeedPerformanceEvents(int count)
    {
        lock (syncRoot)
        {
            EnsureNotDisposed();
            using var transaction = connection.BeginTransaction();
            for (var index = 1; index <= 20; index++)
            {
                Execute(
                    connection,
                    transaction,
                    "INSERT OR REPLACE INTO categories(category_id, label, archived) VALUES ($id, $label, 0)",
                    ("$id", $"perf-cat-{index:00}"),
                    ("$label", $"Performance category {index:00}"));
            }

            Execute(
                connection,
                transaction,
                """
                WITH RECURSIVE sequence(n) AS (
                    SELECT 1 UNION ALL SELECT n + 1 FROM sequence WHERE n < $count
                )
                INSERT INTO business_events(
                    event_id, event_type, effective_date, sequence, account_id, amount,
                    signed_amount, currency, category_id, note, calculation_version
                )
                SELECT
                    printf('perf-event-%06d', n), 'Expense',
                    printf('2026-%02d-%02d', ((n - 1) % 12) + 1, ((n - 1) % 28) + 1),
                    n, 'cash-cny-1', '1.00', '-1.00', 'CNY',
                    printf('perf-cat-%02d', ((n - 1) % 20) + 1),
                    'Synthetic 100k query benchmark', 'ledger-calculation-v1'
                FROM sequence
                """,
                ("$count", count));
            Execute(
                connection,
                transaction,
                """
                INSERT INTO ledger_postings(
                    posting_id, event_id, posting_kind, account_id, quantity_delta,
                    currency, base_value, base_currency, calculation_version
                )
                SELECT
                    'post-' || event_id || '-01', event_id, 'cash', account_id,
                    signed_amount, currency, signed_amount, 'CNY', calculation_version
                FROM business_events WHERE event_id LIKE 'perf-event-%'
                """);
            Execute(
                connection,
                transaction,
                """
                UPDATE projection_state
                SET event_watermark = (SELECT MAX(event_order) FROM business_events),
                    calculation_version = 'ledger-calculation-v1'
                WHERE projection_name = 'cash-balances'
                """);
            transaction.Commit();
            ExecuteBatch(connection, "PRAGMA wal_checkpoint(TRUNCATE);");
        }
    }

    public void Dispose()
    {
        lock (syncRoot)
        {
            if (disposed)
            {
                return;
            }

            connection.Dispose();
            disposed = true;
        }

        GC.SuppressFinalize(this);
    }

    public static void VerifyDatabaseFile(string path)
    {
        using var candidate = new SqliteConnection($"Data Source={Path.GetFullPath(path)};Mode=ReadOnly;Pooling=False");
        candidate.Open();
        VerifyConnection(candidate);
    }

    private static void CreatePreMigrationBackup(SqliteConnection source, string livePath)
    {
        var backupPath = Path.ChangeExtension(livePath, ".pre-migration.sqlite");
        using var destination = new SqliteConnection($"Data Source={backupPath};Mode=ReadWriteCreate;Pooling=False");
        destination.Open();
        source.BackupDatabase(destination);
        VerifyConnection(destination);
    }

    private static void ApplySchema(SqliteConnection target)
    {
        using var transaction = target.BeginTransaction();
        ExecuteBatch(
            target,
            transaction,
            """
            CREATE TABLE business_events (
                event_order INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL UNIQUE,
                event_type TEXT NOT NULL CHECK (event_type IN ('Income', 'Expense')),
                effective_date TEXT NOT NULL,
                sequence INTEGER NOT NULL CHECK (sequence > 0),
                account_id TEXT NOT NULL,
                amount TEXT NOT NULL,
                signed_amount TEXT NOT NULL,
                currency TEXT NOT NULL,
                category_id TEXT,
                note TEXT,
                calculation_version TEXT NOT NULL,
                created_at_utc TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE (effective_date, sequence, event_id)
            );
            CREATE TABLE ledger_postings (
                posting_id TEXT PRIMARY KEY,
                event_id TEXT NOT NULL UNIQUE REFERENCES business_events(event_id),
                posting_kind TEXT NOT NULL,
                account_id TEXT NOT NULL,
                quantity_delta TEXT NOT NULL,
                currency TEXT NOT NULL,
                base_value TEXT NOT NULL,
                base_currency TEXT NOT NULL,
                calculation_version TEXT NOT NULL
            );
            CREATE TABLE cash_balance_projection (
                account_id TEXT PRIMARY KEY,
                balance TEXT NOT NULL,
                currency TEXT NOT NULL,
                event_watermark INTEGER NOT NULL,
                calculation_version TEXT NOT NULL
            );
            CREATE TABLE expense_daily_projection (
                effective_date TEXT NOT NULL,
                category_id TEXT NOT NULL,
                amount TEXT NOT NULL,
                distinct_event_count INTEGER NOT NULL CHECK (distinct_event_count >= 0),
                event_watermark INTEGER NOT NULL,
                calculation_version TEXT NOT NULL,
                PRIMARY KEY (effective_date, category_id)
            );
            CREATE TABLE categories (
                category_id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1))
            );
            CREATE TABLE projection_state (
                projection_name TEXT PRIMARY KEY,
                event_watermark INTEGER NOT NULL,
                calculation_version TEXT NOT NULL
            );
            CREATE TABLE app_metadata (
                metadata_key TEXT PRIMARY KEY,
                metadata_value TEXT NOT NULL
            );
            CREATE INDEX idx_business_events_activity
                ON business_events(effective_date DESC, sequence DESC, event_id DESC);
            CREATE INDEX idx_business_events_expense
                ON business_events(event_type, effective_date, category_id, amount);
            CREATE INDEX idx_expense_daily_projection_range
                ON expense_daily_projection(effective_date, category_id, amount, distinct_event_count);
            INSERT INTO projection_state(projection_name, event_watermark, calculation_version)
                VALUES ('cash-balances', 0, 'ledger-calculation-v1');
            INSERT INTO projection_state(projection_name, event_watermark, calculation_version)
                VALUES ('expense-daily', 0, 'ledger-calculation-v1');
            INSERT INTO app_metadata(metadata_key, metadata_value)
                VALUES ('master_data_watermark', '0');
            PRAGMA user_version = 2;
            """);
        transaction.Commit();
    }

    private static void ApplySchemaV2(SqliteConnection target)
    {
        using var transaction = target.BeginTransaction();
        ExecuteBatch(
            target,
            transaction,
            """
            CREATE TABLE expense_daily_projection (
                effective_date TEXT NOT NULL,
                category_id TEXT NOT NULL,
                amount TEXT NOT NULL,
                distinct_event_count INTEGER NOT NULL CHECK (distinct_event_count >= 0),
                event_watermark INTEGER NOT NULL,
                calculation_version TEXT NOT NULL,
                PRIMARY KEY (effective_date, category_id)
            );
            CREATE INDEX idx_expense_daily_projection_range
                ON expense_daily_projection(effective_date, category_id, amount, distinct_event_count);
            INSERT INTO projection_state(projection_name, event_watermark, calculation_version)
                VALUES ('expense-daily', 0, 'ledger-calculation-v1');
            PRAGMA user_version = 2;
            """);
        transaction.Commit();
    }

    private PostEventResponse PostEventCore(
        PostEventRequest request,
        string? explicitEventId,
        bool failAfterPosting)
    {
        ArgumentNullException.ThrowIfNull(request);
        ValidateDate(request.EffectiveDate);
        if (request.Currency != "CNY" || request.AccountId != "cash-cny-1")
        {
            throw new SpikeException("EVENT_TYPE_UNSUPPORTED", "Only the synthetic CNY account is supported.");
        }

        if (request.Note?.Length > 200)
        {
            throw new SpikeException("EVENT_TYPE_UNSUPPORTED", "Note exceeds the spike boundary.");
        }

        var amount = DecimalContract.ValidatePositiveAmount(
            request.Amount,
            request.CurrencyPrecisionConfirmed);
        var signed = request.EventType switch
        {
            "Income" => amount.Value,
            "Expense" when request.CategoryId is not null => -amount.Value,
            _ => throw new SpikeException("EVENT_TYPE_UNSUPPORTED", "Event type is unsupported."),
        };
        var signedText = DecimalContract.Format(signed);

        using var transaction = connection.BeginTransaction();
        var nextOrder = ScalarLong(
            connection,
            transaction,
            "SELECT COALESCE(MAX(event_order), 0) + 1 FROM business_events");
        var eventId = explicitEventId ?? $"evt-spike-{nextOrder:000000}";
        var sequence = checked((uint)ScalarLong(
            connection,
            transaction,
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM business_events WHERE effective_date = $date",
            ("$date", request.EffectiveDate)));
        Execute(
            connection,
            transaction,
            """
            INSERT INTO business_events(
                event_id, event_type, effective_date, sequence, account_id, amount,
                signed_amount, currency, category_id, note, calculation_version
            ) VALUES ($event_id, $event_type, $date, $sequence, $account_id, $amount,
                      $signed_amount, $currency, $category_id, $note, $calculation_version)
            """,
            ("$event_id", eventId),
            ("$event_type", request.EventType),
            ("$date", request.EffectiveDate),
            ("$sequence", sequence),
            ("$account_id", request.AccountId),
            ("$amount", amount.Text),
            ("$signed_amount", signedText),
            ("$currency", request.Currency),
            ("$category_id", request.CategoryId),
            ("$note", request.Note),
            ("$calculation_version", CalculationVersion));

        var posting = new Posting(
            $"post-{eventId}-01",
            eventId,
            "cash",
            CalculationVersion,
            request.AccountId,
            signedText,
            request.Currency,
            signedText,
            "CNY");
        Execute(
            connection,
            transaction,
            """
            INSERT INTO ledger_postings(
                posting_id, event_id, posting_kind, account_id, quantity_delta,
                currency, base_value, base_currency, calculation_version
            ) VALUES ($posting_id, $event_id, $posting_kind, $account_id, $quantity_delta,
                      $currency, $base_value, $base_currency, $calculation_version)
            """,
            ("$posting_id", posting.PostingId),
            ("$event_id", posting.EventId),
            ("$posting_kind", posting.PostingKind),
            ("$account_id", posting.AccountId),
            ("$quantity_delta", posting.QuantityDelta),
            ("$currency", posting.Currency),
            ("$base_value", posting.BaseValue),
            ("$base_currency", posting.BaseCurrency),
            ("$calculation_version", posting.CalculationVersion));
        if (failAfterPosting)
        {
            throw new SpikeException("SYNTHETIC_FAILPOINT", "Synthetic failpoint rolled back the transaction.");
        }

        if (request.EventType == "Expense")
        {
            var existingAmount = ScalarStringOrDefault(
                connection,
                transaction,
                """
                SELECT amount FROM expense_daily_projection
                WHERE effective_date = $date AND category_id = $category_id
                """,
                "0",
                ("$date", request.EffectiveDate),
                ("$category_id", request.CategoryId));
            var existingCount = ScalarLongOrDefault(
                connection,
                transaction,
                """
                SELECT distinct_event_count FROM expense_daily_projection
                WHERE effective_date = $date AND category_id = $category_id
                """,
                0,
                ("$date", request.EffectiveDate),
                ("$category_id", request.CategoryId));
            var projectedAmount = DecimalContract.ParseStored(existingAmount ?? "0") + amount.Value;
            Execute(
                connection,
                transaction,
                """
                INSERT INTO expense_daily_projection(
                    effective_date, category_id, amount, distinct_event_count,
                    event_watermark, calculation_version
                ) VALUES ($date, $category_id, $amount, $count, $watermark, $version)
                ON CONFLICT(effective_date, category_id) DO UPDATE SET
                    amount = excluded.amount,
                    distinct_event_count = excluded.distinct_event_count,
                    event_watermark = excluded.event_watermark,
                    calculation_version = excluded.calculation_version
                """,
                ("$date", request.EffectiveDate),
                ("$category_id", request.CategoryId),
                ("$amount", DecimalContract.Format(projectedAmount)),
                ("$count", existingCount + 1),
                ("$watermark", nextOrder),
                ("$version", CalculationVersion));
            Execute(
                connection,
                transaction,
                """
                UPDATE projection_state SET event_watermark = $watermark, calculation_version = $version
                WHERE projection_name = 'expense-daily'
                """,
                ("$watermark", nextOrder),
                ("$version", CalculationVersion));
        }

        var existingBalance = ScalarStringOrDefault(
            connection,
            transaction,
            "SELECT balance FROM cash_balance_projection WHERE account_id = $account_id",
            "0",
            ("$account_id", request.AccountId));
        var newBalance = DecimalContract.ParseStored(existingBalance ?? "0") + signed;
        var balanceText = DecimalContract.Format(newBalance);
        Execute(
            connection,
            transaction,
            """
            INSERT INTO cash_balance_projection(
                account_id, balance, currency, event_watermark, calculation_version
            ) VALUES ($account_id, $balance, $currency, $watermark, $version)
            ON CONFLICT(account_id) DO UPDATE SET
                balance = excluded.balance,
                event_watermark = excluded.event_watermark,
                calculation_version = excluded.calculation_version
            """,
            ("$account_id", request.AccountId),
            ("$balance", balanceText),
            ("$currency", request.Currency),
            ("$watermark", nextOrder),
            ("$version", CalculationVersion));
        Execute(
            connection,
            transaction,
            """
            UPDATE projection_state SET event_watermark = $watermark, calculation_version = $version
            WHERE projection_name = 'cash-balances'
            """,
            ("$watermark", nextOrder),
            ("$version", CalculationVersion));
        transaction.Commit();

        var categoryLabel = request.CategoryId is null
            ? null
            : ScalarStringOrDefault(
                connection,
                null,
                "SELECT label FROM categories WHERE category_id = $category_id",
                null,
                ("$category_id", request.CategoryId));
        var eventRecord = new EventRecord(
            eventId,
            request.EventType,
            request.EffectiveDate,
            sequence,
            request.AccountId,
            request.Amount,
            signedText,
            request.Currency,
            request.CategoryId,
            categoryLabel,
            request.Note,
            checked((ulong)nextOrder));
        return new PostEventResponse(eventRecord, posting, balanceText, checked((ulong)nextOrder));
    }

    private JsonObject BuildExpenseAnalysis(string startDate, string endDate)
    {
        var categories = new Dictionary<string, (string Label, bool Archived)>(StringComparer.Ordinal);
        using (var command = CreateCommand(
                   connection,
                   null,
                   "SELECT category_id, label, archived FROM categories ORDER BY category_id"))
        using (var reader = command.ExecuteReader())
        {
            while (reader.Read())
            {
                categories.Add(reader.GetString(0), (reader.GetString(1), reader.GetInt64(2) != 0));
            }
        }

        var aggregates = new Dictionary<string, BucketAggregate>(StringComparer.Ordinal);
        using (var command = CreateCommand(
                   connection,
                   null,
                   """
                   SELECT category_id, amount, distinct_event_count
                   FROM expense_daily_projection
                   WHERE effective_date BETWEEN $start_date AND $end_date
                   ORDER BY category_id
                   """,
                   ("$start_date", startDate),
                   ("$end_date", endDate)))
        using (var reader = command.ExecuteReader())
        {
            while (reader.Read())
            {
                var bucketId = reader.GetString(0);
                if (!aggregates.TryGetValue(bucketId, out var aggregate))
                {
                    var category = categories.TryGetValue(bucketId, out var found)
                        ? found
                        : (bucketId, false);
                    aggregate = new BucketAggregate(category.Item1, category.Item2);
                    aggregates.Add(bucketId, aggregate);
                }

                aggregate.Amount += DecimalContract.ParseStored(reader.GetString(1));
                aggregate.DistinctEventCount += checked((ulong)reader.GetInt64(2));
            }
        }

        var watermark = EventWatermark();
        var buckets = aggregates
            .Select(pair => CreateBucket(
                pair.Key,
                pair.Value.Label,
                pair.Value.Archived,
                pair.Value.Amount,
                pair.Value.DistinctEventCount,
                startDate,
                endDate,
                watermark))
            .OrderByDescending(bucket => DecimalContract.ParseStored(bucket["amount"]!.GetValue<string>()))
            .ThenBy(bucket => bucket["bucket_id"]!.GetValue<string>(), StringComparer.Ordinal)
            .ToArray();
        var total = aggregates.Values.Aggregate(decimal.Zero, static (sum, bucket) => sum + bucket.Amount);
        var globalCount = aggregates.Values.Aggregate(0UL, static (sum, bucket) => sum + bucket.DistinctEventCount);

        var allBuckets = new JsonArray(buckets.Select(static bucket => (JsonNode)bucket).ToArray());
        var topItems = new JsonArray();
        foreach (var bucket in buckets.Take(10))
        {
            topItems.Add((JsonNode)new JsonObject
            {
                ["bucket_id"] = bucket["bucket_id"]!.GetValue<string>(),
                ["label"] = bucket["label"]!.GetValue<string>(),
                ["amount"] = bucket["amount"]!.GetValue<string>(),
                ["distinct_event_count"] = bucket["distinct_event_count"]!.GetValue<ulong>(),
                ["drilldown_context"] = bucket["drilldown_context"]!.DeepClone(),
            });
        }

        JsonObject? other = null;
        if (buckets.Length > 10)
        {
            var remainder = buckets.Skip(10).ToArray();
            var otherAmount = remainder.Aggregate(
                decimal.Zero,
                static (sum, bucket) => sum + DecimalContract.ParseStored(bucket["amount"]!.GetValue<string>()));
            var otherCount = remainder.Aggregate(
                0UL,
                static (sum, bucket) => sum + bucket["distinct_event_count"]!.GetValue<ulong>());
            other = new JsonObject
            {
                ["bucket_id"] = "system:top10-other",
                ["label"] = "Other categories",
                ["amount"] = DecimalContract.Format(otherAmount),
                ["distinct_event_count"] = otherCount,
                ["drilldown_context"] = new JsonObject
                {
                    ["start_date"] = startDate,
                    ["end_date"] = endDate,
                    ["event_watermark"] = watermark,
                    ["calculation_version"] = CalculationVersion,
                    ["expense_policy_version"] = "expense-policy-v1",
                    ["bucket_id"] = "system:top10-other",
                    ["member_rank_gt"] = 10,
                    ["valuation_state"] = "valued",
                },
            };
        }

        JsonObject? largestCategory = buckets.Length == 0
            ? null
            : new JsonObject
            {
                ["bucket_id"] = buckets[0]["bucket_id"]!.GetValue<string>(),
                ["amount"] = buckets[0]["amount"]!.GetValue<string>(),
            };
        var masterDataWatermark = ulong.Parse(
            ScalarString(connection, null, "SELECT metadata_value FROM app_metadata WHERE metadata_key = 'master_data_watermark'"),
            CultureInfo.InvariantCulture);
        var result = new JsonObject
        {
            ["contract"] = "expense-analysis-query-result/v1",
            ["query"] = new JsonObject
            {
                ["start_date"] = startDate,
                ["end_date"] = endDate,
                ["base_currency"] = "CNY",
            },
            ["summary"] = new JsonObject
            {
                ["label"] = "Total expense",
                ["total_expense"] = DecimalContract.Format(total),
                ["valued_subtotal"] = DecimalContract.Format(total),
                ["global_distinct_event_count"] = globalCount,
                ["largest_category"] = largestCategory,
            },
            ["buckets"] = allBuckets,
            ["top10"] = new JsonObject
            {
                ["items"] = topItems,
                ["other"] = other,
            },
            ["refunds"] = new JsonObject
            {
                ["refund"] = EmptySemanticSummary(startDate, endDate, watermark, "refund"),
                ["reimbursement"] = EmptySemanticSummary(startDate, endDate, watermark, "reimbursement"),
            },
            ["unvalued"] = new JsonObject
            {
                ["expense_count"] = 0,
                ["drilldown_context"] = new JsonObject
                {
                    ["start_date"] = startDate,
                    ["end_date"] = endDate,
                    ["event_watermark"] = watermark,
                    ["calculation_version"] = CalculationVersion,
                    ["expense_policy_version"] = "expense-policy-v1",
                    ["semantic_role"] = "expense",
                    ["valuation_state"] = "unvalued",
                },
            },
            ["watermarks"] = new JsonObject
            {
                ["event"] = watermark,
                ["master_data"] = masterDataWatermark,
            },
            ["versions"] = new JsonObject
            {
                ["calculation"] = CalculationVersion,
                ["expense_policy"] = "expense-policy-v1",
                ["bucket_policy"] = "expense-bucket-policy-v1",
                ["refund_policy"] = "refund-policy-v1",
            },
            ["canonicalization"] = "ledgerkit-canonical-json-v1",
        };
        result["canonical_hash"] = CanonicalJson.Hash(result);
        return result;
    }

    private static JsonObject CreateBucket(
        string bucketId,
        string label,
        bool archived,
        decimal amount,
        ulong count,
        string startDate,
        string endDate,
        ulong watermark) => new()
        {
            ["bucket_id"] = bucketId,
            ["bucket_kind"] = "category",
            ["label"] = label,
            ["archived"] = archived,
            ["amount"] = DecimalContract.Format(amount),
            ["distinct_event_count"] = count,
            ["drilldown_context"] = BucketContext(startDate, endDate, watermark, bucketId),
        };

    private static JsonObject BucketContext(
        string startDate,
        string endDate,
        ulong watermark,
        string bucketId) => new()
        {
            ["start_date"] = startDate,
            ["end_date"] = endDate,
            ["event_watermark"] = watermark,
            ["calculation_version"] = CalculationVersion,
            ["expense_policy_version"] = "expense-policy-v1",
            ["bucket_id"] = bucketId,
            ["valuation_state"] = "valued",
        };

    private static JsonObject EmptySemanticSummary(
        string startDate,
        string endDate,
        ulong watermark,
        string semanticRole) => new()
        {
            ["amount"] = "0",
            ["distinct_event_count"] = 0,
            ["unvalued_count"] = 0,
            ["drilldown_context"] = new JsonObject
            {
                ["start_date"] = startDate,
                ["end_date"] = endDate,
                ["event_watermark"] = watermark,
                ["calculation_version"] = CalculationVersion,
                ["expense_policy_version"] = "expense-policy-v1",
                ["semantic_role"] = semanticRole,
                ["valuation_state"] = "all",
            },
        };

    private void SeedExpenseFixtureCore()
    {
        using (var transaction = connection.BeginTransaction())
        {
            for (var index = 1; index <= 12; index++)
            {
                Execute(
                    connection,
                    transaction,
                    "INSERT OR REPLACE INTO categories(category_id, label, archived) VALUES ($id, $label, 0)",
                    ("$id", $"cat-{index:00}"),
                    ("$label", $"Category {index:00}"));
            }

            Execute(
                connection,
                transaction,
                "UPDATE app_metadata SET metadata_value = '1' WHERE metadata_key = 'master_data_watermark'");
            transaction.Commit();
        }

        for (var index = 1; index <= 12; index++)
        {
            PostEventCore(
                new PostEventRequest(
                    "Expense",
                    $"2026-02-{index:00}",
                    "cash-cny-1",
                    (130 - (index * 10)).ToString(CultureInfo.InvariantCulture),
                    "CNY",
                    $"cat-{index:00}",
                    Note: $"Synthetic category {index:00}"),
                $"evt-expense-top10-{index:00}",
                false);
        }
    }

    private void RebuildExpenseProjectionCore(SqliteTransaction transaction)
    {
        Execute(connection, transaction, "DELETE FROM expense_daily_projection");
        var aggregates = new Dictionary<(string Date, string Category), ProjectionAggregate>();
        long expenseWatermark = 0;
        using (var command = CreateCommand(
                   connection,
                   transaction,
                   """
                   SELECT effective_date, category_id, amount, event_order
                   FROM business_events WHERE event_type = 'Expense'
                   ORDER BY effective_date, category_id, event_order
                   """))
        using (var reader = command.ExecuteReader())
        {
            while (reader.Read())
            {
                var key = (reader.GetString(0), reader.GetString(1));
                if (!aggregates.TryGetValue(key, out var aggregate))
                {
                    aggregate = new ProjectionAggregate();
                    aggregates.Add(key, aggregate);
                }

                aggregate.Amount += DecimalContract.ParseStored(reader.GetString(2));
                aggregate.Count++;
                aggregate.Watermark = Math.Max(aggregate.Watermark, reader.GetInt64(3));
                expenseWatermark = Math.Max(expenseWatermark, aggregate.Watermark);
            }
        }

        foreach (var pair in aggregates)
        {
            Execute(
                connection,
                transaction,
                """
                INSERT INTO expense_daily_projection(
                    effective_date, category_id, amount, distinct_event_count,
                    event_watermark, calculation_version
                ) VALUES ($date, $category, $amount, $count, $watermark, $version)
                """,
                ("$date", pair.Key.Date),
                ("$category", pair.Key.Category),
                ("$amount", DecimalContract.Format(pair.Value.Amount)),
                ("$count", pair.Value.Count),
                ("$watermark", pair.Value.Watermark),
                ("$version", CalculationVersion));
        }

        Execute(
            connection,
            transaction,
            """
            UPDATE projection_state SET event_watermark = $watermark, calculation_version = $version
            WHERE projection_name = 'expense-daily'
            """,
            ("$watermark", expenseWatermark),
            ("$version", CalculationVersion));
    }

    private static EventRecord ReadEvent(SqliteDataReader reader) => new(
        reader.GetString(0),
        reader.GetString(1),
        reader.GetString(2),
        checked((uint)reader.GetInt64(3)),
        reader.GetString(4),
        reader.GetString(5),
        reader.GetString(6),
        reader.GetString(7),
        reader.IsDBNull(8) ? null : reader.GetString(8),
        reader.IsDBNull(9) ? null : reader.GetString(9),
        reader.IsDBNull(10) ? null : reader.GetString(10),
        checked((ulong)reader.GetInt64(11)));

    private ulong EventWatermark() => checked((ulong)ScalarLong(
        connection,
        null,
        "SELECT COALESCE(MAX(event_order), 0) FROM business_events"));

    private ulong ProjectionWatermark() => checked((ulong)ScalarLong(
        connection,
        null,
        "SELECT event_watermark FROM projection_state WHERE projection_name = 'cash-balances'"));

    private static void ValidateDate(string value)
    {
        if (!DateOnly.TryParseExact(
                value,
                "yyyy-MM-dd",
                CultureInfo.InvariantCulture,
                DateTimeStyles.None,
                out _))
        {
            throw new SpikeException("DATE_INVALID", "Date must be a valid yyyy-MM-dd value.");
        }
    }

    private static void VerifyConnection(SqliteConnection target)
    {
        if (ScalarString(target, null, "PRAGMA integrity_check") != "ok" ||
            ScalarLong(target, null, "SELECT COUNT(*) FROM pragma_foreign_key_check") != 0)
        {
            throw new SpikeException("BACKUP_INTEGRITY_FAILED", "SQLite integrity validation failed.");
        }
    }

    private static SqliteCommand CreateCommand(
        SqliteConnection target,
        SqliteTransaction? transaction,
        string sql,
        params (string Name, object? Value)[] parameters)
    {
        var command = target.CreateCommand();
        command.Transaction = transaction;
        command.CommandText = sql;
        foreach (var parameter in parameters)
        {
            command.Parameters.AddWithValue(parameter.Name, parameter.Value ?? DBNull.Value);
        }

        return command;
    }

    private static void Execute(
        SqliteConnection target,
        SqliteTransaction? transaction,
        string sql,
        params (string Name, object? Value)[] parameters)
    {
        using var command = CreateCommand(target, transaction, sql, parameters);
        command.ExecuteNonQuery();
    }

    private static void ExecuteBatch(SqliteConnection target, string sql) =>
        ExecuteBatch(target, null, sql);

    private static void ExecuteBatch(SqliteConnection target, SqliteTransaction? transaction, string sql) =>
        Execute(target, transaction, sql);

    private static long ScalarLong(
        SqliteConnection target,
        SqliteTransaction? transaction,
        string sql,
        params (string Name, object? Value)[] parameters) =>
        Convert.ToInt64(CreateScalar(target, transaction, sql, parameters), CultureInfo.InvariantCulture);

    private static long ScalarLongOrDefault(
        SqliteConnection target,
        SqliteTransaction? transaction,
        string sql,
        long defaultValue,
        params (string Name, object? Value)[] parameters)
    {
        var value = CreateScalar(target, transaction, sql, parameters);
        return value is null || value is DBNull
            ? defaultValue
            : Convert.ToInt64(value, CultureInfo.InvariantCulture);
    }

    private static string ScalarString(
        SqliteConnection target,
        SqliteTransaction? transaction,
        string sql,
        params (string Name, object? Value)[] parameters) =>
        Convert.ToString(CreateScalar(target, transaction, sql, parameters), CultureInfo.InvariantCulture) ??
        throw new SpikeException("DATABASE_OPERATION_FAILED", "Expected SQLite scalar is absent.");

    private static string? ScalarStringOrDefault(
        SqliteConnection target,
        SqliteTransaction? transaction,
        string sql,
        string? defaultValue,
        params (string Name, object? Value)[] parameters)
    {
        var value = CreateScalar(target, transaction, sql, parameters);
        return value is null || value is DBNull
            ? defaultValue
            : Convert.ToString(value, CultureInfo.InvariantCulture);
    }

    private static object? CreateScalar(
        SqliteConnection target,
        SqliteTransaction? transaction,
        string sql,
        params (string Name, object? Value)[] parameters)
    {
        using var command = CreateCommand(target, transaction, sql, parameters);
        return command.ExecuteScalar();
    }

    private void EnsureNotDisposed() => ObjectDisposedException.ThrowIf(disposed, this);

    private sealed class BucketAggregate(string label, bool archived)
    {
        public string Label { get; } = label;

        public bool Archived { get; } = archived;

        public decimal Amount { get; set; }

        public ulong DistinctEventCount { get; set; }
    }

    private sealed class ProjectionAggregate
    {
        public decimal Amount { get; set; }

        public long Count { get; set; }

        public long Watermark { get; set; }
    }
}
