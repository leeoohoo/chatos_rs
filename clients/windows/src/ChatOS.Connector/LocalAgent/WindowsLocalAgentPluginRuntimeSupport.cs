using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Relay;

namespace ChatOS.Connector.LocalAgent;

internal static class WindowsLocalAgentPluginRuntimeSupport
{
    public static IReadOnlyList<JsonElement> ValidTools(IReadOnlyList<JsonElement> tools)
    {
        var output = new List<JsonElement>();
        var names = new HashSet<string>(StringComparer.Ordinal);
        foreach (var tool in tools)
        {
            if (tool.ValueKind != JsonValueKind.Object ||
                !tool.TryGetProperty("name", out var name) ||
                name.ValueKind != JsonValueKind.String ||
                string.IsNullOrWhiteSpace(name.GetString()) ||
                !names.Add(name.GetString()!) ||
                !tool.TryGetProperty("inputSchema", out var schema) ||
                schema.ValueKind != JsonValueKind.Object)
            {
                continue;
            }
            output.Add(tool.Clone());
        }
        return output;
    }

    public static bool HasSkillGate(JsonElement definition) =>
        definition.TryGetProperty("_meta", out var metadata) &&
        metadata.ValueKind == JsonValueKind.Object &&
        metadata.TryGetProperty("chatos/skillGate", out _);

    public static string Description(JsonElement definition) =>
        definition.TryGetProperty("description", out var value) &&
        value.ValueKind == JsonValueKind.String
            ? Limit(value.GetString() ?? string.Empty, 1_000)
            : string.Empty;

    public static string SafeArgumentSummary(string toolName, JsonElement arguments)
    {
        var keys = string.Join(", ", arguments.EnumerateObject()
            .Select(value => value.Name).Order(StringComparer.Ordinal));
        var digest = Convert.ToHexString(SHA256.HashData(
            Encoding.UTF8.GetBytes(CanonicalJson.Serialize(arguments))))
            .ToLowerInvariant()[..12];
        return $"{toolName}; fields: {keys}; digest: {digest}";
    }

    public static string Limit(string value, int maximum) =>
        value.Length <= maximum ? value : value[..maximum];
}
