namespace LedgerKit.AvaloniaSpike.Core;

public sealed class ApplicationFacade : IDisposable
{
    private readonly string root;
    private readonly LedgerStore ledger;
    private readonly FileAuthorizationService files;
    private readonly BackupService backups;
    private readonly object backupSync = new();
    private readonly Dictionary<string, string> authorizedBackups = new(StringComparer.Ordinal);

    public ApplicationFacade(string applicationDataRoot)
    {
        root = Path.GetFullPath(applicationDataRoot);
        Directory.CreateDirectory(root);
        ledger = LedgerStore.Open(Path.Combine(root, "ledgerkit-spike.sqlite"));
        ledger.InitializeDemo();
        files = new FileAuthorizationService(root);
        backups = new BackupService(ledger, Path.Combine(root, "restore-work"));
    }

    public LedgerStatus GetLedgerStatus() => ledger.GetStatus();

    public PostEventResponse PostEvent(PostEventRequest request) => ledger.PostEvent(request);

    public ActivityPage GetActivity(uint page, uint pageSize) => ledger.GetActivity(page, pageSize);

    public Overview GetOverview() => ledger.GetOverview();

    public ExpenseAnalysisView GetExpenseAnalysis(string startDate, string endDate) =>
        ledger.GetExpenseAnalysis(startDate, endDate);

    public FileAuthorization AuthorizeSelectedWorkbook(string selectedPath) =>
        files.AuthorizeSelectedFile(selectedPath, "workbook");

    public ImportSummary AnalyzeImport(string authorizationToken)
    {
        var path = files.Consume(authorizationToken, "workbook");
        return ExcelAdapter.AnalyzeKnownTemplate(path);
    }

    public ExportSummary ExportData()
    {
        var exportRoot = Path.Combine(root, "exports");
        Directory.CreateDirectory(exportRoot);
        var fileName = $"ledgerkit-standardized-{DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}.xlsx";
        return ExcelAdapter.ExportStandardized(ledger.GetAllActivity(), Path.Combine(exportRoot, fileName));
    }

    public FileAuthorization AuthorizeSelectedAttachment(string selectedPath) =>
        files.AuthorizeSelectedFile(selectedPath, "attachment");

    public ManagedAttachment CopyAttachment(string authorizationToken) =>
        files.CopyAuthorizedAttachment(authorizationToken);

    public BackupSummary CreateBackup(string password)
    {
        ArgumentNullException.ThrowIfNull(password);
        var backupId = $"backup-{DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}";
        var backupRoot = Path.Combine(root, "backups");
        Directory.CreateDirectory(backupRoot);
        var path = Path.Combine(backupRoot, $"{backupId}.ledgerkit-backup");
        var summary = backups.Create(backupId, path, password);
        lock (backupSync)
        {
            authorizedBackups.Add(backupId, path);
        }

        return summary;
    }

    public RestoreSummary RestoreBackup(string backupId, string password)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(backupId);
        ArgumentNullException.ThrowIfNull(password);
        string path;
        lock (backupSync)
        {
            if (!authorizedBackups.TryGetValue(backupId, out path!))
            {
                throw new SpikeException(
                    "BACKUP_AUTHORIZATION_REJECTED",
                    "Backup identifier is not authorized in this application session.");
            }
        }

        return backups.Restore(backupId, path, password);
    }

    public void Dispose() => ledger.Dispose();
}
