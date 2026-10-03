using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed class StoryProjectStore
{
    private const int MaximumManifestBytes = 16 * 1024 * 1024;
    private const int MaximumImageBytes = 20 * 1024 * 1024;
    private const int MaximumVideoBytes = 512 * 1024 * 1024;
    internal const int DefaultProjectPageSize = 40;
    internal const int DefaultPlanningRunPageSize = 20;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true,
    };
    private readonly string _root;

    public StoryProjectStore(string? root = null)
    {
        _root = root ?? Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ChatOS",
            "story-studio");
    }

    public async Task<IReadOnlyList<StoryProjectDocument>> LoadAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
        => (await LoadPageAsync(ownerUserId, null, DefaultProjectPageSize, cancellationToken)
            .ConfigureAwait(false)).Items;

    internal async Task<StoryStorePage<StoryProjectDocument>> LoadPageAsync(
        string ownerUserId,
        StoryStoreCursor? after,
        int limit = DefaultProjectPageSize,
        CancellationToken cancellationToken = default)
    {
        var ownerFolder = OwnerFolder(ownerUserId);
        if (!Directory.Exists(ownerFolder)) return new([], null);
        var page = ProjectCandidates(ownerFolder, after, limit, cancellationToken);
        var projects = new List<StoryProjectDocument>();
        foreach (var candidate in page.Candidates)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var folder = Path.GetDirectoryName(candidate.Path)!;
            if (!Guid.TryParse(Path.GetFileName(folder), out var folderId)) continue;
            var manifestPath = candidate.Path;
            try
            {
                var info = new FileInfo(manifestPath);
                if (!info.Exists || info.Length is <= 0 or > MaximumManifestBytes) continue;
                await using var stream = File.OpenRead(manifestPath);
                var project = await JsonSerializer.DeserializeAsync<StoryProjectDocument>(
                    stream,
                    JsonOptions,
                    cancellationToken).ConfigureAwait(false);
                if (project is null || project.Id != folderId) continue;
                project.Validate();
                projects.Add(project);
            }
            catch (Exception exception) when (exception is IOException or JsonException or UnauthorizedAccessException or InvalidDataException)
            {
                // Keep unreadable projects untouched so a later version can recover them.
            }
        }
        return new(
            projects.OrderByDescending(project => project.UpdatedAt).ToArray(),
            page.HasMore ? page.Candidates[^1].Cursor : null);
    }

    public async Task SaveAsync(
        string ownerUserId,
        StoryProjectDocument project,
        CancellationToken cancellationToken = default)
    {
        project.Validate();
        var folder = ProjectFolder(ownerUserId, project.Id);
        Directory.CreateDirectory(folder);
        var json = JsonSerializer.Serialize(project, JsonOptions);
        if (Encoding.UTF8.GetByteCount(json) > MaximumManifestBytes)
            throw new InvalidDataException("剧情项目文件超过 16 MB。请缩短原文或分段内容。");
        var temporaryPath = Path.Combine(folder, $"project-{Guid.NewGuid():N}.tmp");
        await File.WriteAllTextAsync(temporaryPath, json, cancellationToken).ConfigureAwait(false);
        File.Move(temporaryPath, Path.Combine(folder, "project.json"), true);
    }

    public async Task<string> ImportAssetAsync(
        string ownerUserId,
        Guid projectId,
        string segmentId,
        string sourcePath,
        bool video,
        CancellationToken cancellationToken = default)
    {
        var source = new FileInfo(sourcePath);
        var maximumBytes = video ? MaximumVideoBytes : MaximumImageBytes;
        if (!source.Exists || source.Length is <= 0 || source.Length > maximumBytes)
            throw new InvalidDataException(video ? "视频文件为空或超过 512 MB。" : "图片文件为空或超过 20 MB。");
        var extension = source.Extension.ToLowerInvariant();
        var allowed = video
            ? extension is ".mp4" or ".mov"
            : extension is ".png" or ".jpg" or ".jpeg" or ".webp";
        if (!allowed) throw new InvalidDataException(video ? "剧情视频必须是 MP4 或 MOV。" : "剧情图片必须是 PNG、JPEG 或 WebP。");

        var safeSegment = SafeSegment(segmentId);
        var relative = Path.Combine("assets", safeSegment, $"{(video ? "video" : "frame")}-{Guid.NewGuid():N}{extension}");
        var destination = ResolveAssetPath(ownerUserId, projectId, relative)
            ?? throw new InvalidDataException("剧情素材目标路径无效。");
        Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
        await using var input = new FileStream(source.FullName, FileMode.Open, FileAccess.Read, FileShare.Read, 81920, true);
        await using var output = new FileStream(destination, FileMode.CreateNew, FileAccess.Write, FileShare.None, 81920, true);
        await input.CopyToAsync(output, cancellationToken).ConfigureAwait(false);
        return relative;
    }

    public async Task<string> ImportFrameBytesAsync(
        string ownerUserId,
        Guid projectId,
        string segmentId,
        byte[] bytes,
        string mimeType,
        CancellationToken cancellationToken = default)
    {
        if (bytes.Length is <= 0 or > MaximumImageBytes)
            throw new InvalidDataException("提取的成片末帧为空或超过 20 MB。");
        var extension = mimeType.Equals("image/png", StringComparison.OrdinalIgnoreCase) ? ".png" : ".jpg";
        var relative = Path.Combine(
            "assets", SafeSegment(segmentId), $"video-final-frame-{Guid.NewGuid():N}{extension}");
        var destination = ResolveAssetPath(ownerUserId, projectId, relative)
            ?? throw new InvalidDataException("成片末帧目标路径无效。");
        Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
        await File.WriteAllBytesAsync(destination, bytes, cancellationToken).ConfigureAwait(false);
        return relative;
    }

    public async Task<IReadOnlyList<StoryPlanningRunDocument>> LoadPlanningRunsAsync(
        string ownerUserId,
        Guid projectId,
        CancellationToken cancellationToken = default)
        => (await LoadPlanningRunsPageAsync(
            ownerUserId, projectId, null, DefaultPlanningRunPageSize, cancellationToken)
            .ConfigureAwait(false)).Items;

    internal async Task<StoryStorePage<StoryPlanningRunDocument>> LoadPlanningRunsPageAsync(
        string ownerUserId,
        Guid projectId,
        StoryStoreCursor? after,
        int limit = DefaultPlanningRunPageSize,
        CancellationToken cancellationToken = default)
    {
        var folder = Path.Combine(ProjectFolder(ownerUserId, projectId), "planning-runs");
        if (!Directory.Exists(folder)) return new([], null);
        var page = FileCandidates(folder, "*.json", after, limit, cancellationToken);
        var runs = new List<StoryPlanningRunDocument>();
        foreach (var candidate in page.Candidates)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var path = candidate.Path;
            try
            {
                var info = new FileInfo(path);
                if (info.Length is <= 0 or > MaximumManifestBytes) continue;
                await using var stream = File.OpenRead(path);
                var run = await JsonSerializer.DeserializeAsync<StoryPlanningRunDocument>(
                    stream, JsonOptions, cancellationToken).ConfigureAwait(false);
                if (run is null || run.ProjectId != projectId) continue;
                run.Validate();
                runs.Add(run);
            }
            catch (Exception exception) when (exception is IOException or JsonException or
                UnauthorizedAccessException or InvalidDataException)
            {
                // Preserve unreadable records for a later compatible version.
            }
        }
        return new(
            runs.OrderByDescending(run => run.UpdatedAt).ToArray(),
            page.HasMore ? page.Candidates[^1].Cursor : null);
    }

    public async Task SavePlanningRunAsync(
        string ownerUserId,
        StoryPlanningRunDocument run,
        CancellationToken cancellationToken = default)
    {
        run.Validate();
        var folder = Path.Combine(ProjectFolder(ownerUserId, run.ProjectId), "planning-runs");
        Directory.CreateDirectory(folder);
        var json = JsonSerializer.Serialize(run, JsonOptions);
        if (Encoding.UTF8.GetByteCount(json) > MaximumManifestBytes)
            throw new InvalidDataException("剧情规划运行记录超过 16 MB。");
        var target = Path.Combine(folder, $"{run.Id:N}.json");
        var temporary = $"{target}.{Guid.NewGuid():N}.tmp";
        await File.WriteAllTextAsync(temporary, json, cancellationToken).ConfigureAwait(false);
        File.Move(temporary, target, true);
    }

    public string? ResolveAssetPath(string ownerUserId, Guid projectId, string? relativePath)
    {
        if (string.IsNullOrWhiteSpace(relativePath) || Path.IsPathFullyQualified(relativePath)) return null;
        var projectFolder = Path.GetFullPath(ProjectFolder(ownerUserId, projectId));
        var candidate = Path.GetFullPath(Path.Combine(projectFolder, relativePath));
        var prefix = projectFolder.TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar;
        return candidate.StartsWith(prefix, StringComparison.OrdinalIgnoreCase) ? candidate : null;
    }

    private string OwnerFolder(string ownerUserId)
    {
        if (string.IsNullOrWhiteSpace(ownerUserId)) throw new ArgumentException("账户不能为空。", nameof(ownerUserId));
        var hash = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(ownerUserId)))[..24];
        return Path.Combine(_root, hash);
    }

    private string ProjectFolder(string ownerUserId, Guid projectId) =>
        Path.Combine(OwnerFolder(ownerUserId), projectId.ToString());

    private static CandidatePage ProjectCandidates(
        string ownerFolder,
        StoryStoreCursor? after,
        int limit,
        CancellationToken cancellationToken)
    {
        var paths = Directory.EnumerateDirectories(ownerFolder)
            .Where(folder => Guid.TryParse(Path.GetFileName(folder), out _))
            .Select(folder => Path.Combine(folder, "project.json"))
            .Where(File.Exists);
        return CandidatePageFor(
            paths, after, limit, cancellationToken,
            path => Path.GetFileName(Path.GetDirectoryName(path)!));
    }

    private static CandidatePage FileCandidates(
        string folder,
        string pattern,
        StoryStoreCursor? after,
        int limit,
        CancellationToken cancellationToken) =>
        CandidatePageFor(
            Directory.EnumerateFiles(folder, pattern, SearchOption.TopDirectoryOnly),
            after, limit, cancellationToken);

    private static CandidatePage CandidatePageFor(
        IEnumerable<string> paths,
        StoryStoreCursor? after,
        int requestedLimit,
        CancellationToken cancellationToken,
        Func<string, string>? recordId = null)
    {
        var limit = Math.Clamp(requestedLimit, 1, 200);
        var candidates = new List<FileCandidate>(limit + 1);
        foreach (var path in paths)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var id = recordId is null ? Path.GetFileName(path) : recordId(path);
            var cursor = new StoryStoreCursor(
                File.GetLastWriteTimeUtc(path), id);
            if (after is not null && !IsAfter(cursor, after)) continue;
            var candidate = new FileCandidate(path, cursor);
            var index = candidates.FindIndex(value => IsNewer(cursor, value.Cursor));
            if (index < 0) index = candidates.Count;
            if (index > limit) continue;
            candidates.Insert(index, candidate);
            if (candidates.Count > limit + 1) candidates.RemoveAt(candidates.Count - 1);
        }
        var hasMore = candidates.Count > limit;
        if (hasMore) candidates.RemoveAt(candidates.Count - 1);
        return new(candidates, hasMore);
    }

    private static bool IsNewer(StoryStoreCursor left, StoryStoreCursor right) =>
        left.ModifiedAt != right.ModifiedAt
            ? left.ModifiedAt > right.ModifiedAt
            : string.CompareOrdinal(left.RecordId, right.RecordId) > 0;

    private static bool IsAfter(StoryStoreCursor value, StoryStoreCursor cursor) =>
        value.ModifiedAt != cursor.ModifiedAt
            ? value.ModifiedAt < cursor.ModifiedAt
            : string.CompareOrdinal(value.RecordId, cursor.RecordId) < 0;

    private static string SafeSegment(string value)
    {
        var safe = new string(value.Where(character => char.IsAsciiLetterOrDigit(character) || character is '-' or '_').ToArray());
        return safe.Length == 0 ? "segment" : safe[..Math.Min(safe.Length, 64)];
    }

    private sealed record FileCandidate(string Path, StoryStoreCursor Cursor);
    private sealed record CandidatePage(List<FileCandidate> Candidates, bool HasMore);
}

internal sealed record StoryStoreCursor(DateTime ModifiedAt, string RecordId);
internal sealed record StoryStorePage<T>(IReadOnlyList<T> Items, StoryStoreCursor? NextCursor);
