using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsAgentCapabilityDescriptor(
    string PluginOption,
    string Name,
    string PluginKey,
    string ComponentKey,
    IReadOnlyList<WindowsAgentCapabilityToolDescriptor> Tools);

internal sealed record WindowsAgentCapabilityToolDescriptor(
    string ToolOption,
    string Name,
    string Description,
    JsonElement InputSchema,
    string Approval,
    IReadOnlyList<string> RequiredSkillRefs);

/// <summary>
/// Produces disclosure-v2 responses at complete JSON boundaries so a generic text
/// budget can never silently remove part of an external tool schema.
/// </summary>
internal static class WindowsAgentCapabilityDisclosure
{
    public const int MaximumResponseCharacters = 8_000;
    private const int DefaultLimit = 8;
    private const int MaximumLimit = 20;
    private const int MaximumSchemaPageCharacters = 3_000;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public static JsonElement Response(
        WindowsAgentCapabilityDescriptor descriptor,
        string? toolOption = null,
        string? query = null,
        int offset = 0,
        int limit = DefaultLimit,
        int? schemaOffset = null)
    {
        if (offset < 0 || limit is < 1 or > MaximumLimit || schemaOffset < 0)
        {
            throw new WindowsAgentCapabilityDisclosureException();
        }

        return toolOption is null
            ? Catalog(descriptor, query, offset, limit, schemaOffset)
            : Tool(descriptor, toolOption, query, offset, schemaOffset);
    }

    private static JsonElement Catalog(
        WindowsAgentCapabilityDescriptor descriptor,
        string? query,
        int offset,
        int limit,
        int? schemaOffset)
    {
        if (schemaOffset is not null)
        {
            throw new WindowsAgentCapabilityDisclosureException();
        }

        var terms = (query ?? string.Empty)
            .Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries |
                StringSplitOptions.TrimEntries);
        var filtered = descriptor.Tools.Where(tool =>
        {
            if (terms.Length == 0) return true;
            var searchable = $"{tool.Name} {tool.Description}";
            return terms.Any(term => searchable.Contains(
                term, StringComparison.OrdinalIgnoreCase));
        }).ToArray();
        if (offset > filtered.Length)
        {
            throw new WindowsAgentCapabilityDisclosureException();
        }

        var page = new List<Dictionary<string, object?>>();
        foreach (var tool in filtered.Skip(offset).Take(limit))
        {
            var candidate = new List<Dictionary<string, object?>>(page) { Brief(tool) };
            var candidateRoot = CatalogRoot(
                descriptor, candidate, filtered.Length, offset, offset + candidate.Count);
            if (!Fits(candidateRoot)) break;
            page.Add(Brief(tool));
        }

        var next = offset + page.Count;
        var root = CatalogRoot(
            descriptor,
            page,
            filtered.Length,
            offset,
            next < filtered.Length ? next : null);
        if (!Fits(root) || (next < filtered.Length && page.Count == 0))
        {
            return Overflow(descriptor, null);
        }
        return Element(root);
    }

    private static JsonElement Tool(
        WindowsAgentCapabilityDescriptor descriptor,
        string toolOption,
        string? query,
        int offset,
        int? schemaOffset)
    {
        if (query is not null || offset != 0)
        {
            throw new WindowsAgentCapabilityDisclosureException();
        }
        var tool = descriptor.Tools.FirstOrDefault(value =>
            value.ToolOption == toolOption)
            ?? throw new WindowsAgentCapabilityDisclosureException();
        var root = BaseRoot(descriptor);
        root["view"] = "tool";
        root["skills"] = Skills(tool.RequiredSkillRefs);
        var detail = Detail(tool, includeSchema: true);
        root["tools"] = new[] { detail };
        if (schemaOffset is null && Fits(root)) return Element(root);

        var schemaText = tool.InputSchema.GetRawText();
        var totalCharacters = ScalarCount(schemaText);
        var start = schemaOffset ?? 0;
        if (start > totalCharacters)
        {
            throw new WindowsAgentCapabilityDisclosureException();
        }

        detail = Detail(tool, includeSchema: false);
        detail["schema_delivery"] = "paged";
        detail["schema_sha256"] = Convert.ToHexString(
            SHA256.HashData(Encoding.UTF8.GetBytes(schemaText))).ToLowerInvariant();
        detail["schema_total_chars"] = totalCharacters;
        detail["next_schema_offset"] = start;
        root["tools"] = new[] { detail };
        root["guidance"] =
            "Read this tool's schema with capability_describe using the same " +
            "plugin_option and tool_option plus schema_offset=next_schema_offset. " +
            "Concatenate schema_chunk pages in offset order, with the same " +
            "schema_sha256, until next_schema_offset is null. These are explicit " +
            "pages, not a truncated schema; do not guess omitted parameters.";
        if (!Fits(root)) return Overflow(descriptor, toolOption);
        if (schemaOffset is null) return Element(root);

        var count = Math.Min(MaximumSchemaPageCharacters, totalCharacters - start);
        while (true)
        {
            detail["schema_offset"] = start;
            detail["schema_chunk"] = ScalarSlice(schemaText, start, count);
            detail["next_schema_offset"] = start + count < totalCharacters
                ? start + count
                : null;
            root["tools"] = new[] { detail };
            if (Fits(root)) return Element(root);
            if (count <= 1) return Overflow(descriptor, toolOption);
            count /= 2;
        }
    }

    private static Dictionary<string, object?> CatalogRoot(
        WindowsAgentCapabilityDescriptor descriptor,
        IReadOnlyList<Dictionary<string, object?>> tools,
        int totalMatching,
        int offset,
        int? nextOffset)
    {
        var root = BaseRoot(descriptor);
        root["view"] = "catalog";
        root["total_available_tools"] = descriptor.Tools.Count;
        root["total_matching_tools"] = totalMatching;
        root["offset"] = offset;
        root["next_offset"] = nextOffset;
        root["tools"] = tools;
        root["skills"] = Skills(tools
            .SelectMany(tool => (IReadOnlyList<string>)tool["required_skill_refs"]!)
            .Distinct(StringComparer.Ordinal));
        root["guidance"] =
            "This is an index, not callable tool schemas. Read only the chosen " +
            "tool with capability_describe(plugin_option, tool_option). Use query " +
            "with tool names/keywords to narrow the index, or offset=next_offset " +
            "for more entries. Activate the exact required_skill_refs before execution.";
        return root;
    }

    private static Dictionary<string, object?> BaseRoot(
        WindowsAgentCapabilityDescriptor descriptor) => new(StringComparer.Ordinal)
    {
        ["disclosure_version"] = 2,
        ["plugin_option"] = descriptor.PluginOption,
        ["name"] = descriptor.Name,
        ["plugin_key"] = descriptor.PluginKey,
        ["component_key"] = descriptor.ComponentKey,
    };

    private static Dictionary<string, object?> Brief(
        WindowsAgentCapabilityToolDescriptor tool) => new(StringComparer.Ordinal)
    {
        ["tool_option"] = tool.ToolOption,
        ["name"] = tool.Name,
        ["description"] = tool.Description,
        ["approval"] = tool.Approval,
        ["required_skill_refs"] = tool.RequiredSkillRefs,
    };

    private static Dictionary<string, object?> Detail(
        WindowsAgentCapabilityToolDescriptor tool,
        bool includeSchema)
    {
        var value = Brief(tool);
        if (includeSchema) value["input_schema"] = tool.InputSchema;
        return value;
    }

    private static IReadOnlyList<object> Skills(IEnumerable<string> skillRefs) => skillRefs
        .Distinct(StringComparer.Ordinal)
        .Order(StringComparer.Ordinal)
        .Select(skillRef => (object)new
        {
            skill_ref = skillRef,
            name = skillRef[(skillRef.LastIndexOf(':') + 1)..],
            role = "leaf",
        })
        .ToArray();

    private static JsonElement Overflow(
        WindowsAgentCapabilityDescriptor descriptor,
        string? toolOption) => Element(new Dictionary<string, object?>(StringComparer.Ordinal)
    {
        ["disclosure_version"] = 2,
        ["view"] = "error",
        ["error"] = "capability_description_entry_too_large",
        ["plugin_option"] = descriptor.PluginOption,
        ["tool_option"] = toolOption,
        ["guidance"] = "The catalog or tool metadata alone exceeds the safe response " +
            "budget. No schema was truncated. The capability publisher must reduce " +
            "this metadata before this entry can be used.",
    });

    private static bool Fits(object value) =>
        ScalarCount(JsonSerializer.Serialize(value, JsonOptions)) <= MaximumResponseCharacters;

    private static JsonElement Element(object value) =>
        JsonSerializer.SerializeToElement(value, JsonOptions);

    private static int ScalarCount(string value) => value.EnumerateRunes().Count();

    private static string ScalarSlice(string value, int offset, int count)
    {
        var builder = new StringBuilder();
        var index = 0;
        foreach (var rune in value.EnumerateRunes())
        {
            if (index >= offset && index < offset + count) builder.Append(rune);
            if (++index >= offset + count) break;
        }
        return builder.ToString();
    }
}

internal sealed class WindowsAgentCapabilityDisclosureException : Exception
{
    public WindowsAgentCapabilityDisclosureException()
        : base("Capability description arguments are invalid. For a catalog page, " +
            "omit tool_option and schema_offset (or pass null). For one tool, use " +
            "the exact tool_option returned by the catalog and omit query. Only pass " +
            "schema_offset after that tool explicitly returns next_schema_offset.")
    {
    }
}
