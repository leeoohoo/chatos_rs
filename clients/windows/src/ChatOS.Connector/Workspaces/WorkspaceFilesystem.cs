using System.Diagnostics;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Workspaces;

public sealed class WorkspaceFilesystem
{
    internal const long MaximumPreviewBytes = 2 * 1024 * 1024;
    internal const long MaximumSearchFileBytes = 2 * 1024 * 1024;
    internal const int MaximumSearchVisits = 20_000;
    internal static readonly TimeSpan MaximumSearchDuration = TimeSpan.FromSeconds(3);

    private static readonly HashSet<string> IgnoredSearchDirectories = new(
        [
            ".git", ".build", ".cache", ".next", ".idea", ".vscode",
            "node_modules", "DerivedData", "Pods", "target", "dist", "build", "vendor",
        ],
        StringComparer.OrdinalIgnoreCase);

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private readonly WorkspacePathGuard _paths;

    public WorkspaceFilesystem(ConnectorWorkspace workspace)
    {
        _paths = new WorkspacePathGuard(workspace.AbsoluteRoot);
    }

    public JsonElement List(string path, bool includeFiles)
    {
        var directory = _paths.ResolveExisting(path);
        if (!Directory.Exists(directory))
        {
            throw new Relay.RelayRequestException(400, "Workspace entry is not a directory.");
        }

        var entries = new List<object>();
        foreach (var entry in new DirectoryInfo(directory).EnumerateFileSystemInfos())
        {
            if ((entry.Attributes & FileAttributes.ReparsePoint) != 0)
            {
                continue;
            }

            var isDirectory = (entry.Attributes & FileAttributes.Directory) != 0;
            if (!includeFiles && !isDirectory)
            {
                continue;
            }

            entries.Add(Entry(entry, isDirectory));
        }

        entries.Sort((left, right) => CompareEntries(left, right));
        var relative = _paths.RelativePath(directory);
        var parent = relative == "."
            ? null
            : _paths.RelativePath(Directory.GetParent(directory)?.FullName ?? _paths.Root);
        return JsonSerializer.SerializeToElement(new { path = relative, parent, entries }, JsonOptions);
    }

    public JsonElement Read(string path)
    {
        var file = _paths.ResolveExisting(path, permitsRoot: false);
        if (!File.Exists(file))
        {
            throw new Relay.RelayRequestException(400, "Workspace entry is not a file.");
        }

        var info = new FileInfo(file);
        if (info.Length > MaximumPreviewBytes)
        {
            throw new Relay.RelayRequestException(
                413,
                $"File is too large to preview ({info.Length} bytes; maximum {MaximumPreviewBytes}).");
        }

        var data = File.ReadAllBytes(file);
        var binary = data.AsSpan(0, Math.Min(data.Length, 8_000)).Contains((byte)0);
        return JsonSerializer.SerializeToElement(new
        {
            path = _paths.RelativePath(file),
            size = info.Length,
            modified_at = Milliseconds(info.LastWriteTimeUtc),
            is_binary = binary,
            content = binary ? Convert.ToBase64String(data) : Encoding.UTF8.GetString(data),
        }, JsonOptions);
    }

    public JsonElement SearchEntries(
        string path,
        string query,
        int limit,
        CancellationToken cancellationToken)
    {
        var start = _paths.ResolveExisting(path);
        if (!Directory.Exists(start))
        {
            throw new Relay.RelayRequestException(400, "Workspace entry is not a directory.");
        }

        var needle = Required(query, "query");
        var maximum = Math.Clamp(limit, 1, 500);
        var stopwatch = Stopwatch.StartNew();
        var pending = new Stack<string>();
        pending.Push(start);
        var matches = new List<object>();
        var visitedDirectories = 0;
        var truncated = false;

        while (pending.TryPop(out var directory))
        {
            if (ShouldStop(cancellationToken, stopwatch, visitedDirectories))
            {
                truncated = true;
                break;
            }

            visitedDirectories++;
            foreach (var entry in SafeEntries(directory))
            {
                if (cancellationToken.IsCancellationRequested ||
                    stopwatch.Elapsed >= MaximumSearchDuration ||
                    matches.Count >= maximum)
                {
                    truncated = true;
                    break;
                }

                if ((entry.Attributes & FileAttributes.ReparsePoint) != 0)
                {
                    continue;
                }

                var isDirectory = (entry.Attributes & FileAttributes.Directory) != 0;
                if (isDirectory && !IgnoredSearchDirectories.Contains(entry.Name))
                {
                    pending.Push(entry.FullName);
                }

                var relative = _paths.RelativePath(entry.FullName);
                if (entry.Name.Contains(needle, StringComparison.CurrentCultureIgnoreCase) ||
                    relative.Contains(needle, StringComparison.CurrentCultureIgnoreCase))
                {
                    matches.Add(Entry(entry, isDirectory));
                }
            }

            if (truncated)
            {
                break;
            }
        }

        cancellationToken.ThrowIfCancellationRequested();
        matches.Sort((left, right) => string.Compare(
            JsonSerializer.SerializeToElement(left).GetProperty("path").GetString(),
            JsonSerializer.SerializeToElement(right).GetProperty("path").GetString(),
            StringComparison.CurrentCultureIgnoreCase));
        return JsonSerializer.SerializeToElement(new
        {
            matches,
            visited_dirs = visitedDirectories,
            truncated,
        }, JsonOptions);
    }

    public JsonElement SearchContent(
        string path,
        string query,
        int limit,
        CancellationToken cancellationToken)
    {
        var start = _paths.ResolveExisting(path);
        var needle = Required(query, "query");
        var maximum = Math.Clamp(limit, 1, 500);
        var stopwatch = Stopwatch.StartNew();
        var pending = new Stack<string>();
        pending.Push(start);
        var matches = new List<object>();
        var scannedFiles = 0;
        var visits = 0;
        var truncated = false;

        while (pending.TryPop(out var current))
        {
            if (ShouldStop(cancellationToken, stopwatch, visits))
            {
                truncated = true;
                break;
            }

            visits++;
            var attributes = File.GetAttributes(current);
            if ((attributes & FileAttributes.ReparsePoint) != 0)
            {
                continue;
            }

            if ((attributes & FileAttributes.Directory) != 0)
            {
                if (current != start && IgnoredSearchDirectories.Contains(Path.GetFileName(current)))
                {
                    continue;
                }

                foreach (var entry in SafeEntries(current))
                {
                    pending.Push(entry.FullName);
                }
                continue;
            }

            var info = new FileInfo(current);
            if (info.Length > MaximumSearchFileBytes)
            {
                continue;
            }

            byte[] data;
            try
            {
                data = File.ReadAllBytes(current);
            }
            catch (IOException)
            {
                continue;
            }
            catch (UnauthorizedAccessException)
            {
                continue;
            }

            if (data.AsSpan(0, Math.Min(data.Length, 8_000)).Contains((byte)0))
            {
                continue;
            }

            scannedFiles++;
            var lines = Encoding.UTF8.GetString(data).Split('\n');
            for (var index = 0; index < lines.Length; index++)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var column = lines[index].IndexOf(needle, StringComparison.CurrentCultureIgnoreCase);
                if (column < 0)
                {
                    continue;
                }

                matches.Add(new
                {
                    path = _paths.RelativePath(current),
                    line = index + 1,
                    column = column + 1,
                    text = lines[index][..Math.Min(lines[index].Length, 2_000)].TrimEnd('\r'),
                });
                if (matches.Count >= maximum)
                {
                    truncated = true;
                    break;
                }
            }

            if (truncated)
            {
                break;
            }
        }

        cancellationToken.ThrowIfCancellationRequested();
        return JsonSerializer.SerializeToElement(new { matches, scanned_files = scannedFiles, truncated }, JsonOptions);
    }

    public JsonElement CreateDirectory(string path)
    {
        var components = WorkspacePathGuard.NormalizeComponents(path, permitsRoot: false);
        var current = _paths.Root;
        foreach (var component in components)
        {
            var next = Path.Combine(current, component);
            if (File.Exists(next))
            {
                throw new Relay.RelayRequestException(400, "A file blocks the requested directory path.");
            }

            if (Directory.Exists(next))
            {
                _ = _paths.ResolveExisting(_paths.RelativePath(next), permitsRoot: false);
            }
            else
            {
                Directory.CreateDirectory(next);
            }

            current = next;
        }

        return JsonSerializer.SerializeToElement(new
        {
            path = _paths.RelativePath(current),
            created = true,
        }, JsonOptions);
    }

    public JsonElement Write(string path, string content, bool createOnly)
    {
        var target = _paths.ResolveWritable(path);
        var existed = File.Exists(target);
        if (Directory.Exists(target))
        {
            throw new Relay.RelayRequestException(400, "Workspace entry is not a file.");
        }

        if (createOnly && existed)
        {
            throw new Relay.RelayRequestException(409, "Workspace entry already exists.");
        }

        var bytes = Encoding.UTF8.GetBytes(content);
        if (createOnly)
        {
            using var stream = new FileStream(target, FileMode.CreateNew, FileAccess.Write, FileShare.None);
            stream.Write(bytes);
        }
        else
        {
            var temporary = Path.Combine(
                Path.GetDirectoryName(target)!,
                $".{Path.GetFileName(target)}.{Guid.NewGuid():N}.tmp");
            try
            {
                File.WriteAllBytes(temporary, bytes);
                File.Move(temporary, target, overwrite: true);
            }
            finally
            {
                if (File.Exists(temporary))
                {
                    File.Delete(temporary);
                }
            }
        }

        var info = new FileInfo(target);
        return JsonSerializer.SerializeToElement(new
        {
            path = _paths.RelativePath(target),
            size = info.Length,
            modified_at = Milliseconds(info.LastWriteTimeUtc),
            created = !existed,
        }, JsonOptions);
    }

    public JsonElement Delete(string path, bool recursive)
    {
        var target = _paths.ResolveMutationTarget(path);
        var isDirectory = Directory.Exists(target);
        try
        {
            if (isDirectory)
            {
                Directory.Delete(target, recursive);
            }
            else
            {
                File.Delete(target);
            }
        }
        catch (IOException exception) when (isDirectory && !recursive)
        {
            throw new Relay.RelayRequestException(409, $"Directory is not empty: {exception.Message}");
        }

        return JsonSerializer.SerializeToElement(new
        {
            path = _paths.RelativePath(target),
            is_dir = isDirectory,
            recursive,
            deleted = true,
        }, JsonOptions);
    }

    public JsonElement Move(string sourcePath, string targetPath, bool replaceExisting)
    {
        var source = _paths.ResolveMutationTarget(sourcePath);
        var target = _paths.ResolveWritable(targetPath);
        var isDirectory = Directory.Exists(source);
        if (string.Equals(source, target, OperatingSystem.IsWindows()
                ? StringComparison.OrdinalIgnoreCase
                : StringComparison.Ordinal))
        {
            return MoveResult(source, target, isDirectory, replaced: false, moved: false);
        }

        _paths.EnsureMoveTargetIsOutsideSource(source, target);
        var targetExists = File.Exists(target) || Directory.Exists(target);
        if (targetExists && !replaceExisting)
        {
            throw new Relay.RelayRequestException(409, "Move target already exists.");
        }

        if (targetExists && Directory.Exists(target) != isDirectory)
        {
            throw new Relay.RelayRequestException(409, "Move source and target types do not match.");
        }

        if (!targetExists)
        {
            MoveEntry(source, target, isDirectory);
            return MoveResult(source, target, isDirectory, replaced: false, moved: true);
        }

        var backup = Path.Combine(
            Path.GetDirectoryName(target)!,
            $".{Path.GetFileName(target)}.{Guid.NewGuid():N}.replace-backup");
        MoveEntry(target, backup, isDirectory);
        try
        {
            MoveEntry(source, target, isDirectory);
        }
        catch
        {
            MoveEntry(backup, target, isDirectory);
            throw;
        }

        if (isDirectory)
        {
            Directory.Delete(backup, recursive: true);
        }
        else
        {
            File.Delete(backup);
        }

        return MoveResult(source, target, isDirectory, replaced: true, moved: true);
    }

    private object Entry(FileSystemInfo entry, bool isDirectory) => new
    {
        name = entry.Name,
        path = _paths.RelativePath(entry.FullName),
        is_dir = isDirectory,
        size = entry is FileInfo file ? file.Length : 0,
        modified_at = Milliseconds(entry.LastWriteTimeUtc),
    };

    private static int CompareEntries(object left, object right)
    {
        var leftJson = JsonSerializer.SerializeToElement(left);
        var rightJson = JsonSerializer.SerializeToElement(right);
        var directoryComparison = rightJson.GetProperty("is_dir").GetBoolean()
            .CompareTo(leftJson.GetProperty("is_dir").GetBoolean());
        return directoryComparison != 0
            ? directoryComparison
            : string.Compare(
                leftJson.GetProperty("name").GetString(),
                rightJson.GetProperty("name").GetString(),
                StringComparison.CurrentCultureIgnoreCase);
    }

    private JsonElement MoveResult(
        string source,
        string target,
        bool isDirectory,
        bool replaced,
        bool moved) => JsonSerializer.SerializeToElement(new
        {
            from_path = _paths.RelativePath(source),
            to_path = _paths.RelativePath(target),
            name = Path.GetFileName(target),
            is_dir = isDirectory,
            replaced,
            moved,
        }, JsonOptions);

    private static void MoveEntry(string source, string target, bool isDirectory)
    {
        if (isDirectory)
        {
            Directory.Move(source, target);
        }
        else
        {
            File.Move(source, target);
        }
    }

    private static IReadOnlyList<FileSystemInfo> SafeEntries(string directory)
    {
        try
        {
            return new DirectoryInfo(directory).EnumerateFileSystemInfos().ToArray();
        }
        catch (IOException)
        {
            return Array.Empty<FileSystemInfo>();
        }
        catch (UnauthorizedAccessException)
        {
            return Array.Empty<FileSystemInfo>();
        }
    }

    private static bool ShouldStop(
        CancellationToken cancellationToken,
        Stopwatch stopwatch,
        int visits) =>
        cancellationToken.IsCancellationRequested ||
        stopwatch.Elapsed >= MaximumSearchDuration ||
        visits >= MaximumSearchVisits;

    private static string Required(string? value, string field) =>
        !string.IsNullOrWhiteSpace(value)
            ? value.Trim()
            : throw new Relay.RelayRequestException(400, $"Filesystem request is missing {field}.");

    private static long Milliseconds(DateTime value) =>
        new DateTimeOffset(value.ToUniversalTime()).ToUnixTimeMilliseconds();
}
