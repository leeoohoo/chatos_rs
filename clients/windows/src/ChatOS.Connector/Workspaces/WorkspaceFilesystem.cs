using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Workspaces;

public sealed class WorkspaceFilesystem
{
    internal const long MaximumPreviewBytes = 2 * 1024 * 1024;
    internal const long MaximumSearchFileBytes = 2 * 1024 * 1024;
    internal const int MaximumSearchVisits = 20_000;
    internal const int MaximumWriteDiffCharacters = 64 * 1024;
    internal static readonly TimeSpan MaximumSearchDuration = TimeSpan.FromSeconds(3);

    private static readonly HashSet<string> IgnoredSearchDirectories = new(
        [
            ".git", ".build", ".cache", ".next", ".idea", ".vscode",
            "node_modules", "DerivedData", "Pods", "target", "dist", "build", "vendor",
        ],
        StringComparer.OrdinalIgnoreCase);

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private static readonly UTF8Encoding StrictUtf8 = new(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);
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
            throw new WorkspaceRequestException(
                400, "workspace_not_directory", "Workspace entry is not a directory.");
        }

        var entries = new List<object>();
        var omittedReparsePoints = 0;
        foreach (var entry in new DirectoryInfo(directory).EnumerateFileSystemInfos())
        {
            if ((entry.Attributes & FileAttributes.ReparsePoint) != 0)
            {
                omittedReparsePoints++;
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
        return JsonSerializer.SerializeToElement(new
        {
            path = relative,
            parent,
            entries,
            incomplete = omittedReparsePoints > 0,
            omitted_reparse_points = omittedReparsePoints,
        }, JsonOptions);
    }

    public JsonElement Read(string path)
    {
        var file = _paths.ResolveExisting(path, permitsRoot: false);
        if (!File.Exists(file))
        {
            throw new WorkspaceRequestException(
                400, "workspace_not_file", "Workspace entry is not a file.");
        }

        var info = new FileInfo(file);
        if (info.Length > MaximumPreviewBytes)
        {
            throw new WorkspaceRequestException(
                413,
                "workspace_file_too_large",
                $"File is too large to preview ({info.Length} bytes; maximum {MaximumPreviewBytes}).");
        }

        var data = File.ReadAllBytes(file);
        var text = string.Empty;
        var binary = data.AsSpan(0, Math.Min(data.Length, 8_000)).Contains((byte)0);
        if (!binary)
        {
            binary = !TryDecodeUtf8(data, out text);
        }
        return JsonSerializer.SerializeToElement(new
        {
            path = _paths.RelativePath(file),
            size = info.Length,
            modified_at = Milliseconds(info.LastWriteTimeUtc),
            is_binary = binary,
            content = binary ? Convert.ToBase64String(data) : text,
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
            throw new WorkspaceRequestException(
                400, "workspace_not_directory", "Workspace entry is not a directory.");
        }

        var needle = Required(query, "query");
        var maximum = Math.Clamp(limit, 1, 500);
        var stopwatch = Stopwatch.StartNew();
        var pending = new Stack<string>();
        pending.Push(start);
        var matches = new List<object>();
        var visitedDirectories = 0;
        var truncated = false;
        var incomplete = false;

        while (pending.TryPop(out var directory))
        {
            if (ShouldStop(cancellationToken, stopwatch, visitedDirectories))
            {
                truncated = true;
                break;
            }

            visitedDirectories++;
            if (!TryEnumerateEntries(directory, out var entries))
            {
                incomplete = true;
                continue;
            }

            foreach (var entry in entries)
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
                    incomplete = true;
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
            truncated = truncated || incomplete,
            incomplete,
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
        var incomplete = false;

        while (pending.TryPop(out var current))
        {
            if (ShouldStop(cancellationToken, stopwatch, visits))
            {
                truncated = true;
                break;
            }

            visits++;
            FileAttributes attributes;
            try
            {
                attributes = File.GetAttributes(current);
            }
            catch (IOException)
            {
                incomplete = true;
                continue;
            }
            catch (UnauthorizedAccessException)
            {
                incomplete = true;
                continue;
            }
            if ((attributes & FileAttributes.ReparsePoint) != 0)
            {
                incomplete = true;
                continue;
            }

            if ((attributes & FileAttributes.Directory) != 0)
            {
                if (current != start && IgnoredSearchDirectories.Contains(Path.GetFileName(current)))
                {
                    continue;
                }

                if (!TryEnumerateEntries(current, out var entries))
                {
                    incomplete = true;
                    continue;
                }

                foreach (var entry in entries)
                {
                    pending.Push(entry.FullName);
                }
                continue;
            }

            long length;
            try
            {
                length = new FileInfo(current).Length;
            }
            catch (IOException)
            {
                incomplete = true;
                continue;
            }
            catch (UnauthorizedAccessException)
            {
                incomplete = true;
                continue;
            }
            if (length > MaximumSearchFileBytes)
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
                incomplete = true;
                continue;
            }
            catch (UnauthorizedAccessException)
            {
                incomplete = true;
                continue;
            }

            if (data.AsSpan(0, Math.Min(data.Length, 8_000)).Contains((byte)0) ||
                !TryDecodeUtf8(data, out var text))
            {
                continue;
            }

            scannedFiles++;
            var lines = text.Split('\n');
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
        return JsonSerializer.SerializeToElement(new
        {
            matches,
            scanned_files = scannedFiles,
            truncated = truncated || incomplete,
            incomplete,
        }, JsonOptions);
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
                throw new WorkspaceRequestException(
                    400,
                    "workspace_file_blocks_directory",
                    "A file blocks the requested directory path.");
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
            throw new WorkspaceRequestException(
                400, "workspace_not_file", "Workspace entry is not a file.");
        }

        if (createOnly && existed)
        {
            throw new WorkspaceRequestException(
                409, "workspace_entry_exists", "Workspace entry already exists.");
        }

        var previous = CapturePreviousText(target, existed);
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
        var relativePath = _paths.RelativePath(target);
        var change = BuildWriteChange(relativePath, previous, content, bytes);
        return JsonSerializer.SerializeToElement(new
        {
            path = relativePath,
            size = info.Length,
            modified_at = Milliseconds(info.LastWriteTimeUtc),
            created = !existed,
            change,
        }, JsonOptions);
    }

    private static WritePreviousText CapturePreviousText(string target, bool existed)
    {
        if (!existed)
        {
            return new WritePreviousText(string.Empty, [], null);
        }

        try
        {
            var info = new FileInfo(target);
            if (info.Length > MaximumPreviewBytes)
            {
                return new WritePreviousText(null, null, "previous_file_too_large");
            }

            var bytes = File.ReadAllBytes(target);
            try
            {
                return new WritePreviousText(StrictUtf8.GetString(bytes), bytes, null);
            }
            catch (DecoderFallbackException)
            {
                return new WritePreviousText(null, bytes, "previous_file_not_utf8");
            }
        }
        catch (IOException)
        {
            return new WritePreviousText(null, null, "previous_file_unavailable");
        }
        catch (UnauthorizedAccessException)
        {
            return new WritePreviousText(null, null, "previous_file_unavailable");
        }
    }

    private static object BuildWriteChange(
        string relativePath,
        WritePreviousText previous,
        string content,
        byte[] bytes)
    {
        var beforeSha256 = previous.Bytes is null
            ? null
            : Convert.ToHexString(SHA256.HashData(previous.Bytes)).ToLowerInvariant();
        var afterSha256 = Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();
        if (previous.Text is null)
        {
            return new
            {
                changed = true,
                before_sha256 = beforeSha256,
                after_sha256 = afterSha256,
                diff_available = false,
                diff_truncated = false,
                unified_diff = (string?)null,
                unavailable_reason = previous.UnavailableReason,
            };
        }

        var changed = !string.Equals(previous.Text, content, StringComparison.Ordinal);
        var diff = changed
            ? UnifiedReplacementDiff(relativePath, previous.Text, content)
            : string.Empty;
        var truncated = diff.Length > MaximumWriteDiffCharacters;
        if (truncated)
        {
            var length = MaximumWriteDiffCharacters;
            if (length > 0 && char.IsHighSurrogate(diff[length - 1]))
            {
                length--;
            }
            diff = diff[..length];
        }

        return new
        {
            changed,
            before_sha256 = beforeSha256,
            after_sha256 = afterSha256,
            diff_available = true,
            diff_truncated = truncated,
            unified_diff = diff,
            unavailable_reason = (string?)null,
        };
    }

    private static string UnifiedReplacementDiff(string path, string before, string after)
    {
        var beforeLines = DiffLines(before);
        var afterLines = DiffLines(after);
        var output = new StringBuilder();
        output.Append("--- ").Append(before.Length == 0 ? "/dev/null" : $"a/{path}").Append('\n');
        output.Append("+++ b/").Append(path).Append('\n');
        output.Append("@@ -")
            .Append(beforeLines.Count == 0 ? "0,0" : $"1,{beforeLines.Count}")
            .Append(" +")
            .Append(afterLines.Count == 0 ? "0,0" : $"1,{afterLines.Count}")
            .Append(" @@\n");
        foreach (var line in beforeLines)
        {
            output.Append('-').Append(line).Append('\n');
        }
        foreach (var line in afterLines)
        {
            output.Append('+').Append(line).Append('\n');
        }
        return output.ToString();
    }

    private static IReadOnlyList<string> DiffLines(string content)
    {
        if (content.Length == 0)
        {
            return [];
        }

        var normalized = content.Replace("\r\n", "\n", StringComparison.Ordinal).Replace('\r', '\n');
        var lines = normalized.Split('\n').ToList();
        if (normalized.EndsWith('\n'))
        {
            lines.RemoveAt(lines.Count - 1);
        }
        return lines;
    }

    private sealed record WritePreviousText(
        string? Text,
        byte[]? Bytes,
        string? UnavailableReason);

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
        catch (IOException) when (isDirectory && !recursive)
        {
            throw new WorkspaceRequestException(
                409, "workspace_directory_not_empty", "Directory is not empty.");
        }

        return JsonSerializer.SerializeToElement(new
        {
            path = _paths.RelativePath(target),
            is_dir = isDirectory,
            recursive,
            deleted = true,
        }, JsonOptions);
    }

    public JsonElement Copy(
        string sourcePath,
        string targetPath,
        bool replaceExisting,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var source = _paths.ResolveMutationTarget(sourcePath);
        var target = _paths.ResolveWritable(targetPath);
        var isDirectory = Directory.Exists(source);
        if (string.Equals(source, target, OperatingSystem.IsWindows()
                ? StringComparison.OrdinalIgnoreCase
                : StringComparison.Ordinal))
        {
            throw new WorkspaceRequestException(
                400, "workspace_copy_self", "An entry cannot be copied onto itself.");
        }

        _paths.EnsureCopyTargetIsOutsideSource(source, target);
        var targetExists = File.Exists(target) || Directory.Exists(target);
        if (targetExists && !replaceExisting)
        {
            throw new WorkspaceRequestException(
                409, "workspace_copy_target_exists", "Copy target already exists.");
        }
        if (targetExists && Directory.Exists(target) != isDirectory)
        {
            throw new WorkspaceRequestException(
                409,
                "workspace_copy_type_mismatch",
                "Copy source and target types do not match.");
        }

        var parent = Path.GetDirectoryName(target)!;
        var staging = Path.Combine(
            parent,
            $".{Path.GetFileName(target)}.{Guid.NewGuid():N}.copy-staging");
        string? backup = null;
        try
        {
            CopyEntry(source, staging, isDirectory, cancellationToken);
            cancellationToken.ThrowIfCancellationRequested();
            if (targetExists)
            {
                backup = Path.Combine(
                    parent,
                    $".{Path.GetFileName(target)}.{Guid.NewGuid():N}.replace-backup");
                MoveEntry(target, backup, isDirectory);
            }

            try
            {
                MoveEntry(staging, target, isDirectory);
            }
            catch
            {
                if (backup is not null)
                {
                    MoveEntry(backup, target, isDirectory);
                    backup = null;
                }
                throw;
            }

            if (backup is not null)
            {
                DeleteEntry(backup, isDirectory);
                backup = null;
            }
        }
        finally
        {
            DeleteEntryIfPresent(staging);
            if (backup is not null && !File.Exists(target) && !Directory.Exists(target))
            {
                MoveEntry(backup, target, isDirectory);
                backup = null;
            }
        }

        return CopyResult(source, target, isDirectory, targetExists);
    }

    public JsonElement Move(string sourcePath, string targetPath, bool replaceExisting)
    {
        var source = _paths.ResolveMutationTarget(sourcePath);
        var target = _paths.ResolveWritable(targetPath);
        var isDirectory = Directory.Exists(source);
        if (string.Equals(source, target, StringComparison.Ordinal))
        {
            return MoveResult(source, target, isDirectory, replaced: false, moved: false);
        }

        if (OperatingSystem.IsWindows() &&
            string.Equals(source, target, StringComparison.OrdinalIgnoreCase))
        {
            var sourceName = isDirectory
                ? new DirectoryInfo(source).Name
                : new FileInfo(source).Name;
            if (string.Equals(sourceName, Path.GetFileName(target), StringComparison.Ordinal))
            {
                return MoveResult(source, target, isDirectory, replaced: false, moved: false);
            }

            MoveEntryChangingCase(source, target, isDirectory);
            return MoveResult(source, target, isDirectory, replaced: false, moved: true);
        }

        _paths.EnsureMoveTargetIsOutsideSource(source, target);
        var targetExists = File.Exists(target) || Directory.Exists(target);
        if (targetExists && !replaceExisting)
        {
            throw new WorkspaceRequestException(
                409, "workspace_move_target_exists", "Move target already exists.");
        }

        if (targetExists && Directory.Exists(target) != isDirectory)
        {
            throw new WorkspaceRequestException(
                409,
                "workspace_move_type_mismatch",
                "Move source and target types do not match.");
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

    private JsonElement CopyResult(
        string source,
        string target,
        bool isDirectory,
        bool replaced) => JsonSerializer.SerializeToElement(new
        {
            from_path = _paths.RelativePath(source),
            to_path = _paths.RelativePath(target),
            name = Path.GetFileName(target),
            is_dir = isDirectory,
            replaced,
            copied = true,
        }, JsonOptions);

    private static void CopyEntry(
        string source,
        string target,
        bool isDirectory,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (!isDirectory)
        {
            File.Copy(source, target, overwrite: false);
            return;
        }

        Directory.CreateDirectory(target);
        foreach (var entry in new DirectoryInfo(source).EnumerateFileSystemInfos())
        {
            cancellationToken.ThrowIfCancellationRequested();
            if ((entry.Attributes & FileAttributes.ReparsePoint) != 0)
            {
                throw new WorkspaceRequestException(
                    400,
                    "workspace_copy_reparse_point",
                    "Workspace operations cannot copy a symbolic link or junction.");
            }

            var childTarget = Path.Combine(target, entry.Name);
            var childIsDirectory = (entry.Attributes & FileAttributes.Directory) != 0;
            CopyEntry(entry.FullName, childTarget, childIsDirectory, cancellationToken);
        }
    }

    private static void DeleteEntry(string path, bool isDirectory)
    {
        if (isDirectory)
        {
            Directory.Delete(path, recursive: true);
        }
        else
        {
            File.Delete(path);
        }
    }

    private static void DeleteEntryIfPresent(string path)
    {
        if (Directory.Exists(path))
        {
            Directory.Delete(path, recursive: true);
        }
        else if (File.Exists(path))
        {
            File.Delete(path);
        }
    }

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

    private static void MoveEntryChangingCase(string source, string target, bool isDirectory)
    {
        var temporary = Path.Combine(
            Path.GetDirectoryName(source)!,
            $".{Path.GetFileName(source)}.{Guid.NewGuid():N}.case-rename");
        MoveEntry(source, temporary, isDirectory);
        try
        {
            MoveEntry(temporary, target, isDirectory);
        }
        catch
        {
            if (!File.Exists(source) && !Directory.Exists(source) &&
                (File.Exists(temporary) || Directory.Exists(temporary)))
            {
                MoveEntry(temporary, source, isDirectory);
            }
            throw;
        }
    }

    internal static bool TryEnumerateEntries(
        string directory,
        out IReadOnlyList<FileSystemInfo> entries)
    {
        try
        {
            entries = new DirectoryInfo(directory).EnumerateFileSystemInfos().ToArray();
            return true;
        }
        catch (IOException)
        {
            entries = Array.Empty<FileSystemInfo>();
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            entries = Array.Empty<FileSystemInfo>();
            return false;
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
            : throw new WorkspaceRequestException(
                400,
                "workspace_argument_missing",
                $"Filesystem request is missing {field}.");

    private static long Milliseconds(DateTime value) =>
        new DateTimeOffset(value.ToUniversalTime()).ToUnixTimeMilliseconds();

    private static bool TryDecodeUtf8(byte[] data, out string text)
    {
        try
        {
            text = StrictUtf8.GetString(data);
            return true;
        }
        catch (DecoderFallbackException)
        {
            text = string.Empty;
            return false;
        }
    }
}
