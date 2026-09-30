using System.Diagnostics;
using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Workspaces;

public sealed class WindowsProjectFilesystemService(
    ILocalProjectPathResolver paths) : IProjectFilesystemService
{
    public Task<ProjectDirectoryListing> ListEntriesAsync(
        string path,
        bool forceRefresh = false,
        CancellationToken cancellationToken = default) => RunAsync(path, (resolved, filesystem) =>
    {
        var value = filesystem.List(resolved.RelativePath, includeFiles: true);
        var relative = value.GetProperty("path").GetString() ?? resolved.RelativePath;
        var parent = value.GetProperty("parent").ValueKind == JsonValueKind.String
            ? value.GetProperty("parent").GetString()
            : null;
        var entries = value.GetProperty("entries").EnumerateArray()
            .Select(item => Entry(resolved, item))
            .ToArray();
        return new ProjectDirectoryListing(
            resolved.LogicalPath(relative),
            parent is null ? null : resolved.LogicalPath(parent),
            true,
            entries,
            false);
    }, cancellationToken);

    public Task<IReadOnlyList<ProjectFileEntry>> SearchEntriesAsync(
        string path,
        string query,
        int limit = 100,
        CancellationToken cancellationToken = default) => RunAsync<IReadOnlyList<ProjectFileEntry>>(
        path,
        (resolved, filesystem) => filesystem.SearchEntries(
                resolved.RelativePath,
                query,
                limit,
                cancellationToken)
            .GetProperty("matches")
            .EnumerateArray()
            .Select(item => Entry(resolved, item))
            .ToArray(),
        cancellationToken);

    public Task<IReadOnlyList<ProjectFileContentMatch>> SearchContentAsync(
        string path,
        string query,
        int limit = 100,
        CancellationToken cancellationToken = default) => RunAsync<IReadOnlyList<ProjectFileContentMatch>>(
        path,
        (resolved, filesystem) => filesystem.SearchContent(
                resolved.RelativePath,
                query,
                limit,
                cancellationToken)
            .GetProperty("matches")
            .EnumerateArray()
            .Select(item =>
            {
                var relative = RequiredString(item, "path");
                return new ProjectFileContentMatch(
                    resolved.LogicalPath(relative),
                    DisplayPath(relative),
                    Math.Max(1, item.GetProperty("line").GetInt32()),
                    Math.Max(1, item.GetProperty("column").GetInt32()),
                    item.GetProperty("text").GetString() ?? string.Empty);
            })
            .ToArray(),
        cancellationToken);

    public Task<ProjectFileContent> ReadFileAsync(
        string path,
        CancellationToken cancellationToken = default) => RunAsync(path, (resolved, filesystem) =>
    {
        var value = filesystem.Read(resolved.RelativePath);
        var relative = RequiredString(value, "path");
        return new ProjectFileContent(
            resolved.LogicalPath(relative),
            DisplayPath(relative),
            Path.GetFileName(relative),
            ContentType(relative),
            value.GetProperty("is_binary").GetBoolean(),
            true,
            value.GetProperty("size").GetInt64(),
            UnixDate(value, "modified_at"),
            value.GetProperty("content").GetString() ?? string.Empty);
    }, cancellationToken);

    public Task WriteFileAsync(
        string path,
        string content,
        CancellationToken cancellationToken = default) => RunAsync<object?>(path, (resolved, filesystem) =>
    {
        _ = filesystem.Write(resolved.RelativePath, content, createOnly: false);
        return null;
    }, cancellationToken);

    public Task CreateFileAsync(
        string parentPath,
        string name,
        CancellationToken cancellationToken = default) => RunAsync<object?>(parentPath, (resolved, filesystem) =>
    {
        _ = filesystem.Write(ChildPath(resolved.RelativePath, name), string.Empty, createOnly: true);
        return null;
    }, cancellationToken);

    public Task CreateDirectoryAsync(
        string parentPath,
        string name,
        CancellationToken cancellationToken = default) => RunAsync<object?>(parentPath, (resolved, filesystem) =>
    {
        _ = filesystem.CreateDirectory(ChildPath(resolved.RelativePath, name));
        return null;
    }, cancellationToken);

    public Task DeleteEntryAsync(
        string path,
        bool recursive,
        CancellationToken cancellationToken = default) => RunAsync<object?>(path, (resolved, filesystem) =>
    {
        _ = filesystem.Delete(resolved.RelativePath, recursive);
        return null;
    }, cancellationToken);

    public async Task<ProjectFileMoveResult> MoveEntryAsync(
        string sourcePath,
        string targetParentPath,
        string? targetName = null,
        bool replaceExisting = false,
        CancellationToken cancellationToken = default)
    {
        var source = paths.Resolve(sourcePath);
        var targetParent = paths.Resolve(targetParentPath);
        if (!string.Equals(source.Workspace.Id, targetParent.Workspace.Id, StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Project entries cannot be moved between workspaces.");
        }
        return await Task.Run(() =>
        {
            cancellationToken.ThrowIfCancellationRequested();
            var name = string.IsNullOrWhiteSpace(targetName)
                ? Path.GetFileName(source.RelativePath)
                : targetName.Trim();
            var target = ChildPath(targetParent.RelativePath, name);
            var value = new WorkspaceFilesystem(source.Workspace)
                .Move(source.RelativePath, target, replaceExisting);
            var from = RequiredString(value, "from_path");
            var to = RequiredString(value, "to_path");
            return new ProjectFileMoveResult(
                source.LogicalPath(from),
                source.LogicalPath(to),
                DisplayPath(to),
                value.GetProperty("name").GetString(),
                value.GetProperty("replaced").GetBoolean(),
                value.GetProperty("moved").GetBoolean());
        }, cancellationToken).ConfigureAwait(false);
    }

    public Task OpenExternallyAsync(
        string path,
        ProjectFileExternalOpenMode mode,
        CancellationToken cancellationToken = default)
    {
        var resolved = paths.Resolve(path);
        cancellationToken.ThrowIfCancellationRequested();
        var start = mode switch
        {
            ProjectFileExternalOpenMode.Reveal => Command(
                "explorer.exe",
                $"/select,{resolved.AbsolutePath}"),
            ProjectFileExternalOpenMode.Code => Command("code", resolved.AbsolutePath),
            _ => new ProcessStartInfo(resolved.AbsolutePath) { UseShellExecute = true },
        };
        _ = Process.Start(start) ?? throw new InvalidOperationException(
            "The operating system did not start an application for the project entry.");
        return Task.CompletedTask;
    }

    private Task<T> RunAsync<T>(
        string path,
        Func<ResolvedLocalProjectPath, WorkspaceFilesystem, T> operation,
        CancellationToken cancellationToken)
    {
        var resolved = paths.Resolve(path);
        return Task.Run(() =>
        {
            cancellationToken.ThrowIfCancellationRequested();
            return operation(resolved, new WorkspaceFilesystem(resolved.Workspace));
        }, cancellationToken);
    }

    private static ProjectFileEntry Entry(ResolvedLocalProjectPath resolved, JsonElement item)
    {
        var relative = RequiredString(item, "path");
        var isDirectory = item.GetProperty("is_dir").GetBoolean();
        return new ProjectFileEntry(
            item.GetProperty("name").GetString() ?? Path.GetFileName(relative),
            resolved.LogicalPath(relative),
            DisplayPath(relative),
            isDirectory,
            true,
            isDirectory ? null : item.GetProperty("size").GetInt64(),
            UnixDate(item, "modified_at"));
    }

    private static ProcessStartInfo Command(string executable, params string[] arguments)
    {
        var start = new ProcessStartInfo(executable) { UseShellExecute = false };
        foreach (var argument in arguments) start.ArgumentList.Add(argument);
        return start;
    }

    private static string ChildPath(string parent, string name) =>
        parent == "." ? name : $"{parent}/{name}";

    private static string RequiredString(JsonElement value, string name) =>
        value.GetProperty(name).GetString()
        ?? throw new InvalidDataException($"Local workspace response is missing {name}.");

    private static DateTimeOffset? UnixDate(JsonElement value, string name) =>
        value.TryGetProperty(name, out var timestamp) && timestamp.TryGetInt64(out var milliseconds)
            ? DateTimeOffset.FromUnixTimeMilliseconds(milliseconds)
            : null;

    private static string DisplayPath(string relative) =>
        relative == "." ? "/" : "/" + relative.Replace('\\', '/');

    private static string? ContentType(string path) => Path.GetExtension(path).ToLowerInvariant() switch
    {
        ".cs" => "text/x-csharp",
        ".css" => "text/css",
        ".html" or ".htm" => "text/html",
        ".js" or ".mjs" or ".cjs" => "text/javascript",
        ".json" => "application/json",
        ".md" => "text/markdown",
        ".rs" => "text/x-rust",
        ".swift" => "text/x-swift",
        ".ts" or ".tsx" => "text/typescript",
        ".xml" => "application/xml",
        ".yaml" or ".yml" => "application/yaml",
        ".txt" => "text/plain",
        _ => null,
    };
}
