using System.Buffers;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace LedgerKit.AvaloniaSpike.Core;

public static class CanonicalJson
{
    public static string Hash(JsonNode value)
    {
        ArgumentNullException.ThrowIfNull(value);
        var target = value.DeepClone();
        if (target is JsonObject targetObject)
        {
            targetObject.Remove("canonical_hash");
        }

        return Sha256Prefixed(Bytes(target));
    }

    public static byte[] Bytes(JsonNode value)
    {
        ArgumentNullException.ThrowIfNull(value);
        var buffer = new ArrayBufferWriter<byte>();
        using (var writer = new Utf8JsonWriter(buffer, new JsonWriterOptions { Indented = false }))
        {
            WriteNode(writer, value);
        }

        return buffer.WrittenSpan.ToArray();
    }

    public static string Sha256Prefixed(ReadOnlySpan<byte> bytes) =>
        $"sha256:{Convert.ToHexStringLower(SHA256.HashData(bytes))}";

    public static string Sha256Hex(ReadOnlySpan<byte> bytes) =>
        Convert.ToHexStringLower(SHA256.HashData(bytes));

    private static void WriteNode(Utf8JsonWriter writer, JsonNode? node)
    {
        switch (node)
        {
            case null:
                writer.WriteNullValue();
                break;
            case JsonObject jsonObject:
                writer.WriteStartObject();
                foreach (var property in jsonObject.OrderBy(
                             static property => property.Key,
                             UnicodeScalarComparer.Instance))
                {
                    writer.WritePropertyName(property.Key.Normalize(NormalizationForm.FormC));
                    WriteNode(writer, property.Value);
                }

                writer.WriteEndObject();
                break;
            case JsonArray jsonArray:
                writer.WriteStartArray();
                foreach (var item in jsonArray)
                {
                    WriteNode(writer, item);
                }

                writer.WriteEndArray();
                break;
            case JsonValue jsonValue:
                WriteValue(writer, jsonValue);
                break;
            default:
                throw new SpikeException("SERIALIZATION_FAILED", "Unsupported canonical JSON value.");
        }
    }

    private static void WriteValue(Utf8JsonWriter writer, JsonValue value)
    {
        switch (value.GetValueKind())
        {
            case JsonValueKind.String:
                writer.WriteStringValue(value.GetValue<string>().Normalize(NormalizationForm.FormC));
                break;
            case JsonValueKind.Number:
                WriteNonNegativeInteger(writer, value);
                break;
            case JsonValueKind.True:
                writer.WriteBooleanValue(true);
                break;
            case JsonValueKind.False:
                writer.WriteBooleanValue(false);
                break;
            case JsonValueKind.Null:
                writer.WriteNullValue();
                break;
            default:
                throw new SpikeException(
                    "SERIALIZATION_FAILED",
                    "Canonical JSON numbers are limited to non-negative integers.");
        }
    }

    private static void WriteNonNegativeInteger(Utf8JsonWriter writer, JsonValue value)
    {
        if (value.TryGetValue<ulong>(out var unsignedLong))
        {
            writer.WriteNumberValue(unsignedLong);
            return;
        }

        if (value.TryGetValue<long>(out var signedLong) && signedLong >= 0)
        {
            writer.WriteNumberValue(signedLong);
            return;
        }

        if (value.TryGetValue<uint>(out var unsignedInteger))
        {
            writer.WriteNumberValue(unsignedInteger);
            return;
        }

        if (value.TryGetValue<int>(out var signedInteger) && signedInteger >= 0)
        {
            writer.WriteNumberValue(signedInteger);
            return;
        }

        throw new SpikeException(
            "SERIALIZATION_FAILED",
            "Canonical JSON numbers are limited to non-negative integers.");
    }

    private sealed class UnicodeScalarComparer : IComparer<string>
    {
        public static UnicodeScalarComparer Instance { get; } = new();

        public int Compare(string? x, string? y)
        {
            if (ReferenceEquals(x, y))
            {
                return 0;
            }

            if (x is null)
            {
                return -1;
            }

            if (y is null)
            {
                return 1;
            }

            var left = x.Normalize(NormalizationForm.FormC).EnumerateRunes().GetEnumerator();
            var right = y.Normalize(NormalizationForm.FormC).EnumerateRunes().GetEnumerator();
            while (left.MoveNext())
            {
                if (!right.MoveNext())
                {
                    return 1;
                }

                var comparison = left.Current.Value.CompareTo(right.Current.Value);
                if (comparison != 0)
                {
                    return comparison;
                }
            }

            return right.MoveNext() ? -1 : 0;
        }
    }
}
