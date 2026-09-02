using System.Diagnostics;
using System.Globalization;
using System.Text;
using System.Text.Json.Nodes;
using Avalonia;
using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using LedgerKit.AvaloniaSpike.Core;

namespace LedgerKit.AvaloniaSpike;

internal sealed class MainWindow : Window
{
    private const uint PageSize = 6;
    private static readonly IBrush Ink = new SolidColorBrush(Color.Parse("#172019"));
    private static readonly IBrush Muted = new SolidColorBrush(Color.Parse("#637168"));
    private static readonly IBrush Accent = new SolidColorBrush(Color.Parse("#315D3D"));
    private static readonly IBrush AccentSoft = new SolidColorBrush(Color.Parse("#DDE9DF"));
    private static readonly IBrush Surface = Brushes.White;
    private static readonly IBrush Canvas = new SolidColorBrush(Color.Parse("#EDF2EB"));

    private readonly ApplicationFacade facade;
    private readonly TextBlock message = Text("Loading local ledger…", 14, Brushes.White);
    private readonly TextBlock schemaValue = Text("—", 15, Ink, FontWeight.SemiBold);
    private readonly TextBlock sqliteValue = Text("—", 15, Ink, FontWeight.SemiBold);
    private readonly TextBlock watermarkValue = Text("—", 15, Ink, FontWeight.SemiBold);
    private readonly TextBlock networkValue = Text("—", 15, Ink, FontWeight.SemiBold);
    private readonly TextBlock netWorthValue = Text("—", 24, Ink, FontWeight.Bold);
    private readonly TextBlock expenseValue = Text("—", 24, Ink, FontWeight.Bold);
    private readonly TextBlock expenseHash = Text("—", 11, Muted);
    private readonly ProgressBar valuedBar = new() { Minimum = 0, Maximum = 100, Height = 18 };
    private readonly StackPanel bars = new() { Spacing = 9 };
    private readonly StackPanel activity = new() { Spacing = 0 };
    private readonly StackPanel expenseTable = new() { Spacing = 0 };
    private readonly TextBox amount = new() { Text = "12.34", MinWidth = 150 };
    private readonly TextBox password = new() { Text = "synthetic-password", PasswordChar = '●', MinWidth = 220 };
    private readonly Button previousButton = Button("Previous");
    private readonly Button nextButton = Button("Next");
    private readonly TextBlock pageLabel = Text("Page 1", 13, Muted);
    private uint currentPage = 1;
    private string? latestBackupId;
    private bool readyReported;

    public MainWindow(ApplicationFacade facade)
    {
        this.facade = facade;
        Title = "LedgerKit Avalonia M1 Spike";
        Width = 1180;
        Height = 800;
        MinWidth = 900;
        MinHeight = 650;
        Background = Canvas;
        Content = BuildContent();
        Loaded += async (_, _) => await RefreshAndReportReady();
        previousButton.Click += async (_, _) => await SelectPage(currentPage - 1);
        nextButton.Click += async (_, _) => await SelectPage(currentPage + 1);
    }

    private ScrollViewer BuildContent()
    {
        AutomationProperties.SetName(message, "Application status");
        var root = new StackPanel { Spacing = 14, Margin = new Thickness(24) };
        root.Children.Add(new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,Auto"),
            Children =
            {
                At(new StackPanel
                {
                    Children =
                    {
                        Eyebrow("LOCAL-FIRST FINANCIAL CORE"),
                        Text("LedgerKit", 44, Ink, FontWeight.Bold),
                        Text("Avalonia M1 vertical spike", 16, Muted),
                    },
                }, 0),
                At(StatusGrid(), 1),
            },
        });
        root.Children.Add(new Border
        {
            Background = Accent,
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(14, 10),
            Child = message,
        });

        var overview = Card(new StackPanel
        {
            Spacing = 10,
            Children =
            {
                SectionHeading("Net worth", "Authoritative Core projection", netWorthValue),
                valuedBar,
                Text("Cash and security values are decimal strings; the UI only renders the view model.", 13, Muted),
            },
        });
        root.Children.Add(overview);

        var expenseCard = Card(new StackPanel
        {
            Spacing = 12,
            Children =
            {
                SectionHeading("Expense analysis", "Top 10 + Other, native controls", expenseValue),
                bars,
                expenseHash,
            },
        });
        var activityCard = Card(new StackPanel
        {
            Spacing = 10,
            Children =
            {
                SectionHeading("Activity", "Bounded page query", null),
                activity,
                new Grid
                {
                    ColumnDefinitions = new ColumnDefinitions("Auto,*,Auto"),
                    Children =
                    {
                        At(previousButton, 0),
                        At(pageLabel, 1, HorizontalAlignment.Center),
                        At(nextButton, 2),
                    },
                },
            },
        });
        root.Children.Add(new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("1.1*,0.9*"),
            ColumnSpacing = 14,
            Children = { At(expenseCard, 0), At(activityCard, 1) },
        });

        root.Children.Add(Card(new StackPanel
        {
            Spacing = 10,
            Children =
            {
                SectionHeading("Expense table", "Same Core query as the bars", null),
                expenseTable,
            },
        }));
        root.Children.Add(BuildActions());
        return new ScrollViewer { Content = root };
    }

    private Border BuildActions()
    {
        var postButton = Button("Post expense");
        postButton.Click += async (_, _) => await Perform(
            "Posting synthetic expense",
            () =>
            {
                var response = facade.PostEvent(new PostEventRequest(
                    "Expense",
                    "2026-02-20",
                    "cash-cny-1",
                    amount.Text ?? string.Empty,
                    "CNY",
                    "cat-01",
                    Note: "Synthetic UI spike event"));
                return $"Posted {response.Event.EventId}";
            },
            true);
        var importButton = Button("Analyze 10k XLSX");
        importButton.Click += async (_, _) => await AnalyzeWorkbook();
        var exportButton = Button("Export XLSX");
        exportButton.Click += async (_, _) => await Perform(
            "Exporting standardized workbook",
            () =>
            {
                var result = facade.ExportData();
                return $"Exported {result.RowCount} rows to {result.FileName}";
            });
        var attachmentButton = Button("Copy attachment");
        attachmentButton.Click += async (_, _) => await CopyAttachment();
        var backupButton = Button("Create backup");
        backupButton.Click += async (_, _) => await Perform(
            "Creating encrypted backup",
            () =>
            {
                var result = facade.CreateBackup(password.Text ?? string.Empty);
                latestBackupId = result.BackupId;
                return $"Backup {result.BackupId} verified ({result.PackageBytes} bytes)";
            });
        var restoreButton = Button("Restore latest");
        restoreButton.Click += async (_, _) => await Perform(
            "Restoring encrypted backup",
            () =>
            {
                if (latestBackupId is null)
                {
                    throw new SpikeException("BACKUP_AUTHORIZATION_REJECTED", "Create a backup first.");
                }

                facade.RestoreBackup(latestBackupId, password.Text ?? string.Empty);
                return $"Backup {latestBackupId} restored and verified";
            },
            true);
        return Card(new StackPanel
        {
            Spacing = 12,
            Children =
            {
                SectionHeading("Vertical slice actions", "Named in-process Application Facade", null),
                new WrapPanel
                {
                    HorizontalAlignment = HorizontalAlignment.Left,
                    VerticalAlignment = VerticalAlignment.Center,
                    ItemSpacing = 10,
                    LineSpacing = 10,
                    Children =
                    {
                        Text("Synthetic amount", 13, Ink, FontWeight.SemiBold),
                        amount,
                        postButton,
                        importButton,
                        exportButton,
                        attachmentButton,
                    },
                },
                new WrapPanel
                {
                    HorizontalAlignment = HorizontalAlignment.Left,
                    VerticalAlignment = VerticalAlignment.Center,
                    ItemSpacing = 10,
                    LineSpacing = 10,
                    Children =
                    {
                        Text("Backup password", 13, Ink, FontWeight.SemiBold),
                        password,
                        backupButton,
                        restoreButton,
                    },
                },
            },
        });
    }

    private async Task RefreshAndReportReady()
    {
        var expenseStarted = Stopwatch.StartNew();
        await Refresh(currentPage);
        if (readyReported)
        {
            return;
        }

        readyReported = true;
        await Dispatcher.UIThread.InvokeAsync(() => { }, DispatcherPriority.Render);
        ReportReady(Program.Startup.Elapsed.TotalMilliseconds, expenseStarted.Elapsed.TotalMilliseconds);
        if (Environment.GetEnvironmentVariable("LEDGERKIT_SPIKE_AUTOCLOSE") == "1")
        {
            await Task.Delay(150);
            Close();
        }
    }

    private async Task Refresh(uint page)
    {
        var snapshot = await Task.Run(() => new ViewSnapshot(
            facade.GetLedgerStatus(),
            facade.GetOverview(),
            facade.GetActivity(page, PageSize),
            facade.GetExpenseAnalysis("2026-02-01", "2026-02-28")));
        schemaValue.Text = snapshot.Status.SchemaVersion.ToString(CultureInfo.InvariantCulture);
        sqliteValue.Text = snapshot.Status.SqliteVersion;
        watermarkValue.Text = snapshot.Status.EventWatermark.ToString(CultureInfo.InvariantCulture);
        networkValue.Text = snapshot.Status.DefaultNetworkEnabled ? "enabled" : "disabled";
        netWorthValue.Text = $"{snapshot.Overview.NetWorth} {snapshot.Overview.BaseCurrency}";
        valuedBar.Value = snapshot.Overview.ValuedRatioPercent;
        expenseValue.Text = $"{snapshot.Analysis.QueryResult["summary"]!["total_expense"]!.GetValue<string>()} CNY";
        expenseHash.Text = snapshot.Analysis.QueryResult["canonical_hash"]!.GetValue<string>();
        RenderBars(snapshot.Analysis.ChartRows);
        RenderExpenseTable(snapshot.Analysis.QueryResult["buckets"]!.AsArray());
        RenderActivity(snapshot.Activity);
        message.Text = $"Schema v{snapshot.Status.SchemaVersion} · SQLite {snapshot.Status.SqliteVersion} · local-only";
    }

    private void RenderBars(IReadOnlyList<ExpenseChartRow> rows)
    {
        bars.Children.Clear();
        foreach (var row in rows)
        {
            var bar = new Border
            {
                Height = 8,
                CornerRadius = new CornerRadius(4),
                Background = Accent,
                HorizontalAlignment = HorizontalAlignment.Left,
                Width = Math.Max(20, 460 * row.WidthBasisPoints / 10_000D),
            };
            AutomationProperties.SetName(bar, $"{row.Label}: {row.Amount} CNY");
            bars.Children.Add(new StackPanel
            {
                Spacing = 3,
                Children =
                {
                    new Grid
                    {
                        ColumnDefinitions = new ColumnDefinitions("*,Auto"),
                        Children =
                        {
                            At(Text(row.Label, 13, Ink, FontWeight.SemiBold), 0),
                            At(Text($"{row.Amount} · {row.DistinctEventCount}", 13, Muted), 1),
                        },
                    },
                    new Border
                    {
                        Height = 8,
                        CornerRadius = new CornerRadius(4),
                        Background = AccentSoft,
                        Child = bar,
                    },
                },
            });
        }
    }

    private void RenderActivity(ActivityPage page)
    {
        activity.Children.Clear();
        foreach (var ledgerEvent in page.Items)
        {
            activity.Children.Add(new Border
            {
                BorderBrush = AccentSoft,
                BorderThickness = new Thickness(0, 0, 0, 1),
                Padding = new Thickness(0, 9),
                Child = new Grid
                {
                    ColumnDefinitions = new ColumnDefinitions("*,Auto"),
                    Children =
                    {
                        At(new StackPanel
                        {
                            Children =
                            {
                                Text(ledgerEvent.CategoryLabel ?? ledgerEvent.EventType, 13, Ink, FontWeight.SemiBold),
                                Text(ledgerEvent.EffectiveDate, 12, Muted),
                            },
                        }, 0),
                        At(Text($"{ledgerEvent.SignedAmount} {ledgerEvent.Currency}", 13,
                            ledgerEvent.SignedAmount.StartsWith('-') ? Brushes.DarkRed : Accent,
                            FontWeight.SemiBold), 1),
                    },
                },
            });
        }

        previousButton.IsEnabled = page.Page > 1;
        nextButton.IsEnabled = page.HasMore;
        pageLabel.Text = $"Page {page.Page} of {Math.Max(1, Math.Ceiling(page.TotalCount / (double)page.PageSize)):0}";
    }

    private void RenderExpenseTable(JsonArray buckets)
    {
        expenseTable.Children.Clear();
        expenseTable.Children.Add(TableRow("Category", "Amount (CNY)", "Events", "Bucket ID", true));
        foreach (var item in buckets)
        {
            var row = item!.AsObject();
            expenseTable.Children.Add(TableRow(
                row["label"]!.GetValue<string>(),
                row["amount"]!.GetValue<string>(),
                row["distinct_event_count"]!.GetValue<ulong>().ToString(CultureInfo.InvariantCulture),
                row["bucket_id"]!.GetValue<string>(),
                false));
        }
    }

    private async Task AnalyzeWorkbook()
    {
        var selections = await StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Select the shared synthetic workbook",
            AllowMultiple = false,
            FileTypeFilter =
            [
                new FilePickerFileType("Excel workbook") { Patterns = ["*.xlsx"] },
            ],
        });
        var path = selections.Count == 0 ? null : selections[0].TryGetLocalPath();
        if (path is null)
        {
            message.Text = "Workbook selection cancelled";
            return;
        }

        var authorization = facade.AuthorizeSelectedWorkbook(path);
        await Perform(
            "Analyzing workbook on a worker thread",
            () =>
            {
                var result = facade.AnalyzeImport(authorization.AuthorizationToken);
                return $"Validated {result.RowCount} rows in {result.ElapsedMs:0.000} ms";
            });
    }

    private async Task CopyAttachment()
    {
        var selections = await StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Select a synthetic attachment",
            AllowMultiple = false,
        });
        var path = selections.Count == 0 ? null : selections[0].TryGetLocalPath();
        if (path is null)
        {
            message.Text = "Attachment selection cancelled";
            return;
        }

        var authorization = facade.AuthorizeSelectedAttachment(path);
        await Perform(
            "Copying authorized attachment",
            () =>
            {
                var result = facade.CopyAttachment(authorization.AuthorizationToken);
                return $"Copied {authorization.DisplayName} to {result.RelativeLocation}";
            });
    }

    private async Task SelectPage(uint page)
    {
        if (page == 0)
        {
            return;
        }

        currentPage = page;
        await Perform("Loading activity page", () => "Activity page loaded", true);
    }

    private async Task Perform(string workingMessage, Func<string> operation, bool refresh = false)
    {
        message.Text = workingMessage;
        try
        {
            var result = await Task.Run(operation);
            if (refresh)
            {
                await Refresh(currentPage);
            }

            message.Text = result;
        }
        catch (SpikeException exception)
        {
            message.Text = $"{exception.Code}: {exception.Message}";
        }
        catch (Exception exception)
        {
            message.Text = $"UNEXPECTED_FAILURE: {exception.GetType().Name}";
        }
    }

    private static void ReportReady(double firstRenderMs, double expenseRenderMs)
    {
        var path = Environment.GetEnvironmentVariable("LEDGERKIT_SPIKE_READY_FILE");
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        var json = FormattableString.Invariant(
            $"{{\"firstRenderMs\":{firstRenderMs:0.000},\"expenseRenderMs\":{expenseRenderMs:0.000},\"recordedAtUnixMs\":{DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}}}");
        File.WriteAllText(path, json, new UTF8Encoding(false));
    }

    private static Border Card(Control child) => new()
    {
        Background = Surface,
        BorderBrush = new SolidColorBrush(Color.Parse("#D1DBD2")),
        BorderThickness = new Thickness(1),
        CornerRadius = new CornerRadius(14),
        Padding = new Thickness(18),
        Child = child,
    };

    private static Grid SectionHeading(string title, string eyebrow, Control? right) => new()
    {
        ColumnDefinitions = new ColumnDefinitions("*,Auto"),
        Children =
        {
            At(new StackPanel
            {
                Children = { Eyebrow(eyebrow), Text(title, 20, Ink, FontWeight.Bold) },
            }, 0),
            At(right ?? new Border(), 1, HorizontalAlignment.Right),
        },
    };

    private Grid StatusGrid()
    {
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("Auto,Auto"),
            RowDefinitions = new RowDefinitions("Auto,Auto"),
            ColumnSpacing = 22,
            RowSpacing = 6,
        };
        grid.Children.Add(StatusCell("Schema", schemaValue, 0, 0));
        grid.Children.Add(StatusCell("SQLite", sqliteValue, 1, 0));
        grid.Children.Add(StatusCell("Watermark", watermarkValue, 0, 1));
        grid.Children.Add(StatusCell("Network", networkValue, 1, 1));
        return grid;
    }

    private static StackPanel StatusCell(string label, Control value, int column, int row)
    {
        var panel = new StackPanel { Children = { Text(label, 11, Muted), value } };
        Grid.SetColumn(panel, column);
        Grid.SetRow(panel, row);
        return panel;
    }

    private static Grid TableRow(string first, string second, string third, string fourth, bool header)
    {
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("1.2*,0.7*,0.5*,1.2*"),
            Background = header ? AccentSoft : Brushes.Transparent,
        };
        var values = new[] { first, second, third, fourth };
        for (var index = 0; index < values.Length; index++)
        {
            var cell = Text(values[index], header ? 12 : 13, header ? Ink : Muted, header ? FontWeight.Bold : FontWeight.Normal);
            cell.Margin = new Thickness(8, 7);
            Grid.SetColumn(cell, index);
            grid.Children.Add(cell);
        }

        AutomationProperties.SetName(grid, string.Join(", ", values));
        return grid;
    }

    private static TextBlock Eyebrow(string value) => Text(value, 11, new SolidColorBrush(Color.Parse("#497055")), FontWeight.Bold);

    private static TextBlock Text(string value, double size, IBrush brush, FontWeight weight = default) => new()
    {
        Text = value,
        FontSize = size,
        Foreground = brush,
        FontWeight = weight == default ? FontWeight.Normal : weight,
        TextWrapping = TextWrapping.Wrap,
        VerticalAlignment = VerticalAlignment.Center,
    };

    private static Button Button(string label)
    {
        var button = new Button
        {
            Content = label,
            MinHeight = 42,
            Padding = new Thickness(14, 8),
            HorizontalContentAlignment = HorizontalAlignment.Center,
        };
        AutomationProperties.SetName(button, label);
        return button;
    }

    private static T At<T>(T control, int column, HorizontalAlignment? alignment = null)
        where T : Control
    {
        Grid.SetColumn(control, column);
        if (alignment is not null)
        {
            control.HorizontalAlignment = alignment.Value;
        }

        return control;
    }

    private sealed record ViewSnapshot(
        LedgerStatus Status,
        Overview Overview,
        ActivityPage Activity,
        ExpenseAnalysisView Analysis);
}
