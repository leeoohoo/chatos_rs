using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalAgentProductSkillBundleSnapshot(
    string Json,
    string Revision,
    string PolicyInstructions,
    JsonElement CatalogInputItem);

/// <summary>
/// Builds the immutable Main Chat Product Skill bundle from the same canonical Markdown
/// resources used by macOS. Windows deliberately owns only the transport/packaging adapter;
/// policy and workflow content remain single-sourced under clients/macos.
/// </summary>
internal static class WindowsLocalAgentProductSkillBundle
{
    private const string ProductSkillPrefix = "product-skill:";
    private const string ResourcePrefix = "ChatOS.AgentSkills.";

    private static readonly SkillDescriptor[] MainChatSkills =
    [
        new("chatos-async-task-orchestration", "specialist",
        [
            "references/acceptance-examples-and-antipatterns.md",
            "references/capabilities-and-plugins.md",
            "references/complexity-and-decomposition.md",
            "references/design-and-creative.md",
            "references/documents-presentations-and-media.md",
            "references/followups-cancellation-and-recovery.md",
            "references/graph-patterns-and-dependencies.md",
            "references/research-data-and-operations.md",
            "references/software-engineering.md",
            "references/writing-content-and-marketing.md",
        ]),
        new("chatos-main-chat-invariants", "policy", []),
        new("chatos-requirement-discovery", "specialist",
        [
            "references/proposal-binding.md",
            "references/scenarios-and-question-design.md",
        ]),
        new("chatos-skill-runtime", "policy", []),
    ];

    public static WindowsLocalAgentProductSkillBundleSnapshot BuildMainChat()
    {
        var documents = MainChatSkills.Select(Document).ToArray();
        var policy = documents.Single(value =>
            string.Equals(value["name"] as string, "chatos-main-chat-invariants",
                StringComparison.Ordinal));
        var task = documents.Single(value =>
            string.Equals(value["name"] as string, "chatos-async-task-orchestration",
                StringComparison.Ordinal));
        var survey = documents.Single(value =>
            string.Equals(value["name"] as string, "chatos-requirement-discovery",
                StringComparison.Ordinal));
        var bindings = new[]
        {
            Binding(
                "chatos.local.agent-chat",
                "agent-chat.skill-control-plane",
                ["agent_skill_activate", "agent_skill_read_resource"],
                "chatos-skill-runtime",
                "chatos-skill-runtime",
                "control_plane"),
            Binding(
                "chatos.local.main-chat-task",
                "main-chat.task-orchestration",
                WindowsLocalAgentCapabilityCatalog.MainChatTaskToolNames,
                "chatos-async-task-orchestration",
                "chatos-async-task-orchestration",
                "on_demand"),
            Binding(
                "chatos.local.requirement-survey",
                "project.requirement-discovery",
                ["requirement_plan_save", "requirement_survey_create"],
                "chatos-requirement-discovery",
                "chatos-requirement-discovery",
                "on_demand"),
        };
        var unversioned = Bundle(1, "unversioned", documents, bindings);
        var revision = Digest(CanonicalJson(unversioned), prefix: true);
        var json = CanonicalJson(Bundle(1, revision, documents, bindings));
        var taskRef = ProductSkillPrefix + (string)task["name"]!;
        var surveyRef = ProductSkillPrefix + (string)survey["name"]!;
        var catalog = JsonSerializer.SerializeToElement(new
        {
            type = "message",
            role = "system",
            content = new[]
            {
                new
                {
                    type = "input_text",
                    text = "[ChatOS Product Skill Catalog]\n" +
                        "The following Skills are available on demand. Requirement discovery is a companion capability, not a ninth Task tool.\n" +
                        $"- `{taskRef}` = **{task["name"]}**: {task["description"]}\n" +
                        $"- `{surveyRef}` = **{survey["name"]}**: {survey["description"]}\n\n" +
                        "Before calling a Task or Requirement Survey tool, call `agent_skill_activate` with its exact `skill_ref` and follow the returned instructions. " +
                        "Clarify material unknown goals or preferences before dispatching dependent work. The activation result already lists resources; read only relevant references with `agent_skill_read_resource`. " +
                        "If that result is no longer available after resume, activate the same exact ref again. Skill activation does not grant permissions or expand the user's requested scope.",
                },
            },
        });
        return new(json, revision, (string)policy["instructions"]!, catalog);
    }

    private static Dictionary<string, object?> Document(SkillDescriptor descriptor)
    {
        var instructions = ReadResource(descriptor.Name, "SKILL.md").Trim();
        if (!string.Equals(FrontmatterValue(instructions, "name"), descriptor.Name,
                StringComparison.Ordinal))
        {
            throw new InvalidDataException(
                $"The bundled Product Skill has an invalid identity: {descriptor.Name}");
        }
        var description = FrontmatterValue(instructions, "description") ??
            throw new InvalidDataException(
                $"The bundled Product Skill has no description: {descriptor.Name}");
        return new()
        {
            ["skill_ref"] = ProductSkillPrefix + descriptor.Name,
            ["name"] = descriptor.Name,
            ["role"] = descriptor.Role,
            ["description"] = description,
            ["instructions"] = instructions,
            ["instructions_sha256"] = Digest(instructions, prefix: true),
            ["resources"] = descriptor.Resources.Select(path =>
            {
                var content = ReadResource(descriptor.Name, path).Trim();
                return new Dictionary<string, object?>
                {
                    ["relative_path"] = path,
                    ["content"] = content,
                    ["sha256"] = Digest(content, prefix: true),
                };
            }).ToArray(),
        };
    }

    private static Dictionary<string, object?> Binding(
        string providerId,
        string bindingId,
        IReadOnlyCollection<string> toolNames,
        string routerSkill,
        string specialistSkill,
        string activationPolicy) => new()
    {
        ["provider_id"] = providerId,
        ["binding_id"] = bindingId,
        ["tool_names"] = toolNames.Distinct(StringComparer.Ordinal)
            .Order(StringComparer.Ordinal).ToArray(),
        ["router_skill_ref"] = ProductSkillPrefix + routerSkill,
        ["specialist_skill_ref"] = ProductSkillPrefix + specialistSkill,
        ["activation_policy"] = activationPolicy,
    };

    private static JsonElement Bundle(
        int schemaVersion,
        string revision,
        Dictionary<string, object?>[] documents,
        Dictionary<string, object?>[] bindings) => JsonSerializer.SerializeToElement(
            new Dictionary<string, object?>
            {
                ["schema_version"] = schemaVersion,
                ["bundle_revision"] = revision,
                ["documents"] = documents,
                ["tool_skill_bindings"] = bindings.OrderBy(value =>
                    (string)value["binding_id"]!, StringComparer.Ordinal).ToArray(),
            });

    private static string ReadResource(string skillName, string path)
    {
        var name = ResourcePrefix + skillName + "." + path.Replace('/', '.');
        using var stream = Assembly.GetExecutingAssembly().GetManifestResourceStream(name)
            ?? throw new InvalidDataException(
                $"The bundled Product Skill resource is missing: {skillName}/{path}");
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
                foreach (var property in value.EnumerateObject()
                    .OrderBy(property => property.Name, StringComparer.Ordinal))
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

    private sealed record SkillDescriptor(
        string Name,
        string Role,
        IReadOnlyList<string> Resources);
}
