using System.Globalization;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;

namespace ChatOS.NetworkGuard.Contracts;

internal static class CanonicalJson
{
    private static readonly JsonSerializerOptions StringOptions = new()
    {
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };

    public static string Serialize(JsonElement value)
    {
        var output = new StringBuilder();
        Write(value, output);
        return output.ToString();
    }

    private static void Write(JsonElement value, StringBuilder output)
    {
        switch (value.ValueKind)
        {
            case JsonValueKind.Null:
            case JsonValueKind.Undefined:
                output.Append("null");
                break;
            case JsonValueKind.True:
                output.Append("true");
                break;
            case JsonValueKind.False:
                output.Append("false");
                break;
            case JsonValueKind.Number:
                WriteNumber(value, output);
                break;
            case JsonValueKind.String:
                output.Append(JsonSerializer.Serialize(value.GetString(), StringOptions));
                break;
            case JsonValueKind.Array:
                output.Append('[');
                var firstItem = true;
                foreach (var item in value.EnumerateArray())
                {
                    if (!firstItem) output.Append(',');
                    firstItem = false;
                    Write(item, output);
                }
                output.Append(']');
                break;
            case JsonValueKind.Object:
                output.Append('{');
                var firstProperty = true;
                foreach (var property in value.EnumerateObject()
                    .OrderBy(property => property.Name, StringComparer.Ordinal))
                {
                    if (!firstProperty) output.Append(',');
                    firstProperty = false;
                    output.Append(JsonSerializer.Serialize(property.Name, StringOptions));
                    output.Append(':');
                    Write(property.Value, output);
                }
                output.Append('}');
                break;
            default:
                throw new InvalidDataException("Canonical JSON contains an unsupported value.");
        }
    }

    private static void WriteNumber(JsonElement value, StringBuilder output)
    {
        if (value.TryGetInt64(out var integer))
        {
            output.Append(integer.ToString(CultureInfo.InvariantCulture));
            return;
        }
        if (value.TryGetDecimal(out var decimalValue))
        {
            output.Append(decimalValue.ToString("G29", CultureInfo.InvariantCulture));
            return;
        }
        var number = value.GetDouble();
        if (!double.IsFinite(number))
        {
            throw new InvalidDataException("Canonical JSON number is not finite.");
        }
        output.Append(number.ToString("R", CultureInfo.InvariantCulture).Replace('E', 'e'));
    }
}
