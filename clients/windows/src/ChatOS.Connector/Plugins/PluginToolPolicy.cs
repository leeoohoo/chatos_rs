using System.Text.Json;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Relay;

namespace ChatOS.Connector.Plugins;

internal sealed record PluginToolPolicy(
    string ApprovalMode,
    ConnectorApprovalRiskLevel RiskLevel,
    TimeSpan Timeout,
    IReadOnlySet<string> BasePermissions,
    IReadOnlyList<PluginToolPermissionRule> Rules)
{
    public static PluginToolPolicy Parse(JsonElement tool)
    {
        var metadata = tool.TryGetProperty("_meta", out var meta) &&
            meta.ValueKind == JsonValueKind.Object
                ? meta
                : default;
        var approval = ReadString(metadata, "chatos/approvalMode") == "per_call"
            ? "per_call"
            : "none";
        var risk = ReadString(metadata, "chatos/riskLevel") switch
        {
            "medium" => ConnectorApprovalRiskLevel.Medium,
            "high" or "critical" => ConnectorApprovalRiskLevel.High,
            _ => ConnectorApprovalRiskLevel.Low,
        };
        var declaredTimeout = ReadInt(metadata, "chatos/timeoutMs") ?? 7_200_000;
        var bounded = Math.Clamp(declaredTimeout, 300, 7_200_000);
        if (bounded < 7_200_000)
        {
            bounded = Math.Min(7_200_000,
                bounded + Math.Min(10_000, Math.Max(2_000, bounded / 2)));
        }

        var permissions = ReadStrings(metadata, "chatos/requiredPermissions")
            .ToHashSet(StringComparer.Ordinal);
        var rules = new List<PluginToolPermissionRule>();
        if (metadata.ValueKind == JsonValueKind.Object &&
            metadata.TryGetProperty("chatos/permissionRules", out var ruleValues) &&
            ruleValues.ValueKind == JsonValueKind.Array)
        {
            foreach (var rule in ruleValues.EnumerateArray())
            {
                if (rule.ValueKind != JsonValueKind.Object ||
                    ReadString(rule, "argumentPointer") is not { } pointer)
                {
                    continue;
                }
                rules.Add(new PluginToolPermissionRule(
                    pointer,
                    rule.TryGetProperty("equals", out var expected)
                        ? expected.Clone()
                        : JsonSerializer.SerializeToElement<object?>(null),
                    rule.TryGetProperty("matchWhenMissing", out var missing) &&
                        missing.ValueKind == JsonValueKind.True,
                    ReadStrings(rule, "requiredPermissions")
                        .ToHashSet(StringComparer.Ordinal)));
            }
        }
        return new PluginToolPolicy(approval, risk, TimeSpan.FromMilliseconds(bounded),
            permissions, rules);
    }

    public IReadOnlySet<string> RequiredPermissions(JsonElement arguments)
    {
        var result = BasePermissions.ToHashSet(StringComparer.Ordinal);
        foreach (var rule in Rules)
        {
            var value = JsonPointer(arguments, rule.Pointer);
            if ((value is null && rule.MatchWhenMissing) ||
                (value is not null &&
                 CanonicalJson.Serialize(value.Value) == CanonicalJson.Serialize(rule.Expected)))
            {
                result.UnionWith(rule.Permissions);
            }
        }
        return result;
    }

    private static string? ReadString(JsonElement value, string property) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(property, out var child) &&
        child.ValueKind == JsonValueKind.String
            ? child.GetString()
            : null;

    private static int? ReadInt(JsonElement value, string property) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(property, out var child) &&
        child.TryGetInt32(out var number)
            ? number
            : null;

    private static IReadOnlyList<string> ReadStrings(JsonElement value, string property) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(property, out var child) &&
        child.ValueKind == JsonValueKind.Array
            ? child.EnumerateArray()
                .Where(item => item.ValueKind == JsonValueKind.String)
                .Select(item => item.GetString())
                .Where(item => !string.IsNullOrWhiteSpace(item))
                .Select(item => item!.Trim())
                .ToArray()
            : [];

    private static JsonElement? JsonPointer(JsonElement root, string pointer)
    {
        if (!pointer.StartsWith("/", StringComparison.Ordinal)) return null;
        var current = root;
        foreach (var raw in pointer[1..].Split('/'))
        {
            var key = raw.Replace("~1", "/", StringComparison.Ordinal)
                .Replace("~0", "~", StringComparison.Ordinal);
            if (current.ValueKind != JsonValueKind.Object ||
                !current.TryGetProperty(key, out current))
            {
                return null;
            }
        }
        return current.Clone();
    }
}

internal sealed record PluginToolPermissionRule(
    string Pointer,
    JsonElement Expected,
    bool MatchWhenMissing,
    IReadOnlySet<string> Permissions);
