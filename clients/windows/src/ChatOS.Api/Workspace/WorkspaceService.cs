using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Workspace;

public sealed class WorkspaceService : IWorkspaceService
{
    private readonly ChatOSApiClient _client;

    public WorkspaceService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<WorkspaceSnapshot> FetchWorkspaceAsync(
        CancellationToken cancellationToken = default)
    {
        var projectsTask = _client.GetAsync<IReadOnlyList<ProjectDto>>(
            "projects",
            cancellationToken);
        var contactsTask = _client.GetAsync<IReadOnlyList<ContactDto>>(
            "contacts?limit=500&offset=0",
            cancellationToken);
        var conversationsTask = _client.GetAsync<IReadOnlyList<ConversationDto>>(
            "conversations?limit=500&offset=0",
            cancellationToken);

        await Task.WhenAll(projectsTask, contactsTask, conversationsTask).ConfigureAwait(false);
        return new WorkspaceSnapshot(
            projectsTask.Result.Select(static value => value.ToDomain()).ToArray(),
            contactsTask.Result.Select(static value => value.ToDomain()).ToArray(),
            conversationsTask.Result.Select(static value => value.ToDomain()).ToArray());
    }
}

internal sealed record ProjectDto
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

    public WorkspaceProject ToDomain()
    {
        var rootPath = RootPath.TrimmedOrNull() ?? DisplayRootPath.TrimmedOrNull();
        var displayPath = DisplayRootPath.TrimmedOrNull() ?? RootPath.TrimmedOrNull();
        return new WorkspaceProject(
            Id,
            Name,
            rootPath,
            displayPath,
            LatestConversationId.TrimmedOrNull());
    }
}

internal sealed record ContactDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("agent_id")]
    public required string AgentId { get; init; }

    [JsonPropertyName("agent_name_snapshot")]
    public string? Name { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    public WorkspaceContact ToDomain() => new(
        Id,
        AgentId,
        Name.TrimmedOrNull() ?? AgentId,
        Status.TrimmedOrNull());
}

internal sealed record ConversationDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("title")]
    public required string Title { get; init; }

    [JsonPropertyName("project_id")]
    public string? ProjectId { get; init; }

    [JsonPropertyName("created_at")]
    public string? CreatedAt { get; init; }

    [JsonPropertyName("updated_at")]
    public string? UpdatedAt { get; init; }

    [JsonPropertyName("message_count")]
    public int? MessageCount { get; init; }

    [JsonPropertyName("archived")]
    public bool? Archived { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("metadata")]
    public JsonElement Metadata { get; init; }

    public WorkspaceConversation ToDomain()
    {
        var metadata = Metadata.AsObject();
        var source = metadata.Object("source_metadata") ?? metadata;
        var runtime = source.Object("chat_runtime") ?? JsonObject.Empty;
        var contact = source.Object("contact") ?? JsonObject.Empty;
        var uiContact = source.Object("ui_contact") ?? JsonObject.Empty;
        var projectId = ProjectScope(ProjectId)
            ?? ProjectScope(runtime.FirstString("project_id", "projectId"));
        var contactId = contact.FirstString("contact_id", "contactId")
            ?? uiContact.FirstString("contact_id", "contactId");
        var contactAgentId = contact.FirstString("agent_id", "agentId")
            ?? runtime.FirstString("contact_agent_id", "contactAgentId")
            ?? uiContact.FirstString("agent_id", "agentId");
        var normalizedStatus = Status?.Trim().ToLowerInvariant();

        return new WorkspaceConversation(
            Id,
            Title,
            projectId,
            contactId,
            contactAgentId,
            Math.Max(0, MessageCount ?? 0),
            ParseDate(UpdatedAt) ?? ParseDate(CreatedAt) ?? DateTimeOffset.MinValue,
            Archived == true || normalizedStatus is "archived" or "archiving");
    }

    private static string? ProjectScope(string? value)
    {
        var normalized = value.TrimmedOrNull();
        return normalized is "-1" or "0" ? null : normalized;
    }

    private static DateTimeOffset? ParseDate(string? value) =>
        DateTimeOffset.TryParse(value, out var date) ? date : null;
}

internal readonly record struct JsonObject(JsonElement Value)
{
    public static JsonObject Empty { get; } = new(default);

    public JsonObject? Object(string name)
    {
        if (Value.ValueKind != JsonValueKind.Object ||
            !Value.TryGetProperty(name, out var child) ||
            child.ValueKind != JsonValueKind.Object)
        {
            return null;
        }

        return new JsonObject(child);
    }

    public string? FirstString(params string[] names)
    {
        if (Value.ValueKind != JsonValueKind.Object)
        {
            return null;
        }

        foreach (var name in names)
        {
            if (Value.TryGetProperty(name, out var child) && child.ValueKind == JsonValueKind.String)
            {
                var value = child.GetString().TrimmedOrNull();
                if (value is not null)
                {
                    return value;
                }
            }
        }

        return null;
    }
}

internal static class WorkspaceJsonExtensions
{
    public static JsonObject AsObject(this JsonElement element)
    {
        if (element.ValueKind == JsonValueKind.Object)
        {
            return new JsonObject(element);
        }

        if (element.ValueKind == JsonValueKind.String)
        {
            try
            {
                using var document = JsonDocument.Parse(element.GetString() ?? string.Empty);
                return document.RootElement.ValueKind == JsonValueKind.Object
                    ? new JsonObject(document.RootElement.Clone())
                    : JsonObject.Empty;
            }
            catch (JsonException)
            {
                return JsonObject.Empty;
            }
        }

        return JsonObject.Empty;
    }

    public static string? TrimmedOrNull(this string? value)
    {
        value = value?.Trim();
        return string.IsNullOrEmpty(value) ? null : value;
    }
}
