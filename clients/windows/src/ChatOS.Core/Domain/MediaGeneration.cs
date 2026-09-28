namespace ChatOS.Core.Domain;

public sealed record MediaGenerationModel(
    string Id,
    string Name,
    string Provider,
    string ModelName,
    bool Enabled,
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
