using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using JsonNodeObject = System.Text.Json.Nodes.JsonObject;

namespace ChatOS.Api.Workspace;

public sealed class ProjectConversationService : IProjectConversationService
{
    private readonly ChatOSApiClient _client;

    public ProjectConversationService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<string> EnsureConversationAsync(
        WorkspaceProject project,
        WorkspaceContact contact,
        CancellationToken cancellationToken = default)
    {
        if (project.ProjectContext is null ||
            !string.Equals(project.ProjectContext.ProjectId, project.Id, StringComparison.Ordinal))
            throw new InvalidOperationException("The client project context is required.");
        var conversations = await _client.GetAsync<IReadOnlyList<ProjectConversationDto>>(
            $"conversations?project_id={Query(project.Id)}&limit=500&offset=0",
            cancellationToken).ConfigureAwait(false);
        var existing = conversations
            .Where(value => value.Matches(project.Id, contact))
            .OrderByDescending(static value => (value.MessageCount ?? 0) > 0)
            .ThenByDescending(static value => value.UpdatedAt, StringComparer.Ordinal)
            .FirstOrDefault();
        if (existing is not null)
        {
            if (!existing.HasProjectContext(project.ProjectContext))
            {
                var metadata = ProjectConversationMetadataDto.Create(project, contact);
                await _client.PutAsync<ProjectConversationDto>(
                    $"conversations/{Query(existing.Id)}",
                    new UpdateProjectConversationRequestDto(existing.Merge(metadata)),
                    cancellationToken).ConfigureAwait(false);
            }
            return existing.Id;
        }

        var created = await _client.PostAsync<ProjectConversationDto>(
            "conversations",
            CreateProjectConversationRequestDto.Create(project, contact),
            cancellationToken).ConfigureAwait(false);
        return created.Id;
    }

    private static string Query(string value) => Uri.EscapeDataString(value);
}

internal sealed record ProjectConversationDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("project_id")]
    public string? ProjectId { get; init; }

    [JsonPropertyName("message_count")]
    public int? MessageCount { get; init; }

    [JsonPropertyName("updated_at")]
    public string? UpdatedAt { get; init; }

    [JsonPropertyName("metadata")]
    public JsonElement Metadata { get; init; }

    public bool Matches(string projectId, WorkspaceContact contact)
    {
        if (!string.Equals(ProjectId.TrimmedOrNull(), projectId, StringComparison.Ordinal))
        {
            return false;
        }

        var root = Metadata.AsObject();
        var source = root.Object("source_metadata") ?? root;
        var runtime = source.Object("chat_runtime") ?? JsonObject.Empty;
        var metadataContact = source.Object("contact") ?? JsonObject.Empty;
        var uiContact = source.Object("ui_contact") ?? JsonObject.Empty;
        var contactId = metadataContact.FirstString("contact_id", "contactId")
            ?? uiContact.FirstString("contact_id", "contactId");
        if (contactId is not null)
        {
            return string.Equals(contactId, contact.Id, StringComparison.Ordinal);
        }

        var agentId = metadataContact.FirstString("agent_id", "agentId")
            ?? runtime.FirstString("contact_agent_id", "contactAgentId")
            ?? uiContact.FirstString("agent_id", "agentId");
        return string.Equals(agentId, contact.AgentId, StringComparison.Ordinal);
    }

    public bool HasProjectContext(ProjectContextSnapshot expected)
    {
        var root = Metadata.AsObject();
        var source = root.Object("source_metadata") ?? root;
        var runtime = source.Object("chat_runtime") ?? JsonObject.Empty;
        var element = runtime.Element("project_context");
        if (element is null) return false;
        try { return element.Value.Deserialize<ProjectContextSnapshot>() == expected; }
        catch (JsonException) { return false; }
    }

    public JsonNodeObject Merge(ProjectConversationMetadataDto current)
    {
        JsonNodeObject root;
        try { root = Metadata.ValueKind == JsonValueKind.Object
                ? JsonNode.Parse(Metadata.GetRawText())?.AsObject() ?? new JsonNodeObject()
                : new JsonNodeObject(); }
        catch (JsonException) { root = new JsonNodeObject(); }
        var target = root["source_metadata"] as JsonNodeObject ?? root;
        var value = JsonSerializer.SerializeToNode(current)?.AsObject()
            ?? throw new InvalidOperationException("Failed to encode project conversation metadata.");
        foreach (var pair in value) target[pair.Key] = pair.Value?.DeepClone();
        return root;
    }
}

internal sealed record UpdateProjectConversationRequestDto(
    [property: JsonPropertyName("metadata")] JsonNodeObject Metadata);

internal sealed record CreateProjectConversationRequestDto(
    [property: JsonPropertyName("title")] string Title,
    [property: JsonPropertyName("project_id")] string ProjectId,
    [property: JsonPropertyName("metadata")] ProjectConversationMetadataDto Metadata)
{
    public static CreateProjectConversationRequestDto Create(
        WorkspaceProject project,
        WorkspaceContact contact) => new(
        contact.Name,
        project.Id,
        ProjectConversationMetadataDto.Create(project, contact));
}

internal sealed record ProjectConversationMetadataDto(
    [property: JsonPropertyName("chat_runtime")] ProjectChatRuntimeDto ChatRuntime,
    [property: JsonPropertyName("contact")] ProjectContactIdentityDto Contact,
    [property: JsonPropertyName("ui_chat_selection")] ProjectChatSelectionDto UiChatSelection,
    [property: JsonPropertyName("ui_contact")] ProjectContactIdentityDto UiContact)
{
    public static ProjectConversationMetadataDto Create(
        WorkspaceProject project,
        WorkspaceContact contact)
    {
        var identity = new ProjectContactIdentityDto("memory_agent", contact.Id, contact.AgentId);
        return new ProjectConversationMetadataDto(
            new ProjectChatRuntimeDto(project.Id, project.RootPath,
                project.ProjectContext ?? throw new InvalidOperationException("The client project context is required."),
                contact.AgentId),
            identity,
            new ProjectChatSelectionDto(contact.AgentId),
            identity);
    }
}

internal sealed record ProjectChatRuntimeDto(
    [property: JsonPropertyName("project_id")] string ProjectId,
    [property: JsonPropertyName("project_root")] string? ProjectRoot,
    [property: JsonPropertyName("project_context")] ProjectContextSnapshot ProjectContext,
    [property: JsonPropertyName("contact_agent_id")] string ContactAgentId);

internal sealed record ProjectContactIdentityDto(
    [property: JsonPropertyName("type")] string Type,
    [property: JsonPropertyName("contact_id")] string ContactId,
    [property: JsonPropertyName("agent_id")] string AgentId);

internal sealed record ProjectChatSelectionDto(
    [property: JsonPropertyName("selected_agent_id")] string SelectedAgentId);
