using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Notepad;

public sealed class NotepadService : INotepadService
{
    private readonly ChatOSApiClient _client;

    public NotepadService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task InitializeAsync(CancellationToken cancellationToken = default) =>
        _ = await _client.GetAsync<SimpleResponse>("notepad/init", cancellationToken).ConfigureAwait(false);

    public async Task<IReadOnlyList<string>> ListFoldersAsync(CancellationToken cancellationToken = default) =>
        (await _client.GetAsync<FoldersResponse>("notepad/folders", cancellationToken).ConfigureAwait(false)).Folders;

    public async Task CreateFolderAsync(string folder, CancellationToken cancellationToken = default) =>
        _ = await _client.PostAsync<SimpleResponse>(
            "notepad/folders",
            new FolderRequest(folder),
            cancellationToken).ConfigureAwait(false);

    public async Task RenameFolderAsync(
        string from,
        string to,
        CancellationToken cancellationToken = default) =>
        _ = await _client.SendAsync<SimpleResponse>(
            HttpMethod.Patch,
            "notepad/folders",
            new RenameFolderRequest(from, to),
            cancellationToken).ConfigureAwait(false);

    public async Task DeleteFolderAsync(
        string folder,
        bool recursive,
        CancellationToken cancellationToken = default) =>
        _ = await _client.DeleteAsync<SimpleResponse>(
            $"notepad/folders?folder={Query(folder)}&recursive={recursive.ToString().ToLowerInvariant()}",
            cancellationToken).ConfigureAwait(false);

    public async Task<IReadOnlyList<NotepadNote>> ListNotesAsync(
        string? query,
        int limit = 500,
        CancellationToken cancellationToken = default)
    {
        var path = $"notepad/notes?recursive=true&limit={Math.Clamp(limit, 1, 500)}";
        if (!string.IsNullOrWhiteSpace(query))
        {
            path += $"&query={Query(query.Trim())}";
        }

        var response = await _client.GetAsync<NotesResponse>(path, cancellationToken).ConfigureAwait(false);
        return response.Notes.Select(static note => note.ToDomain()).ToArray();
    }

    public async Task<NotepadNoteDetail> CreateNoteAsync(
        NotepadNoteDraft draft,
        CancellationToken cancellationToken = default) =>
        (await _client.PostAsync<NoteDetailResponse>(
            "notepad/notes",
            draft,
            cancellationToken).ConfigureAwait(false)).ToDomain();

    public async Task<NotepadNoteDetail> FetchNoteAsync(
        string id,
        CancellationToken cancellationToken = default) =>
        (await _client.GetAsync<NoteDetailResponse>(
            $"notepad/notes/{PathSegment(id)}",
            cancellationToken).ConfigureAwait(false)).ToDomain();

    public async Task<NotepadNoteDetail> UpdateNoteAsync(
        string id,
        NotepadNoteUpdate update,
        CancellationToken cancellationToken = default) =>
        (await _client.SendAsync<NoteDetailResponse>(
            HttpMethod.Patch,
            $"notepad/notes/{PathSegment(id)}",
            update,
            cancellationToken).ConfigureAwait(false)).ToDomain();

    public async Task DeleteNoteAsync(string id, CancellationToken cancellationToken = default) =>
        _ = await _client.DeleteAsync<SimpleResponse>(
            $"notepad/notes/{PathSegment(id)}",
            cancellationToken).ConfigureAwait(false);

    private static string Query(string value) => Uri.EscapeDataString(value);

    private static string PathSegment(string value) => Uri.EscapeDataString(value).Replace("%2F", "%2F", StringComparison.OrdinalIgnoreCase);
}

internal sealed record SimpleResponse(bool? Ok);

internal sealed record FoldersResponse
{
    [JsonPropertyName("folders")]
    public IReadOnlyList<string> Folders { get; init; } = Array.Empty<string>();
}

internal sealed record NotesResponse
{
    [JsonPropertyName("notes")]
    public IReadOnlyList<NotepadNoteDto> Notes { get; init; } = Array.Empty<NotepadNoteDto>();
}

internal sealed record NoteDetailResponse
{
    [JsonPropertyName("note")]
    public NotepadNoteDto? Note { get; init; }

    [JsonPropertyName("content")]
    public string? Content { get; init; }

    public NotepadNoteDetail ToDomain() => new(
        Note?.ToDomain() ?? throw new ChatOSApiException("记事本响应缺少 note。"),
        Content ?? string.Empty);
}

internal sealed record NotepadNoteDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("title")]
    public string? Title { get; init; }

    [JsonPropertyName("folder")]
    public string? Folder { get; init; }

    [JsonPropertyName("tags")]
    public IReadOnlyList<string>? Tags { get; init; }

    [JsonPropertyName("created_at")]
    public string? CreatedAt { get; init; }

    [JsonPropertyName("updated_at")]
    public string? UpdatedAt { get; init; }

    [JsonPropertyName("file")]
    public string? File { get; init; }

    public NotepadNote ToDomain() => new(
        Id,
        Title ?? string.Empty,
        Folder ?? string.Empty,
        Tags ?? Array.Empty<string>(),
        ParseDate(CreatedAt),
        ParseDate(UpdatedAt),
        File ?? string.Empty);

    private static DateTimeOffset? ParseDate(string? value) =>
        DateTimeOffset.TryParse(value, out var parsed) ? parsed : null;
}

internal sealed record FolderRequest(
    [property: JsonPropertyName("folder")] string Folder);

internal sealed record RenameFolderRequest(
    [property: JsonPropertyName("from")] string From,
    [property: JsonPropertyName("to")] string To);
