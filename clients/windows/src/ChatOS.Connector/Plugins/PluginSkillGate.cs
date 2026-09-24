using System.Globalization;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Plugins;

internal sealed class PluginSkillGate
{
    private readonly string[] _allOf;
    private readonly ArgumentSelector? _selector;

    private PluginSkillGate(string[] allOf, ArgumentSelector? selector)
    {
        _allOf = allOf;
        _selector = selector;
    }

    public IReadOnlyList<string> CatalogSkillNames => _allOf
        .Concat(_selector?.Map.Values ?? [])
        .Distinct(StringComparer.Ordinal)
        .Order(StringComparer.Ordinal)
        .ToArray();

    public static PluginSkillGate Parse(JsonElement value)
    {
        try
        {
            if (value.ValueKind != JsonValueKind.Object ||
                value.EnumerateObject().Any(property =>
                    property.Name is not ("allOf" or "selectByArgument")))
            {
                throw InvalidDeclaration();
            }

            var allOf = value.TryGetProperty("allOf", out var rawAllOf)
                ? StringArray(rawAllOf)
                : [];
            ArgumentSelector? selector = null;
            if (value.TryGetProperty("selectByArgument", out var rawSelector))
            {
                selector = ParseSelector(rawSelector);
            }
            if (allOf.Length == 0 && selector is null)
            {
                throw InvalidDeclaration();
            }

            var gate = new PluginSkillGate(allOf, selector);
            foreach (var name in gate.CatalogSkillNames)
            {
                if (!ValidSkillName(name))
                {
                    throw new PluginSkillGateException("invalid_skill_name",
                        $"Plugin Skill gate contains an invalid Skill name: {name}");
                }
            }
            return gate;
        }
        catch (PluginSkillGateException)
        {
            throw;
        }
        catch (Exception exception) when (exception is InvalidOperationException or FormatException)
        {
            throw InvalidDeclaration(exception);
        }
    }

    public IReadOnlyList<string> RequiredSkillNames(JsonElement arguments)
    {
        if (arguments.ValueKind != JsonValueKind.Object)
        {
            throw new PluginSkillGateException("invalid_arguments",
                "Plugin Skill gate requires tool arguments to be a JSON object.");
        }

        var required = _allOf.ToHashSet(StringComparer.Ordinal);
        if (_selector is not null)
        {
            if (!TryResolvePointer(arguments, _selector.Pointer, out var selected))
            {
                throw new PluginSkillGateException("missing_selector",
                    $"Plugin Skill gate selector argument is missing: {_selector.Pointer}");
            }
            if (selected.ValueKind != JsonValueKind.String)
            {
                throw new PluginSkillGateException("invalid_selector_value",
                    $"Plugin Skill gate selector value must be a string: {_selector.Pointer}");
            }
            var selectedValue = selected.GetString()!;
            if (!_selector.Map.TryGetValue(selectedValue, out var skillName))
            {
                throw new PluginSkillGateException("unmapped_selector_value",
                    $"Plugin Skill gate has no mapping for selector value: {selectedValue}");
            }
            required.Add(skillName);
        }
        return required.Order(StringComparer.Ordinal).ToArray();
    }

    private static ArgumentSelector ParseSelector(JsonElement value)
    {
        if (value.ValueKind != JsonValueKind.Object ||
            value.EnumerateObject().Any(property => property.Name is not ("pointer" or "map")) ||
            !value.TryGetProperty("pointer", out var pointerValue) ||
            pointerValue.ValueKind != JsonValueKind.String ||
            !value.TryGetProperty("map", out var mapValue) ||
            mapValue.ValueKind != JsonValueKind.Object)
        {
            throw InvalidDeclaration();
        }
        var pointer = pointerValue.GetString()!;
        if (!ValidJsonPointer(pointer))
        {
            throw InvalidDeclaration();
        }
        var map = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var property in mapValue.EnumerateObject())
        {
            if (string.IsNullOrWhiteSpace(property.Name) ||
                property.Value.ValueKind != JsonValueKind.String ||
                property.Value.GetString() is not { } skillName)
            {
                throw InvalidDeclaration();
            }
            map[property.Name] = skillName;
        }
        if (map.Count == 0)
        {
            throw InvalidDeclaration();
        }
        return new ArgumentSelector(pointer, map);
    }

    private static string[] StringArray(JsonElement value)
    {
        if (value.ValueKind != JsonValueKind.Array)
        {
            throw InvalidDeclaration();
        }
        return value.EnumerateArray().Select(item =>
            item.ValueKind == JsonValueKind.String && item.GetString() is { } text
                ? text
                : throw InvalidDeclaration()).ToArray();
    }

    private static bool ValidSkillName(string value)
    {
        if (string.IsNullOrEmpty(value) || Encoding.UTF8.GetByteCount(value) > 64 ||
            value.StartsWith('-') || value.EndsWith('-') || value.Contains("--", StringComparison.Ordinal))
        {
            return false;
        }
        return value.All(character => character is >= 'a' and <= 'z' or >= '0' and <= '9' or '-');
    }

    private static bool ValidJsonPointer(string value)
    {
        if (!value.StartsWith('/')) return false;
        for (var index = 0; index < value.Length; index++)
        {
            if (value[index] != '~') continue;
            index++;
            if (index >= value.Length || value[index] is not ('0' or '1')) return false;
        }
        return true;
    }

    private static bool TryResolvePointer(JsonElement root, string pointer, out JsonElement value)
    {
        value = root;
        foreach (var rawToken in pointer[1..].Split('/', StringSplitOptions.None))
        {
            var token = rawToken.Replace("~1", "/", StringComparison.Ordinal)
                .Replace("~0", "~", StringComparison.Ordinal);
            if (value.ValueKind == JsonValueKind.Object && value.TryGetProperty(token, out var next))
            {
                value = next;
                continue;
            }
            if (value.ValueKind == JsonValueKind.Array &&
                int.TryParse(token, NumberStyles.None, CultureInfo.InvariantCulture, out var index) &&
                index >= 0 && index < value.GetArrayLength())
            {
                value = value[index];
                continue;
            }
            value = default;
            return false;
        }
        return true;
    }

    private static PluginSkillGateException InvalidDeclaration(Exception? innerException = null) =>
        new("invalid_declaration", "Plugin Skill gate declaration is invalid.", innerException);

    private sealed record ArgumentSelector(
        string Pointer,
        IReadOnlyDictionary<string, string> Map);
}

internal sealed class PluginSkillGateException : Exception
{
    public PluginSkillGateException(string code, string message, Exception? innerException = null)
        : base(message, innerException)
    {
        Code = code;
    }

    public string Code { get; }
}
