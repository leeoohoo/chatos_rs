using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Workspace;

public sealed class WorkspaceResourceCreationService : IWorkspaceResourceCreationService
{
    private static readonly TimeSpan HarnessImportTimeout = TimeSpan.FromHours(2);
    private readonly ChatOSApiClient _client;

    public WorkspaceResourceCreationService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<WorkspaceProject> CreateLocalProjectAsync(
        LocalProjectCreationDraft draft,
        CancellationToken cancellationToken = default)
    {
        var gitUrl = draft.RepositoryMode == LocalProjectRepositoryMode.External
            ? draft.GitUrl.TrimmedOrNull()
            : null;
        if (draft.RepositoryMode == LocalProjectRepositoryMode.External && gitUrl is null)
        {
            throw new ChatOSApiException(
                "Using an existing Git repository requires a configured remote URL.");
        }
        var request = new CreateLocalProjectRequestDto(
            draft.Name,
            draft.DeviceId,
            draft.WorkspaceId,
            draft.RelativePath,
            draft.RepositoryMode == LocalProjectRepositoryMode.Managed ? "managed" : "external",
            gitUrl);
        var created = draft.RepositoryMode == LocalProjectRepositoryMode.Managed
            ? await _client.PostAsync<CreatedWorkspaceProjectDto>(
                "local-connectors/projects",
                request,
                HarnessImportTimeout,
                cancellationToken).ConfigureAwait(false)
            : await _client.PostAsync<CreatedWorkspaceProjectDto>(
                "local-connectors/projects",
                request,
                cancellationToken).ConfigureAwait(false);
        return created.ToDomain();
    }

    public async Task BindContactAsync(
        string projectId,
        string contactId,
        CancellationToken cancellationToken = default)
    {
        _ = await _client.PostAsync<ProjectContactLinkDto>(
            $"projects/{Path(projectId)}/contacts",
            new BindProjectContactRequestDto(contactId),
            cancellationToken).ConfigureAwait(false);
    }

    public async Task<string> EnsureConversationAsync(
        WorkspaceProject project,
        WorkspaceContact contact,
        CancellationToken cancellationToken = default)
    {
        var links = await _client.GetAsync<IReadOnlyList<ProjectContactLinkDto>>(
            $"projects/{Path(project.Id)}/contacts?limit=500&offset=0",
            cancellationToken).ConfigureAwait(false);
        var matchingLink = links.FirstOrDefault(link =>
            string.Equals(link.ContactId, contact.Id, StringComparison.Ordinal));
        if (matchingLink?.LatestConversationId.TrimmedOrNull() is { } linkedConversationId)
        {
            return linkedConversationId;
        }

        if (matchingLink is null)
        {
            await BindContactAsync(project.Id, contact.Id, cancellationToken).ConfigureAwait(false);
        }

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
            return existing.Id;
        }

        var created = await _client.PostAsync<ProjectConversationDto>(
            "conversations",
            CreateProjectConversationRequestDto.Create(project, contact),
            cancellationToken).ConfigureAwait(false);
        return created.Id;
    }

    private static string Path(string value) => Uri.EscapeDataString(value);

    private static string Query(string value) => Uri.EscapeDataString(value);
}

internal sealed record CreateLocalProjectRequestDto(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("device_id")] string DeviceId,
    [property: JsonPropertyName("workspace_id")] string WorkspaceId,
    [property: JsonPropertyName("relative_path")] string? RelativePath,
    [property: JsonPropertyName("repository_mode")] string RepositoryMode,
    [property: JsonPropertyName("git_url")] string? GitUrl);

internal sealed record BindProjectContactRequestDto(
    [property: JsonPropertyName("contact_id")] string ContactId);

internal sealed record CreatedWorkspaceProjectDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("name")]
    public required string Name { get; init; }

    [JsonPropertyName("root_path")]
    public string? RootPath { get; init; }

    [JsonPropertyName("display_root_path")]
    public string? DisplayRootPath { get; init; }

    [JsonPropertyName("latest_session_id")]
    public string? LatestConversationId { get; init; }

    public WorkspaceProject ToDomain() => new(
        Id,
        Name,
        RootPath.TrimmedOrNull(),
        DisplayRootPath.TrimmedOrNull() ?? RootPath.TrimmedOrNull(),
        LatestConversationId.TrimmedOrNull());
}

internal sealed record ProjectContactLinkDto
{
    [JsonPropertyName("contact_id")]
    public string? ContactId { get; init; }

    [JsonPropertyName("latest_session_id")]
    public string? LatestConversationId { get; init; }
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
}

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
            new ProjectChatRuntimeDto(project.Id, project.RootPath, contact.AgentId),
            identity,
            new ProjectChatSelectionDto(contact.AgentId),
            identity);
    }
}

internal sealed record ProjectChatRuntimeDto(
    [property: JsonPropertyName("project_id")] string ProjectId,
    [property: JsonPropertyName("project_root")] string? ProjectRoot,
    [property: JsonPropertyName("contact_agent_id")] string ContactAgentId);

internal sealed record ProjectContactIdentityDto(
    [property: JsonPropertyName("type")] string Type,
    [property: JsonPropertyName("contact_id")] string ContactId,
    [property: JsonPropertyName("agent_id")] string AgentId);

internal sealed record ProjectChatSelectionDto(
    [property: JsonPropertyName("selected_agent_id")] string SelectedAgentId);
