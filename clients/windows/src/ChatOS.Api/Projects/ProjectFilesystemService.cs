using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Projects;

public sealed class ProjectFilesystemService : IProjectFilesystemService
{
    private readonly ChatOSApiClient _client;

    public ProjectFilesystemService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<ProjectDirectoryListing> ListEntriesAsync(
        string path,
        bool forceRefresh = false,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<EntriesDto>(
            $"fs/entries?path={Query(path)}&force_refresh={forceRefresh.ToString().ToLowerInvariant()}",
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain(path);
    }

    public async Task<IReadOnlyList<ProjectFileEntry>> SearchEntriesAsync(
        string path,
        string query,
        int limit = 100,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<EntriesDto>(
            $"fs/search?path={Query(path)}&q={Query(query)}&limit={Math.Clamp(limit, 1, 500)}",
            cancellationToken).ConfigureAwait(false);
        return response.Entries.Select(static value => value.ToDomain()).ToArray();
    }

    public async Task<IReadOnlyList<ProjectFileContentMatch>> SearchContentAsync(
        string path,
        string query,
        int limit = 100,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<ContentSearchDto>(
            $"fs/search-content?path={Query(path)}&q={Query(query)}&limit={Math.Clamp(limit, 1, 500)}",
            cancellationToken).ConfigureAwait(false);
        return response.Entries.Select(static value => value.ToDomain()).ToArray();
    }

    public async Task<ProjectFileContent> ReadFileAsync(
        string path,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<FileContentDto>(
            $"fs/read?path={Query(path)}",
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain();
    }

    public async Task WriteFileAsync(
        string path,
        string content,
        CancellationToken cancellationToken = default)
    {
        _ = await _client.PostAsync<MutationDto>(
            "fs/write",
            new WriteFileRequestDto(path, content),
            cancellationToken).ConfigureAwait(false);
    }

    public async Task CreateFileAsync(
        string parentPath,
        string name,
        CancellationToken cancellationToken = default)
    {
        _ = await _client.PostAsync<MutationDto>(
            "fs/touch",
            new CreateEntryRequestDto(parentPath, name, string.Empty),
            cancellationToken).ConfigureAwait(false);
    }

    public async Task CreateDirectoryAsync(
        string parentPath,
        string name,
        CancellationToken cancellationToken = default)
    {
        _ = await _client.PostAsync<MutationDto>(
            "fs/mkdir",
            new CreateEntryRequestDto(parentPath, name, null),
            cancellationToken).ConfigureAwait(false);
    }

    public async Task DeleteEntryAsync(
        string path,
        bool recursive,
        CancellationToken cancellationToken = default)
    {
        _ = await _client.PostAsync<MutationDto>(
            "fs/delete",
            new DeleteEntryRequestDto(path, recursive),
            cancellationToken).ConfigureAwait(false);
    }

    public async Task<ProjectFileMoveResult> MoveEntryAsync(
        string sourcePath,
        string targetParentPath,
        string? targetName = null,
        bool replaceExisting = false,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.PostAsync<MoveEntryDto>(
            "fs/move",
            new MoveEntryRequestDto(sourcePath, targetParentPath, targetName, replaceExisting),
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain(sourcePath);
    }

    public async Task OpenExternallyAsync(
        string path,
        ProjectFileExternalOpenMode mode,
        CancellationToken cancellationToken = default)
    {
        var apiMode = mode switch
        {
            ProjectFileExternalOpenMode.Reveal => "reveal",
            ProjectFileExternalOpenMode.Code => "code",
            _ => "default",
        };
        _ = await _client.PostAsync<MutationDto>(
            "fs/open",
            new OpenEntryRequestDto(path, apiMode),
            cancellationToken).ConfigureAwait(false);
    }

    private static string Query(string value) => Uri.EscapeDataString(value);
}

internal sealed record EntriesDto
{
    [JsonPropertyName("path")]
    public string? Path { get; init; }

    [JsonPropertyName("parent")]
    public string? Parent { get; init; }

    [JsonPropertyName("writable")]
    public bool? Writable { get; init; }

    [JsonPropertyName("entries")]
    public IReadOnlyList<ProjectFileEntryDto> Entries { get; init; } = Array.Empty<ProjectFileEntryDto>();

    [JsonPropertyName("truncated")]
    public bool? Truncated { get; init; }

    public ProjectDirectoryListing ToDomain(string fallbackPath) => new(
        Path ?? fallbackPath,
        Parent,
        Writable ?? false,
        Entries.Select(static value => value.ToDomain()).ToArray(),
        Truncated ?? false);
}

internal sealed record ProjectFileEntryDto
{
    [JsonPropertyName("name")]
    public string? Name { get; init; }

    [JsonPropertyName("path")]
    public string? Path { get; init; }

    [JsonPropertyName("display_path")]
    public string? DisplayPath { get; init; }

    [JsonPropertyName("is_dir")]
    public bool? IsDirectory { get; init; }

    [JsonPropertyName("writable")]
    public bool? Writable { get; init; }

    [JsonPropertyName("size")]
    public long? Size { get; init; }

    [JsonPropertyName("modified_at")]
    public string? ModifiedAt { get; init; }

    public ProjectFileEntry ToDomain()
    {
        var path = Path ?? string.Empty;
        var fallbackName = path.Replace('\\', '/').Split('/').LastOrDefault() ?? string.Empty;
        return new ProjectFileEntry(
            Name ?? fallbackName,
            path,
            DisplayPath,
            IsDirectory ?? false,
            Writable ?? false,
            Size,
            ParseDate(ModifiedAt));
    }

    private static DateTimeOffset? ParseDate(string? value) =>
        DateTimeOffset.TryParse(value, out var parsed) ? parsed : null;
}

internal sealed record FileContentDto
{
    [JsonPropertyName("path")]
    public string? Path { get; init; }

    [JsonPropertyName("display_path")]
    public string? DisplayPath { get; init; }

    [JsonPropertyName("relative_path")]
    public string? RelativePath { get; init; }

    [JsonPropertyName("name")]
    public string? Name { get; init; }

    [JsonPropertyName("content_type")]
    public string? ContentType { get; init; }

    [JsonPropertyName("is_binary")]
    public bool? IsBinary { get; init; }

    [JsonPropertyName("writable")]
    public bool? Writable { get; init; }

    [JsonPropertyName("size")]
    public long? Size { get; init; }

    [JsonPropertyName("modified_at")]
    public string? ModifiedAt { get; init; }

    [JsonPropertyName("content")]
    public string? Content { get; init; }

    public ProjectFileContent ToDomain()
    {
        var path = Path ?? string.Empty;
        return new ProjectFileContent(
            path,
            DisplayPath ?? RelativePath,
            Name ?? path.Replace('\\', '/').Split('/').LastOrDefault() ?? string.Empty,
            ContentType,
            IsBinary ?? false,
            Writable ?? false,
            Size ?? 0,
            DateTimeOffset.TryParse(ModifiedAt, out var parsed) ? parsed : null,
            Content ?? string.Empty);
    }
}

internal sealed record ContentSearchDto
{
    [JsonPropertyName("entries")]
    public IReadOnlyList<ContentMatchDto> Entries { get; init; } = Array.Empty<ContentMatchDto>();
}

internal sealed record ContentMatchDto
{
    [JsonPropertyName("path")]
    public string? Path { get; init; }

    [JsonPropertyName("relative_path")]
    public string? RelativePath { get; init; }

    [JsonPropertyName("line")]
    public int? Line { get; init; }

    [JsonPropertyName("column")]
    public int? Column { get; init; }

    [JsonPropertyName("text")]
    public string? Text { get; init; }

    public ProjectFileContentMatch ToDomain() => new(
        Path ?? RelativePath ?? string.Empty,
        RelativePath,
        Math.Max(1, Line ?? 1),
        Math.Max(1, Column ?? 1),
        Text ?? string.Empty);
}

internal sealed record MutationDto(
    [property: JsonPropertyName("success")] bool? Success);

internal sealed record WriteFileRequestDto(
    [property: JsonPropertyName("path")] string Path,
    [property: JsonPropertyName("content")] string Content);

internal sealed record CreateEntryRequestDto(
    [property: JsonPropertyName("parent_path")] string ParentPath,
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("content")] string? Content);

internal sealed record DeleteEntryRequestDto(
    [property: JsonPropertyName("path")] string Path,
    [property: JsonPropertyName("recursive")] bool Recursive);

internal sealed record MoveEntryRequestDto(
    [property: JsonPropertyName("source_path")] string SourcePath,
    [property: JsonPropertyName("target_parent_path")] string TargetParentPath,
    [property: JsonPropertyName("target_name")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? TargetName,
    [property: JsonPropertyName("replace_existing")] bool ReplaceExisting);

internal sealed record MoveEntryDto
{
    [JsonPropertyName("from_path")]
    public string? FromPath { get; init; }

    [JsonPropertyName("to_path")]
    public string? ToPath { get; init; }

    [JsonPropertyName("display_path")]
    public string? DisplayPath { get; init; }

    [JsonPropertyName("name")]
    public string? Name { get; init; }

    [JsonPropertyName("replaced")]
    public bool? Replaced { get; init; }

    [JsonPropertyName("moved")]
    public bool? Moved { get; init; }

    public ProjectFileMoveResult ToDomain(string fallbackSourcePath) => new(
        FromPath ?? fallbackSourcePath,
        ToPath,
        DisplayPath,
        Name,
        Replaced ?? false,
        Moved ?? false);
}

internal sealed record OpenEntryRequestDto(
    [property: JsonPropertyName("path")] string Path,
    [property: JsonPropertyName("mode")] string Mode);
