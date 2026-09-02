namespace LedgerKit.AvaloniaSpike.Core;

public sealed class SpikeException : Exception
{
    public SpikeException(string code, string message)
        : base(message)
    {
        Code = code;
    }

    public SpikeException(string code, string message, Exception innerException)
        : base(message, innerException)
    {
        Code = code;
    }

    public string Code { get; }

    internal static SpikeException Wrap(string code, string message, Exception exception) =>
        exception as SpikeException ?? new SpikeException(code, message, exception);
}
