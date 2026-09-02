using System.Globalization;
using System.Text.RegularExpressions;

namespace LedgerKit.AvaloniaSpike.Core;

public static partial class DecimalContract
{
    public const int MaxAmountScale = 8;
    public const int MaxSignificantDigits = 28;

    [GeneratedRegex("^-?(?:0|[1-9][0-9]*)(?:\\.[0-9]+)?$", RegexOptions.CultureInvariant)]
    private static partial Regex DecimalShape();

    public static (string Text, decimal Value) ValidatePositiveAmount(
        string text,
        bool currencyPrecisionConfirmed)
    {
        ArgumentNullException.ThrowIfNull(text);
        if (!DecimalShape().IsMatch(text))
        {
            throw new SpikeException("DECIMAL_INVALID", "Decimal text is invalid.");
        }

        var unsigned = text.StartsWith('-') ? text[1..] : text;
        var decimalPoint = unsigned.IndexOf('.', StringComparison.Ordinal);
        var scale = decimalPoint < 0 ? 0 : unsigned.Length - decimalPoint - 1;
        if (scale > MaxAmountScale)
        {
            throw new SpikeException("DECIMAL_SCALE_EXCEEDED", "Decimal scale exceeds the contract.");
        }

        var digits = unsigned.Replace(".", string.Empty, StringComparison.Ordinal);
        var firstSignificant = digits.AsSpan().IndexOfAnyExcept('0');
        var significantDigits = firstSignificant < 0 ? 1 : digits.Length - firstSignificant;
        if (significantDigits > MaxSignificantDigits)
        {
            throw new SpikeException(
                "DECIMAL_PRECISION_EXCEEDED",
                "Decimal precision exceeds the contract.");
        }

        if (!decimal.TryParse(
                text,
                NumberStyles.AllowLeadingSign | NumberStyles.AllowDecimalPoint,
                CultureInfo.InvariantCulture,
                out var value))
        {
            throw new SpikeException("DECIMAL_OVERFLOW", "Decimal cannot be represented safely.");
        }

        if (value == decimal.Zero && text.StartsWith('-'))
        {
            throw new SpikeException("DECIMAL_INVALID", "Negative zero is not canonical.");
        }

        if (value <= decimal.Zero)
        {
            throw new SpikeException("AMOUNT_MUST_BE_POSITIVE", "Amount must be positive.");
        }

        if (scale > 2 && !currencyPrecisionConfirmed)
        {
            throw new SpikeException(
                "CURRENCY_PRECISION_CONFIRMATION_REQUIRED",
                "Currency precision requires explicit confirmation.");
        }

        return (text, value);
    }

    public static decimal ParseStored(string text)
    {
        if (!decimal.TryParse(
                text,
                NumberStyles.AllowLeadingSign | NumberStyles.AllowDecimalPoint,
                CultureInfo.InvariantCulture,
                out var value))
        {
            throw new SpikeException("DECIMAL_INVALID", "Stored decimal text is invalid.");
        }

        return value;
    }

    public static string Format(decimal value) => value.ToString(CultureInfo.InvariantCulture);

    public static decimal RoundHalfUp(decimal value, int decimals) =>
        decimal.Round(value, decimals, MidpointRounding.AwayFromZero);
}
