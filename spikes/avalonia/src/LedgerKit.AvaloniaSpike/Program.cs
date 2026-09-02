using System.Diagnostics;
using Avalonia;

namespace LedgerKit.AvaloniaSpike;

internal static class Program
{
    static Program()
    {
        Startup = Stopwatch.StartNew();
    }

    internal static Stopwatch Startup { get; }

    [STAThread]
    public static void Main(string[] args) => BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);

    public static AppBuilder BuildAvaloniaApp() => AppBuilder
        .Configure<App>()
        .UsePlatformDetect();
}
