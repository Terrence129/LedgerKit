using System.Text.Json.Nodes;

namespace LedgerKit.AvaloniaSpike.Core;

public sealed record PostEventRequest(
    string EventType,
    string EffectiveDate,
    string AccountId,
    string Amount,
    string Currency,
    string? CategoryId,
    bool CurrencyPrecisionConfirmed = false,
    string? Note = null);

public sealed record Posting(
    string PostingId,
    string EventId,
    string PostingKind,
    string CalculationVersion,
    string AccountId,
    string QuantityDelta,
    string Currency,
    string BaseValue,
    string BaseCurrency)
{
    public JsonObject ToJson() => new()
    {
        ["posting_id"] = PostingId,
        ["event_id"] = EventId,
        ["posting_kind"] = PostingKind,
        ["calculation_version"] = CalculationVersion,
        ["account_id"] = AccountId,
        ["quantity_delta"] = QuantityDelta,
        ["currency"] = Currency,
        ["base_value"] = BaseValue,
        ["base_currency"] = BaseCurrency,
    };
}

public sealed record EventRecord(
    string EventId,
    string EventType,
    string EffectiveDate,
    uint Sequence,
    string AccountId,
    string Amount,
    string SignedAmount,
    string Currency,
    string? CategoryId,
    string? CategoryLabel,
    string? Note,
    ulong EventWatermark);

public sealed record PostEventResponse(
    EventRecord Event,
    Posting Posting,
    string AccountBalance,
    ulong ProjectionWatermark);

public sealed record ActivityPage(
    IReadOnlyList<EventRecord> Items,
    uint Page,
    uint PageSize,
    ulong TotalCount,
    bool HasMore);

public sealed record Overview(
    string BaseCurrency,
    string NetWorth,
    string CashValue,
    string SecurityValue,
    byte ValuedRatioPercent,
    ulong EventWatermark);

public sealed record LedgerStatus(
    long SchemaVersion,
    string SqliteVersion,
    ulong EventWatermark,
    ulong ProjectionWatermark,
    long DatabaseBytes,
    string CalculationVersion,
    bool DefaultNetworkEnabled);

public sealed record ExpenseChartRow(
    string BucketId,
    string Label,
    string Amount,
    ulong DistinctEventCount,
    uint WidthBasisPoints);

public sealed record ExpenseAnalysisView(JsonObject QueryResult, IReadOnlyList<ExpenseChartRow> ChartRows);

public sealed record ImportSummary(
    string Worksheet,
    int RowCount,
    string FileSha256,
    double ElapsedMs,
    bool FinancialValuesRemainedStrings);

public sealed record ExportSummary(string FileName, int RowCount, string FileSha256);

public sealed record FileAuthorization(string AuthorizationToken, string DisplayName);

public sealed record ManagedAttachment(
    string ManagedName,
    string RelativeLocation,
    long ByteCount,
    string Sha256);

public sealed record BackupSummary(
    string BackupId,
    int FormatVersion,
    long SchemaVersion,
    string DatabaseSha256,
    long PackageBytes,
    bool Verified);

public sealed record RestoreSummary(
    string BackupId,
    long SchemaVersion,
    bool IntegrityVerified,
    bool LiveLedgerReplaced);
