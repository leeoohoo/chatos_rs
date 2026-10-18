using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace ChatOS.Connector.LocalAgent;

/// <summary>
/// Bounded, offline JSON Schema preflight for Plugin and external MCP tools.
/// Unsupported assertions fail closed; references are restricted to the frozen schema.
/// </summary>
internal static class WindowsExternalToolSchemaValidator
{
    private const int MaximumArgumentBytes = 16 * 1024 * 1024;
    private const int MaximumDepth = 32;
    private static readonly TimeSpan PatternTimeout = TimeSpan.FromMilliseconds(250);
    private static readonly HashSet<string> SupportedKeywords = new(StringComparer.Ordinal)
    {
        "$schema", "$id", "$comment", "$defs", "definitions", "$ref", "title",
        "description", "default", "examples", "deprecated", "readOnly", "writeOnly",
        "format", "x-enum-labels", "x-chatos-external-mcp-id", "type", "properties",
        "required", "additionalProperties", "minProperties", "maxProperties", "items",
        "prefixItems", "minItems", "maxItems", "uniqueItems", "minimum", "maximum",
        "exclusiveMinimum", "exclusiveMaximum", "multipleOf", "minLength", "maxLength",
        "pattern", "enum", "const", "anyOf", "oneOf", "allOf", "not", "if", "then",
        "else",
    };

    public static void Validate(JsonElement arguments, JsonElement schema)
    {
        if (Encoding.UTF8.GetByteCount(arguments.GetRawText()) > MaximumArgumentBytes)
        {
            throw WindowsExternalToolSchemaException.Mismatch(
                "$", "argument envelope is too large");
        }

        Check(arguments, schema, schema, "$", 0);
    }

    private static void Check(
        JsonElement value,
        JsonElement schema,
        JsonElement root,
        string path,
        int depth)
    {
        if (depth >= MaximumDepth)
        {
            throw WindowsExternalToolSchemaException.Unsupported(
                "schema nesting/reference limit");
        }

        if (schema.ValueKind is JsonValueKind.True or JsonValueKind.False)
        {
            if (schema.ValueKind == JsonValueKind.False)
            {
                throw WindowsExternalToolSchemaException.Mismatch(
                    path, "schema rejects this value");
            }
            return;
        }
        if (schema.ValueKind != JsonValueKind.Object)
        {
            throw WindowsExternalToolSchemaException.Unsupported(
                "schema must be an object or boolean");
        }

        var unknown = schema.EnumerateObject()
            .Select(property => property.Name)
            .Where(name => !SupportedKeywords.Contains(name))
            .Order(StringComparer.Ordinal)
            .FirstOrDefault();
        if (unknown is not null)
        {
            throw WindowsExternalToolSchemaException.Unsupported(unknown);
        }

        if (schema.TryGetProperty("$ref", out var reference) &&
            reference.ValueKind == JsonValueKind.String)
        {
            Check(value, Resolve(reference.GetString()!, root), root, path, depth + 1);
        }

        foreach (var keyword in new[] { "allOf", "anyOf", "oneOf" })
        {
            if (!schema.TryGetProperty(keyword, out var branches) ||
                branches.ValueKind != JsonValueKind.Array)
            {
                continue;
            }

            var total = 0;
            var matches = 0;
            var reasons = new List<string>();
            foreach (var branch in branches.EnumerateArray())
            {
                total++;
                try
                {
                    Check(value, branch, root, path, depth + 1);
                    matches++;
                }
                catch (WindowsExternalToolSchemaException exception)
                    when (!exception.IsUnsupported)
                {
                    if (reasons.Count < 3) reasons.Add(exception.Message);
                }
            }

            var valid = keyword switch
            {
                "allOf" => matches == total,
                "oneOf" => matches == 1,
                _ => matches > 0,
            };
            if (!valid)
            {
                var detail = reasons.Count == 0 ? string.Empty : $"; {string.Join("; ", reasons)}";
                throw WindowsExternalToolSchemaException.Mismatch(
                    path, $"does not match {keyword}{detail}");
            }
        }

        if (schema.TryGetProperty("not", out var negated) &&
            Matches(value, negated, root, path, depth + 1))
        {
            throw WindowsExternalToolSchemaException.Mismatch(
                path, "matches forbidden schema");
        }
        if (schema.TryGetProperty("if", out var condition))
        {
            var branchName = Matches(value, condition, root, path, depth + 1)
                ? "then"
                : "else";
            if (schema.TryGetProperty(branchName, out var conditionalBranch))
            {
                Check(value, conditionalBranch, root, path, depth + 1);
            }
        }

        if (schema.TryGetProperty("enum", out var choices) &&
            choices.ValueKind == JsonValueKind.Array &&
            !choices.EnumerateArray().Any(choice => Equal(value, choice)))
        {
            throw WindowsExternalToolSchemaException.Mismatch(
                path, "must be one of the declared enum values");
        }
        if (schema.TryGetProperty("const", out var constant) && !Equal(value, constant))
        {
            throw WindowsExternalToolSchemaException.Mismatch(path, "must match const");
        }

        var types = Types(schema);
        if (types is not null && !types.Any(type => HasType(value, type)))
        {
            throw WindowsExternalToolSchemaException.Mismatch(
                path, $"expected {string.Join(" or ", types)}");
        }

        if (value.ValueKind == JsonValueKind.Object)
        {
            var properties = value.EnumerateObject().ToArray();
            Bounds(properties.Length, schema, "minProperties", "maxProperties", path);
            if (schema.TryGetProperty("required", out var required) &&
                required.ValueKind == JsonValueKind.Array)
            {
                foreach (var entry in required.EnumerateArray())
                {
                    if (entry.ValueKind == JsonValueKind.String &&
                        !value.TryGetProperty(entry.GetString()!, out _))
                    {
                        throw WindowsExternalToolSchemaException.Mismatch(
                            $"{path}.{entry.GetString()}", "required field is missing");
                    }
                }
            }

            var hasDeclaredProperties = schema.TryGetProperty(
                "properties", out var declaredProperties) &&
                declaredProperties.ValueKind == JsonValueKind.Object;
            var hasAdditional = schema.TryGetProperty(
                "additionalProperties", out var additionalProperties);
            foreach (var property in properties.OrderBy(property => property.Name, StringComparer.Ordinal))
            {
                if (hasDeclaredProperties &&
                    declaredProperties.TryGetProperty(property.Name, out var propertySchema))
                {
                    Check(property.Value, propertySchema, root,
                        $"{path}.{property.Name}", depth + 1);
                }
                else if (hasAdditional)
                {
                    Check(property.Value, additionalProperties, root,
                        $"{path}.{property.Name}", depth + 1);
                }
            }
        }

        if (value.ValueKind == JsonValueKind.Array)
        {
            var items = value.EnumerateArray().ToArray();
            Bounds(items.Length, schema, "minItems", "maxItems", path);
            if (schema.TryGetProperty("uniqueItems", out var unique) &&
                unique.ValueKind == JsonValueKind.True)
            {
                for (var index = 0; index < items.Length; index++)
                {
                    for (var comparison = 0; comparison < index; comparison++)
                    {
                        if (Equal(items[index], items[comparison]))
                        {
                            throw WindowsExternalToolSchemaException.Mismatch(
                                path, "items must be unique");
                        }
                    }
                }
            }

            JsonElement[] prefix = [];
            var tupleItems = false;
            if (schema.TryGetProperty("prefixItems", out var prefixItems) &&
                prefixItems.ValueKind == JsonValueKind.Array)
            {
                prefix = prefixItems.EnumerateArray().ToArray();
            }
            else if (schema.TryGetProperty("items", out var legacyItems) &&
                legacyItems.ValueKind == JsonValueKind.Array)
            {
                prefix = legacyItems.EnumerateArray().ToArray();
                tupleItems = true;
            }
            for (var index = 0; index < items.Length; index++)
            {
                if (index < prefix.Length)
                {
                    Check(items[index], prefix[index], root, $"{path}[{index}]", depth + 1);
                }
                else if (!tupleItems && schema.TryGetProperty("items", out var itemSchema) &&
                    itemSchema.ValueKind != JsonValueKind.Array)
                {
                    Check(items[index], itemSchema, root, $"{path}[{index}]", depth + 1);
                }
            }
        }

        if (value.ValueKind == JsonValueKind.String)
        {
            var text = value.GetString()!;
            Bounds(text.EnumerateRunes().Count(), schema, "minLength", "maxLength", path);
            if (schema.TryGetProperty("pattern", out var pattern) &&
                pattern.ValueKind == JsonValueKind.String)
            {
                try
                {
                    if (!Regex.IsMatch(text, pattern.GetString()!, RegexOptions.CultureInvariant,
                        PatternTimeout))
                    {
                        throw WindowsExternalToolSchemaException.Mismatch(
                            path, "must match the declared pattern");
                    }
                }
                catch (ArgumentException)
                {
                    throw WindowsExternalToolSchemaException.Unsupported("pattern");
                }
                catch (RegexMatchTimeoutException)
                {
                    throw WindowsExternalToolSchemaException.Unsupported("pattern timeout");
                }
            }
        }

        if (value.ValueKind == JsonValueKind.Number && value.TryGetDouble(out var actual) &&
            double.IsFinite(actual))
        {
            Bounds(actual, schema, "minimum", "maximum", path);
            if (Number(schema, "exclusiveMinimum") is { } exclusiveMinimum &&
                actual <= exclusiveMinimum)
            {
                throw WindowsExternalToolSchemaException.Mismatch(
                    path, $"must exceed {exclusiveMinimum}");
            }
            if (Number(schema, "exclusiveMaximum") is { } exclusiveMaximum &&
                actual >= exclusiveMaximum)
            {
                throw WindowsExternalToolSchemaException.Mismatch(
                    path, $"must be below {exclusiveMaximum}");
            }
            if (Number(schema, "multipleOf") is { } multiple)
            {
                if (multiple <= 0)
                {
                    throw WindowsExternalToolSchemaException.Unsupported("multipleOf");
                }
                var quotient = actual / multiple;
                if (Math.Abs(quotient - Math.Round(quotient)) > 1e-9)
                {
                    throw WindowsExternalToolSchemaException.Mismatch(
                        path, $"must be a multiple of {multiple}");
                }
            }
        }
    }

    private static bool Matches(
        JsonElement value,
        JsonElement schema,
        JsonElement root,
        string path,
        int depth)
    {
        try
        {
            Check(value, schema, root, path, depth);
            return true;
        }
        catch (WindowsExternalToolSchemaException exception) when (!exception.IsUnsupported)
        {
            return false;
        }
    }

    private static JsonElement Resolve(string reference, JsonElement root)
    {
        if (reference == "#") return root;
        if (!reference.StartsWith("#/", StringComparison.Ordinal))
        {
            throw WindowsExternalToolSchemaException.Unsupported(
                "only local JSON-pointer $ref is supported");
        }

        var current = root;
        foreach (var encoded in reference[2..].Split('/', StringSplitOptions.None))
        {
            var segment = encoded.Replace("~1", "/", StringComparison.Ordinal)
                .Replace("~0", "~", StringComparison.Ordinal);
            if (current.ValueKind != JsonValueKind.Object ||
                !current.TryGetProperty(segment, out current))
            {
                throw WindowsExternalToolSchemaException.Unsupported("unresolved $ref");
            }
        }
        return current;
    }

    private static IReadOnlyList<string>? Types(JsonElement schema)
    {
        if (!schema.TryGetProperty("type", out var type)) return null;
        if (type.ValueKind == JsonValueKind.String) return [type.GetString()!];
        if (type.ValueKind != JsonValueKind.Array) return null;
        var values = type.EnumerateArray()
            .Where(value => value.ValueKind == JsonValueKind.String)
            .Select(value => value.GetString()!)
            .ToArray();
        return values.Length == type.GetArrayLength() ? values : null;
    }

    private static bool HasType(JsonElement value, string type) => type switch
    {
        "null" => value.ValueKind == JsonValueKind.Null,
        "object" => value.ValueKind == JsonValueKind.Object,
        "array" => value.ValueKind == JsonValueKind.Array,
        "string" => value.ValueKind == JsonValueKind.String,
        "boolean" => value.ValueKind is JsonValueKind.True or JsonValueKind.False,
        "number" => IsFiniteNumber(value),
        "integer" => IsInteger(value),
        _ => false,
    };

    private static bool IsFiniteNumber(JsonElement value) =>
        value.ValueKind == JsonValueKind.Number &&
        value.TryGetDouble(out var number) &&
        double.IsFinite(number);

    private static bool IsInteger(JsonElement value) =>
        IsFiniteNumber(value) &&
        (value.TryGetDecimal(out var decimalNumber)
            ? decimal.Truncate(decimalNumber) == decimalNumber
            : value.TryGetDouble(out var number) && Math.Round(number) == number);

    private static bool Equal(JsonElement left, JsonElement right)
    {
        if (left.ValueKind == JsonValueKind.Number && right.ValueKind == JsonValueKind.Number)
        {
            if (left.TryGetDecimal(out var leftDecimal) &&
                right.TryGetDecimal(out var rightDecimal))
            {
                return leftDecimal == rightDecimal;
            }
            return left.TryGetDouble(out var leftNumber) &&
                right.TryGetDouble(out var rightNumber) &&
                leftNumber.Equals(rightNumber);
        }
        if (left.ValueKind != right.ValueKind) return false;
        return left.ValueKind switch
        {
            JsonValueKind.Null => true,
            JsonValueKind.True or JsonValueKind.False => left.GetBoolean() == right.GetBoolean(),
            JsonValueKind.String => left.GetString() == right.GetString(),
            JsonValueKind.Array => EqualArrays(left, right),
            JsonValueKind.Object => EqualObjects(left, right),
            _ => false,
        };
    }

    private static bool EqualArrays(JsonElement left, JsonElement right)
    {
        var leftItems = left.EnumerateArray().ToArray();
        var rightItems = right.EnumerateArray().ToArray();
        return leftItems.Length == rightItems.Length &&
            leftItems.Zip(rightItems).All(pair => Equal(pair.First, pair.Second));
    }

    private static bool EqualObjects(JsonElement left, JsonElement right)
    {
        var leftProperties = left.EnumerateObject()
            .ToDictionary(property => property.Name, property => property.Value, StringComparer.Ordinal);
        var rightProperties = right.EnumerateObject()
            .ToDictionary(property => property.Name, property => property.Value, StringComparer.Ordinal);
        return leftProperties.Count == rightProperties.Count &&
            leftProperties.All(pair => rightProperties.TryGetValue(pair.Key, out var value) &&
                Equal(pair.Value, value));
    }

    private static void Bounds(
        double actual,
        JsonElement schema,
        string minimumName,
        string maximumName,
        string path)
    {
        if (Number(schema, minimumName) is { } minimum && actual < minimum)
        {
            throw WindowsExternalToolSchemaException.Mismatch(
                path, $"minimum is {minimum}");
        }
        if (Number(schema, maximumName) is { } maximum && actual > maximum)
        {
            throw WindowsExternalToolSchemaException.Mismatch(
                path, $"maximum is {maximum}");
        }
    }

    private static double? Number(JsonElement schema, string property)
    {
        return schema.TryGetProperty(property, out var value) &&
            value.ValueKind == JsonValueKind.Number &&
            value.TryGetDouble(out var number)
                ? number
                : null;
    }
}

internal sealed class WindowsExternalToolSchemaException : InvalidOperationException
{
    private WindowsExternalToolSchemaException(string message, bool isUnsupported)
        : base(message)
    {
        IsUnsupported = isUnsupported;
    }

    public bool IsUnsupported { get; }

    public static WindowsExternalToolSchemaException Mismatch(string path, string reason) =>
        new($"Tool arguments do not match the schema at {path}: {reason}.", false);

    public static WindowsExternalToolSchemaException Unsupported(string reason) =>
        new($"Tool schema is not supported: {reason}; the call was not executed.", true);
}
