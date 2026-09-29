using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Api.Media;
using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed class MediaStudioHistoryStore
{
    private const int MaximumImageBytes = 20 * 1024 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true,
    };
    private readonly IHttpClientFactory _httpClientFactory;
    private readonly string _root;

    public MediaStudioHistoryStore(IHttpClientFactory httpClientFactory, string? root = null)
    {
        _httpClientFactory = httpClientFactory;
        _root = root ?? Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ChatOS",
            "media-studio");
    }

    public async Task<IReadOnlyList<MediaStudioHistoryItem>> LoadAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        var ownerFolder = OwnerFolder(ownerUserId);
        if (!Directory.Exists(ownerFolder)) return [];
        var items = new List<MediaStudioHistoryItem>();
        foreach (var manifestPath in Directory.EnumerateFiles(ownerFolder, "entry.json", SearchOption.AllDirectories))
        {
            cancellationToken.ThrowIfCancellationRequested();
            try
            {
                await using var stream = File.OpenRead(manifestPath);
                var manifest = await JsonSerializer.DeserializeAsync<HistoryManifest>(
                    stream,
                    JsonOptions,
                    cancellationToken).ConfigureAwait(false);
                if (manifest is null) continue;
                var folder = Path.GetDirectoryName(manifestPath)!;
                var images = manifest.Images
                    .Select(image => new MediaStudioImageItem(
                        image.Id,
                        Path.Combine(folder, image.FileName),
                        image.MimeType,
                        image.RevisedPrompt))
                    .Where(image => File.Exists(image.FilePath))
                    .ToArray();
                if (images.Length == 0) continue;
                items.Add(new MediaStudioHistoryItem(
                    manifest.Id,
                    manifest.Prompt,
                    manifest.ModelName,
                    manifest.CreatedAt,
                    images));
            }
            catch (Exception exception) when (exception is IOException or JsonException or UnauthorizedAccessException)
            {
                StartupDiagnostics.RecordStage($"media history entry skipped: {Path.GetFileName(Path.GetDirectoryName(manifestPath))}");
            }
        }
        return items.OrderByDescending(item => item.CreatedAt).ToArray();
    }

    public async Task<MediaStudioHistoryItem> SaveAsync(
        string ownerUserId,
        string prompt,
        ImageGenerationResult result,
        CancellationToken cancellationToken = default)
    {
        var entryFolder = Path.Combine(
            OwnerFolder(ownerUserId),
            $"{result.CreatedAt:yyyyMMddHHmmssfff}-{SafeSegment(result.Id)}-{Guid.NewGuid():N}");
        Directory.CreateDirectory(entryFolder);
        var stored = new List<StoredImage>();
        var records = new List<MediaStudioImageItem>();
        for (var index = 0; index < result.Images.Count; index++)
        {
            var asset = result.Images[index];
            var bytes = await ReadAssetAsync(asset, cancellationToken).ConfigureAwait(false);
            var extension = Extension(asset.MimeType);
            var fileName = $"image-{index + 1}{extension}";
            var filePath = Path.Combine(entryFolder, fileName);
            await File.WriteAllBytesAsync(filePath, bytes, cancellationToken).ConfigureAwait(false);
            stored.Add(new StoredImage(asset.Id, fileName, asset.MimeType, asset.RevisedPrompt));
            records.Add(new MediaStudioImageItem(asset.Id, filePath, asset.MimeType, asset.RevisedPrompt));
        }

        var manifest = new HistoryManifest(
            result.Id,
            prompt,
            result.ModelName,
            result.CreatedAt,
            stored);
        var temporaryPath = Path.Combine(entryFolder, "entry.json.tmp");
        await File.WriteAllTextAsync(
            temporaryPath,
            JsonSerializer.Serialize(manifest, JsonOptions),
            cancellationToken).ConfigureAwait(false);
        File.Move(temporaryPath, Path.Combine(entryFolder, "entry.json"), true);
        return new MediaStudioHistoryItem(
            result.Id,
            prompt,
            result.ModelName,
            result.CreatedAt,
            records);
    }

    public async Task<IReadOnlyList<MediaStudioVideoHistoryItem>> LoadVideosAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        var ownerFolder = OwnerFolder(ownerUserId);
        if (!Directory.Exists(ownerFolder)) return [];
        var items = new List<MediaStudioVideoHistoryItem>();
        foreach (var manifestPath in Directory.EnumerateFiles(ownerFolder, "video.json", SearchOption.AllDirectories))
        {
            cancellationToken.ThrowIfCancellationRequested();
            try
            {
                await using var stream = File.OpenRead(manifestPath);
                var manifest = await JsonSerializer.DeserializeAsync<VideoManifest>(
                    stream,
                    JsonOptions,
                    cancellationToken).ConfigureAwait(false);
                if (manifest is null) continue;
                var filePath = Path.Combine(Path.GetDirectoryName(manifestPath)!, manifest.FileName);
                if (!File.Exists(filePath)) continue;
                items.Add(new MediaStudioVideoHistoryItem(
                    manifest.Id,
                    manifest.Prompt,
                    manifest.ModelName,
                    manifest.CreatedAt,
                    filePath,
                    manifest.MimeType));
            }
            catch (Exception exception) when (exception is IOException or JsonException or UnauthorizedAccessException)
            {
                StartupDiagnostics.RecordStage($"media video history entry skipped: {Path.GetFileName(Path.GetDirectoryName(manifestPath))}");
            }
        }
        return items.OrderByDescending(item => item.CreatedAt).ToArray();
    }

    public async Task<MediaStudioVideoHistoryItem> SaveVideoAsync(
        string ownerUserId,
        string prompt,
        VideoGenerationResult result,
        CancellationToken cancellationToken = default)
    {
        if (result.VideoData.Length == 0 || result.VideoData.Length > 512 * 1024 * 1024)
            throw new InvalidDataException("Generated video is empty or exceeds 512 MB.");
        var entryFolder = Path.Combine(
            OwnerFolder(ownerUserId),
            $"{result.CreatedAt:yyyyMMddHHmmssfff}-{SafeSegment(result.Id)}-{Guid.NewGuid():N}");
        Directory.CreateDirectory(entryFolder);
        var extension = result.MimeType.Contains("quicktime", StringComparison.OrdinalIgnoreCase)
            ? ".mov"
            : ".mp4";
        var fileName = $"video{extension}";
        var filePath = Path.Combine(entryFolder, fileName);
        await File.WriteAllBytesAsync(filePath, result.VideoData, cancellationToken).ConfigureAwait(false);
        var manifest = new VideoManifest(
            result.Id,
            prompt,
            result.ModelName,
            result.CreatedAt,
            fileName,
            result.MimeType);
        var temporaryPath = Path.Combine(entryFolder, "video.json.tmp");
        await File.WriteAllTextAsync(
            temporaryPath,
            JsonSerializer.Serialize(manifest, JsonOptions),
            cancellationToken).ConfigureAwait(false);
        File.Move(temporaryPath, Path.Combine(entryFolder, "video.json"), true);
        return new MediaStudioVideoHistoryItem(
            result.Id,
            prompt,
            result.ModelName,
            result.CreatedAt,
            filePath,
            result.MimeType);
    }

    private async Task<byte[]> ReadAssetAsync(
        GeneratedMediaAsset asset,
        CancellationToken cancellationToken)
    {
        byte[] bytes;
        if (!string.IsNullOrWhiteSpace(asset.Base64Data))
        {
            try { bytes = Convert.FromBase64String(asset.Base64Data); }
            catch (FormatException exception) { throw new InvalidDataException("Generated image data is invalid.", exception); }
        }
        else if (asset.Url is { Scheme: "https" } url)
        {
            using var response = await _httpClientFactory.CreateClient(MediaGenerationService.ProviderClientName)
                .GetAsync(url, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
                .ConfigureAwait(false);
            response.EnsureSuccessStatusCode();
            var mediaType = response.Content.Headers.ContentType?.MediaType;
            if (mediaType is not null && !mediaType.StartsWith("image/", StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Generated image URL returned non-image content.");
            await using var input = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
            bytes = await ReadLimitedAsync(input, cancellationToken).ConfigureAwait(false);
        }
        else
        {
            throw new InvalidDataException("Generated image does not contain usable content.");
        }
        if (bytes.Length == 0 || bytes.Length > MaximumImageBytes)
            throw new InvalidDataException("Generated image is empty or exceeds 20 MB.");
        return bytes;
    }

    private static async Task<byte[]> ReadLimitedAsync(Stream input, CancellationToken cancellationToken)
    {
        using var output = new MemoryStream();
        var buffer = new byte[81920];
        while (true)
        {
            var read = await input.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (read == 0) break;
            if (output.Length + read > MaximumImageBytes)
                throw new InvalidDataException("Generated image exceeds 20 MB.");
            output.Write(buffer, 0, read);
        }
        return output.ToArray();
    }

    private string OwnerFolder(string ownerUserId)
    {
        var hash = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(ownerUserId)))[..24];
        return Path.Combine(_root, hash);
    }

    private static string SafeSegment(string value)
    {
        var safe = new string(value.Where(character => char.IsAsciiLetterOrDigit(character) || character is '-' or '_').ToArray());
        return safe.Length == 0 ? Guid.NewGuid().ToString("N") : safe[..Math.Min(safe.Length, 64)];
    }

    private static string Extension(string mimeType) => mimeType.ToLowerInvariant() switch
    {
        "image/jpeg" => ".jpg",
        "image/webp" => ".webp",
        _ => ".png",
    };

    private sealed record HistoryManifest(
        string Id,
        string Prompt,
        string ModelName,
        DateTimeOffset CreatedAt,
        IReadOnlyList<StoredImage> Images);

    private sealed record StoredImage(
        string Id,
        string FileName,
        string MimeType,
        string? RevisedPrompt);

    private sealed record VideoManifest(
        string Id,
        string Prompt,
        string ModelName,
        DateTimeOffset CreatedAt,
        string FileName,
        string MimeType);
}

public sealed record MediaStudioHistoryItem(
    string Id,
    string Prompt,
    string ModelName,
    DateTimeOffset CreatedAt,
    IReadOnlyList<MediaStudioImageItem> Images)
{
    public string CreatedAtLabel => CreatedAt.ToLocalTime().ToString("yyyy-MM-dd HH:mm");
}

public sealed record MediaStudioImageItem(
    string Id,
    string FilePath,
    string MimeType,
    string? RevisedPrompt);

public sealed record MediaStudioVideoHistoryItem(
    string Id,
    string Prompt,
    string ModelName,
    DateTimeOffset CreatedAt,
    string FilePath,
    string MimeType)
{
    public string CreatedAtLabel => CreatedAt.ToLocalTime().ToString("yyyy-MM-dd HH:mm");
}
