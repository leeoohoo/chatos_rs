using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalAgentExternalMcpSkillBundleSnapshot(
    string Json,
    string Revision,
    JsonElement CatalogInputItem);

/// <summary>
/// Builds the same schema-v2 frozen Product Skill binding used by macOS for direct
/// External MCP tools. The Markdown is linked from the canonical bundled Skill so
/// Windows packaging cannot silently drift to a platform-specific prompt.
/// </summary>
internal static class WindowsLocalAgentExternalMcpSkillBundle
{
    public const string SkillName = "chatos-external-mcp";
    public const string SkillRef = "product-skill:" + SkillName;
    private const string ResourcePrefix = "ChatOS.AgentSkills.chatos-external-mcp.";

    public static WindowsLocalAgentExternalMcpSkillBundleSnapshot? Build(
        IReadOnlyList<WindowsLocalAgentExternalMcpTool> tools)
    {
        if (tools.Count == 0) return null;

        var instructions = ReadResource("SKILL.md").Trim();
        var description = FrontmatterValue(instructions, "description") ??
            throw new InvalidDataException("The bundled External MCP Skill has no description.");
        var resources = new[]
        {
            Resource("references/errors-and-recovery.md"),
            Resource("references/scenarios-and-verification.md"),
        };
        var document = new Dictionary<string, object?>
        {
            ["skill_ref"] = SkillRef,
            ["name"] = SkillName,
            ["role"] = "specialist",
            ["description"] = description,
            ["instructions"] = instructions,
            ["instructions_sha256"] = Digest(instructions, prefix: true),
            ["resources"] = resources,
        };
        var bindings = tools
            .GroupBy(tool => tool.ResourceId, StringComparer.Ordinal)
            .OrderBy(group => group.Key, StringComparer.Ordinal)
            .Select(group => Binding(group.ToArray()))
            .ToArray();
        if (bindings.Select(binding => (string)binding["binding_id"]!).Distinct(StringComparer.Ordinal).Count()
            != bindings.Length)
        {
            throw new InvalidDataException("The External MCP Skill binding identity is duplicated.");
        }

        var unversioned = Bundle("unversioned", document, bindings);
        var revision = Digest(CanonicalJson(unversioned), prefix: true);
        var bundle = Bundle(revision, document, bindings);
        var json = CanonicalJson(bundle);
        var catalog = JsonSerializer.SerializeToElement(new
        {
            type = "message",
            role = "system",
            content = new[]
            {
                new
                {
                    type = "input_text",
                    text = "[ChatOS Task Execution Product Skill Catalog]\n" +
                        $"- `{SkillRef}`: {description}\n\n" +
                        "External MCP tools are direct named tools, not Plugin broker options. " +
                        $"Activate `{SkillRef}` before calling one. Its guidance covers safe remote invocation, " +
                        "not unavailable server-owned business Skills. A changed remote catalog or endpoint " +
                        "requires a new Task; never retry an uncertain remote mutation automatically.",
                },
            },
        });
        return new(json, revision, catalog);
    }

    public static string ProviderId(string resourceId) => "chatos.external-mcp:" + resourceId;

    public static string BindingId(IReadOnlyList<WindowsLocalAgentExternalMcpTool> tools)
    {
        if (tools.Count == 0) throw new InvalidDataException("An External MCP binding has no tools.");
        var first = tools[0];
        if (tools.Any(tool => tool.ResourceId != first.ResourceId ||
                tool.ServerName != first.ServerName || tool.Endpoint != first.Endpoint))
        {
            throw new InvalidDataException("An External MCP binding mixes runtime identities.");
        }
        var catalog = new Dictionary<string, object?>
        {
            ["resource_id"] = first.ResourceId,
            ["server_name"] = first.ServerName,
            ["endpoint"] = first.Endpoint.AbsoluteUri,
            ["tools"] = tools.OrderBy(tool => tool.PublicName, StringComparer.Ordinal)
                .Select(tool => new Dictionary<string, object?>
                {
                    ["public_name"] = tool.PublicName,
                    ["upstream_name"] = tool.UpstreamName,
                    ["description"] = tool.Description,
                    ["input_schema"] = tool.InputSchema,
                    ["remote_skill_gate"] = tool.RemoteSkillGate,
                }).ToArray(),
        };
        var digest = Digest(CanonicalJson(JsonSerializer.SerializeToElement(catalog)), prefix: false);
        return $"external-mcp:{first.ResourceId}:{digest}";
    }

    private static Dictionary<string, object?> Binding(
        IReadOnlyList<WindowsLocalAgentExternalMcpTool> tools) => new()
    {
        ["provider_id"] = ProviderId(tools[0].ResourceId),
        ["binding_id"] = BindingId(tools),
        ["tool_names"] = tools.Select(tool => tool.PublicName)
            .Distinct(StringComparer.Ordinal).Order(StringComparer.Ordinal).ToArray(),
        ["router_skill_ref"] = SkillRef,
        ["specialist_skill_ref"] = SkillRef,
        ["activation_policy"] = "on_demand",
    };

    private static JsonElement Bundle(
        string revision,
        Dictionary<string, object?> document,
        Dictionary<string, object?>[] bindings) => JsonSerializer.SerializeToElement(
            new Dictionary<string, object?>
            {
                ["schema_version"] = 2,
                ["bundle_revision"] = revision,
                ["documents"] = new[] { document },
                ["tool_skill_bindings"] = bindings,
            });

    private static Dictionary<string, object?> Resource(string path)
    {
        var content = ReadResource(path).Trim();
        return new()
        {
            ["relative_path"] = path,
            ["content"] = content,
            ["sha256"] = Digest(content, prefix: true),
        };
    }

    private static string ReadResource(string path)
    {
        var name = ResourcePrefix + path.Replace('/', '.');
        using var stream = Assembly.GetExecutingAssembly().GetManifestResourceStream(name)
            ?? throw new InvalidDataException($"The bundled External MCP Skill resource is missing: {path}");
        using var reader = new StreamReader(stream, Encoding.UTF8, true);
        return reader.ReadToEnd();
    }

    private static string? FrontmatterValue(string markdown, string key)
    {
        if (!markdown.StartsWith("---\n", StringComparison.Ordinal)) return null;
        var end = markdown.IndexOf("\n---", 4, StringComparison.Ordinal);
        if (end < 0) return null;
        var prefix = key + ":";
        return markdown[4..end].Split('\n')
            .Select(line => line.Trim())
            .Where(line => line.StartsWith(prefix, StringComparison.Ordinal))
            .Select(line => line[prefix.Length..].Trim())
            .FirstOrDefault();
    }

    internal static string CanonicalJson(JsonElement value)
    {
        using var stream = new MemoryStream();
        using (var writer = new Utf8JsonWriter(stream)) WriteCanonical(writer, value);
        return Encoding.UTF8.GetString(stream.ToArray());
    }

    private static void WriteCanonical(Utf8JsonWriter writer, JsonElement value)
    {
        switch (value.ValueKind)
        {
            case JsonValueKind.Object:
                writer.WriteStartObject();
                foreach (var property in value.EnumerateObject().OrderBy(property => property.Name, StringComparer.Ordinal))
                {
                    writer.WritePropertyName(property.Name);
                    WriteCanonical(writer, property.Value);
                }
                writer.WriteEndObject();
                break;
            case JsonValueKind.Array:
                writer.WriteStartArray();
                foreach (var item in value.EnumerateArray()) WriteCanonical(writer, item);
                writer.WriteEndArray();
                break;
            default:
                value.WriteTo(writer);
                break;
        }
    }

    private static string Digest(string value, bool prefix)
    {
        var digest = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value)))
            .ToLowerInvariant();
        return prefix ? "sha256:" + digest : digest;
    }
}
