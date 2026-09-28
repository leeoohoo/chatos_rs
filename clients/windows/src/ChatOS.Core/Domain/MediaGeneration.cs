namespace ChatOS.Core.Domain;

public sealed record MediaGenerationModel(
    string Id,
    string Name,
    string Provider,
    string ModelName,
    bool Enabled,
    bool TaskEnabled,
    bool HasApiKey)
{
    public bool IsLikelyVideoModel
    {
        get
        {
            var value = $"{Name} {ModelName}".ToLowerInvariant();
            string[] markers =
            [
                "video", "sora", "veo", "kling", "seedance", "hailuo",
                "minimax", "runway", "luma", "pixverse", "vidu", "hunyuan",
                "cogvideo", "wan2", "wan-", "wan_",
            ];
            return markers.Any(value.Contains);
        }
    }
}

public sealed class VideoGenerationProfile
{
    private VideoGenerationProfile(
        IReadOnlyList<string> sizes,
        IReadOnlyList<int> durations,
        bool supportsLastFrame,
        bool supportsReferenceVideo)
    {
        Sizes = sizes;
        Durations = durations;
        SupportsLastFrame = supportsLastFrame;
        SupportsReferenceVideo = supportsReferenceVideo;
    }

    public IReadOnlyList<string> Sizes { get; }
    public IReadOnlyList<int> Durations { get; }
    public bool SupportsLastFrame { get; }
    public bool SupportsReferenceVideo { get; }

    public static VideoGenerationProfile ForModel(string modelName)
    {
        var value = modelName.Trim().ToLowerInvariant();
        if (value == "minimax-h3")
            return new(["768P", "2K"], Enumerable.Range(4, 12).ToArray(), true, true);
        if (value == "minimax-h3-max")
            return new(["768P", "480P"], Enumerable.Range(5, 11).ToArray(), true, true);
        if (value.Contains("seedance-2-5") || value.Contains("seedance-2.5"))
            return new(["720p", "1080p", "480p"], Enumerable.Range(4, 27).ToArray(), true, true);
        if (value.Contains("seedance-2-0-fast") || value.Contains("seedance-2.0-fast") ||
            value.Contains("seedance-2-0-mini") || value.Contains("seedance-2.0-mini"))
            return new(["720p", "480p"], Enumerable.Range(4, 12).ToArray(), true, true);
        if (value.Contains("seedance-2-0") || value.Contains("seedance-2.0"))
            return new(["720p", "1080p", "480p"], Enumerable.Range(4, 12).ToArray(), true, true);
        return new(["1280x720", "720x1280", "1792x1024", "1024x1792"], [4, 8, 12], false, false);
    }

    public static IReadOnlyList<string> Ratios { get; } = ["16:9", "9:16", "1:1", "4:3", "3:4", "21:9"];
}

public sealed record ImageGenerationInput(
    string Name,
    string MimeType,
    string Base64Data);

public sealed record ImageGenerationRequest(
    string ModelConfigId,
    string Prompt,
    string? Size,
    int Count,
    IReadOnlyList<ImageGenerationInput> ReferenceImages);

public sealed record GeneratedMediaAsset(
    string Id,
    string MimeType,
    string? Base64Data,
    Uri? Url,
    string? RevisedPrompt);

public sealed record ImageGenerationResult(
    string Id,
    string ModelConfigId,
    string ModelName,
    DateTimeOffset CreatedAt,
    IReadOnlyList<GeneratedMediaAsset> Images);

public sealed record VideoGenerationRequest(
    string ModelConfigId,
    string Prompt,
    string Size,
    int Seconds,
    ImageGenerationInput? FirstFrame,
    ImageGenerationInput? LastFrame,
    string Ratio);

public sealed record VideoGenerationProgress(
    string Status,
    double? Percent = null,
    string? JobId = null);

public sealed record VideoGenerationResult(
    string Id,
    string ModelConfigId,
    string ModelName,
    DateTimeOffset CreatedAt,
    string MimeType,
    byte[] VideoData);
