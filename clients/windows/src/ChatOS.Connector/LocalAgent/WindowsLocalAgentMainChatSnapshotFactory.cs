using System.Security.Cryptography;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.RegularExpressions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalAgentMainChatSnapshots(
    LocalAgentFrozenSnapshot Prompt,
    LocalAgentFrozenSnapshot Capabilities,
    LocalAgentFrozenSnapshot? Project);

/// Builds the immutable Main Chat inputs consumed by the shared Rust Host.
/// Payload objects are recursively key-ordered before their exact UTF-8 bytes
/// are hashed, so the digest is deterministic across process restarts.
public sealed partial class WindowsLocalAgentMainChatSnapshotFactory
{
    public const string BasePrompt = """
        You are ChatOS's primary collaboration agent. Focus first on understanding and improving the user's intended outcome. Give a direct answer when no external action is needed. Ask the user only when a missing decision materially changes the result; do not request confirmation for reversible, read-only, or already-authorized work. For UI and website design, work visually and incrementally: establish art direction and composition, use visual references when available, refine one bounded region at a time, and prioritize a polished interface over speculative interaction behavior. Never claim that files, code, deployments, or external systems changed unless a completed local Task result proves it.
        """;

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = null,
        WriteIndented = false,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };

    public WindowsLocalAgentMainChatSnapshots Make(
        LocalAgentContactRuntimeContext? contact,
        LocalProjectRecord? project)
    {
        var contactPrompt = contact is null ? null : MakeContactPrompt(contact);
        var skillPrompt = contact is null ? null : MakeSkillCatalogPrompt(contact);
        foreach (var value in new[] { BasePrompt, contactPrompt, skillPrompt }.OfType<string>())
        {
            if (ContainsLocalPath(value))
            {
                throw new InvalidDataException("The Main Chat prompt contains a local filesystem path.");
            }
        }

        var promptSource = Object(
            ("base_system_prompt", BasePrompt),
            ("contact_system_prompt", contactPrompt),
            ("skill_catalog_prompt", skillPrompt));
        var promptSourceDigest = Digest(promptSource)["sha256:".Length..];
        var promptRevision = $"main-chat-native-v1-{promptSourceDigest[..16]}";
        var prompt = MakeSnapshot(
            "main-chat-prompt",
            promptRevision,
            Object(
                ("base_system_prompt", BasePrompt),
                ("contact_system_prompt", contactPrompt),
                ("prompt_revision", promptRevision),
                ("skill_catalog_prompt", skillPrompt)));

        const string capabilityId = "main-chat-capabilities-v1";
        var capabilities = MakeSnapshot(
            capabilityId,
            "1",
            Object(
                ("allowed_tools", new[] { "ask_user", "create_local_task" }),
                ("snapshot_ref", capabilityId)));
        return new WindowsLocalAgentMainChatSnapshots(
            prompt,
            capabilities,
            project is null ? null : MakeProjectSnapshot(project));
    }

    private static string MakeContactPrompt(LocalAgentContactRuntimeContext contact)
    {
        ValidateIdentity(contact.AgentId, nameof(contact.AgentId));
        ValidateText(contact.Name, nameof(contact.Name));
        ValidateText(contact.RoleDefinition, nameof(contact.RoleDefinition));
        ValidateIdentity(contact.Revision, nameof(contact.Revision));
        if (contact.Description is { } description) ValidateText(description, nameof(contact.Description));
        if (contact.Category is { } category) ValidateText(category, nameof(contact.Category));
        return string.Join('\n', new[]
        {
            $"Contact Agent: {contact.Name}",
            contact.Description is null ? null : $"Description: {contact.Description}",
            contact.Category is null ? null : $"Category: {contact.Category}",
            $"Role: {contact.RoleDefinition}",
        }.OfType<string>());
    }

    private static string? MakeSkillCatalogPrompt(LocalAgentContactRuntimeContext contact)
    {
        if (contact.Skills.Count == 0) return null;
        var seen = new HashSet<string>(StringComparer.Ordinal);
        var skills = contact.Skills.OrderBy(skill => skill.Id, StringComparer.Ordinal).Select(skill =>
        {
            ValidateIdentity(skill.Id, nameof(skill.Id));
            ValidateText(skill.Name, nameof(skill.Name));
            ValidateText(skill.Content, nameof(skill.Content));
            if (!seen.Add(skill.Id))
            {
                throw new InvalidDataException("The contact runtime context contains duplicate skills.");
            }
            return $"Skill {skill.Id} — {skill.Name}:\n{skill.Content}";
        });
        return $"Frozen Skill Catalog:\n\n{string.Join("\n\n", skills)}";
    }

    private static LocalAgentFrozenSnapshot MakeProjectSnapshot(LocalProjectRecord project)
    {
        project.Validate();
        if (project.Status != LocalProjectStatus.Active)
        {
            throw new InvalidDataException("The Main Chat project is not active.");
        }
        if (ContainsLocalPath(project.Draft.Description))
        {
            throw new InvalidDataException("The Main Chat project description contains a local filesystem path.");
        }
        var revision = project.Revision.ToString(System.Globalization.CultureInfo.InvariantCulture);
        return MakeSnapshot(
            $"main-chat-project-{project.Id}",
            revision,
            Object(
                ("design_context", Object(("description", project.Draft.Description))),
                ("project_id", project.Id),
                ("project_name", project.Draft.Name),
                ("snapshot_revision", revision)));
    }

    private static LocalAgentFrozenSnapshot MakeSnapshot(
        string snapshotId,
        string revision,
        SortedDictionary<string, object?> payload)
    {
        ValidateIdentity(snapshotId, nameof(snapshotId));
        ValidateIdentity(revision, nameof(revision));
        return new LocalAgentFrozenSnapshot(
            snapshotId,
            revision,
            Digest(payload),
            JsonSerializer.SerializeToElement(payload, JsonOptions));
    }

    private static string Digest(SortedDictionary<string, object?> payload)
    {
        var bytes = JsonSerializer.SerializeToUtf8Bytes(payload, JsonOptions);
        return $"sha256:{Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant()}";
    }

    private static SortedDictionary<string, object?> Object(
        params (string Key, object? Value)[] fields)
    {
        var result = new SortedDictionary<string, object?>(StringComparer.Ordinal);
        foreach (var field in fields) result.Add(field.Key, field.Value);
        return result;
    }

    private static void ValidateIdentity(string value, string name)
    {
        if (string.IsNullOrEmpty(value)
            || value.Length > 512
            || value != value.Trim()
            || value.Any(char.IsControl))
        {
            throw new InvalidDataException($"The Main Chat {name} is invalid.");
        }
    }

    private static void ValidateText(string value, string name)
    {
        if (string.IsNullOrEmpty(value)
            || value.Length > 256 * 1024
            || value != value.Trim()
            || value.Contains('\0'))
        {
            throw new InvalidDataException($"The Main Chat {name} is invalid.");
        }
    }

    private static bool ContainsLocalPath(string value) =>
        value.Contains("file://", StringComparison.OrdinalIgnoreCase)
        || value.Contains("/Users/", StringComparison.Ordinal)
        || value.Contains("/Volumes/", StringComparison.Ordinal)
        || value.Contains("/home/", StringComparison.Ordinal)
        || WindowsPathPattern().IsMatch(value);

    [GeneratedRegex(@"[A-Za-z]:[\\/]")]
    private static partial Regex WindowsPathPattern();
}
