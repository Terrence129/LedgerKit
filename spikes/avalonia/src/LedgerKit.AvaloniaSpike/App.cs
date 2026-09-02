using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Styling;
using Avalonia.Themes.Fluent;
using LedgerKit.AvaloniaSpike.Core;

namespace LedgerKit.AvaloniaSpike;

internal sealed class App : Application
{
    public override void Initialize()
    {
        RequestedThemeVariant = ThemeVariant.Light;
        Styles.Add(new FluentTheme());
    }

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            var root = Environment.GetEnvironmentVariable("LEDGERKIT_SPIKE_DATA_DIR");
            if (string.IsNullOrWhiteSpace(root))
            {
                root = Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                    "LedgerKit",
                    "AvaloniaSpike");
            }

            var facade = new ApplicationFacade(root);
            var window = new MainWindow(facade);
            if (Environment.GetEnvironmentVariable("LEDGERKIT_SPIKE_AUTOMATION_HIDDEN") == "1")
            {
                window.ShowInTaskbar = false;
                window.WindowStartupLocation = Avalonia.Controls.WindowStartupLocation.Manual;
                window.Position = new PixelPoint(-30_000, -30_000);
            }

            desktop.MainWindow = window;
            desktop.ShutdownRequested += (_, _) => facade.Dispose();
        }

        base.OnFrameworkInitializationCompleted();
    }
}
