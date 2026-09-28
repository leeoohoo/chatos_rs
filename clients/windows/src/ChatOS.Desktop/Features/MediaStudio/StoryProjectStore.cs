using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed class StoryProjectStore
{
    private const int MaximumManifestBytes = 16 * 1024 * 1024;
    private const int MaximumImageBytes = 20 * 1024 * 1024;
    private const int MaximumVideoBytes = 512 * 1024 * 1024;
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
    {
        var ownerFolder = OwnerFolder(ownerUserId);
        if (!Directory.Exists(ownerFolder)) return [];
        var projects = new List<StoryProjectDocument>();
        foreach (var folder in Directory.EnumerateDirectories(ownerFolder))
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (!Guid.TryParse(Path.GetFileName(folder), out var folderId)) continue;
            var manifestPath = Path.Combine(folder, "project.json");
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
        return projects.OrderByDescending(project => project.UpdatedAt).ToArray();
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

    private static string SafeSegment(string value)
    {
        var safe = new string(value.Where(character => char.IsAsciiLetterOrDigit(character) || character is '-' or '_').ToArray());
        return safe.Length == 0 ? "segment" : safe[..Math.Min(safe.Length, 64)];
    }
}
