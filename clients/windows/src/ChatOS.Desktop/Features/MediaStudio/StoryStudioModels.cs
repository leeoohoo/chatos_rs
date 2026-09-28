namespace ChatOS.Desktop.Features.MediaStudio;

public sealed record StoryProjectDocument(
    Guid Id,
    int Version,
    string Title,
    string Description,
    string Source,
    string Summary,
    string VisualStyle,
    string Ratio,
    string TextModelConfigId,
    string ImageModelConfigId,
    string VideoModelConfigId,
    IReadOnlyList<StorySegmentDocument> Segments,
    DateTimeOffset CreatedAt,
    DateTimeOffset UpdatedAt)
{
    public const int CurrentVersion = 1;
    public string CreativeRequirements { get; init; } = string.Empty;
    public IReadOnlyList<StoryResourceDocument> Resources { get; init; } = [];
    public int TotalSeconds => Segments.Sum(segment => segment.Seconds);
    public int CompletedCount => Segments.Count(segment => !string.IsNullOrWhiteSpace(segment.VideoAsset));

    public void Validate()
    {
        if (Version != CurrentVersion || Id == Guid.Empty ||
            string.IsNullOrWhiteSpace(Title) || Title.Trim().Length > 120 ||
            Description.Length > 4_000 || Source.Length > 80_000 || (Summary?.Length ?? 0) > 16_000 ||
            VisualStyle.Length > 2_000 || CreativeRequirements is null or { Length: > 2_000 } ||
            string.IsNullOrWhiteSpace(TextModelConfigId) ||
            string.IsNullOrWhiteSpace(ImageModelConfigId) ||
            string.IsNullOrWhiteSpace(VideoModelConfigId) ||
            !StoryStudioOptions.Ratios.Contains(Ratio) || Segments is null || Resources is null ||
            Segments.Count > 200 || Resources.Count > 100 ||
            Segments.Select(segment => segment.Id).Distinct(StringComparer.Ordinal).Count() != Segments.Count)
        {
            throw new InvalidDataException("剧情项目数据无效，请检查标题、模型、内容长度和分段。");
        }

        if (Resources.Select(resource => resource.Id).Distinct(StringComparer.Ordinal).Count() != Resources.Count)
            throw new InvalidDataException("剧情素材 ID 重复。");
        foreach (var resource in Resources) resource.Validate();
        var resourceIds = Resources.Select(resource => resource.Id).ToHashSet(StringComparer.Ordinal);
        foreach (var segment in Segments)
        {
            segment.Validate();
            if (segment.ResourceIds.Any(id => !resourceIds.Contains(id)))
                throw new InvalidDataException("剧情分段引用了不存在的角色、场景或道具。");
        }
    }
}

public sealed record StorySegmentDocument(
    string Id,
    string Title,
    string Narrative,
    string ImagePrompt,
    string VideoPrompt,
    int Seconds,
    string? FirstFrameAsset,
    string? LastFrameAsset,
    string? VideoAsset)
{
    public StorySegmentKind Kind { get; init; } = StorySegmentKind.Story;
    public bool IsRefined { get; init; }
    public string? PendingVideoJobId { get; init; }
    public string? PendingVideoJobStatus { get; init; }
    public string? PendingVideoRequestDigest { get; init; }
    public string PendingVideoGuidance { get; init; } = "frames";
    public string? ActualVideoLastFrameAsset { get; init; }
    public IReadOnlyList<StoryArchivedVideoDocument> ArchivedVideos { get; init; } = [];
    public IReadOnlyList<StoryArchivedFrameDocument> ArchivedFrames { get; init; } = [];
    public IReadOnlyList<string> ResourceIds { get; init; } = [];
    public string ContinuityIn { get; init; } = string.Empty;
    public string ContinuityOut { get; init; } = string.Empty;
    public string ShotPlan { get; init; } = string.Empty;

    public void Validate()
    {
        if (string.IsNullOrWhiteSpace(Id) || Id.Length > 80 ||
            string.IsNullOrWhiteSpace(Title) || Title.Length > 200 ||
            Narrative.Length > 8_000 || ImagePrompt.Length > 7_000 || VideoPrompt.Length > 7_000 ||
            ContinuityIn.Length > 2_000 || ContinuityOut.Length > 2_000 || ShotPlan.Length > 8_000 ||
            Seconds is < 2 or > 30 || !Enum.IsDefined(Kind) || ResourceIds is null ||
            ArchivedVideos is null or { Count: > 20 } ||
            ArchivedVideos.Any(archived => archived is null) ||
            ArchivedFrames is null or { Count: > 40 } ||
            ArchivedFrames.Any(archived => archived is null) ||
            !SafeJob(PendingVideoJobId, 512) || !SafeJob(PendingVideoJobStatus, 80) ||
            !SafeDigest(PendingVideoRequestDigest) ||
            PendingVideoGuidance is not ("frames" or "source-video") ||
            (PendingVideoJobId is null) != (PendingVideoRequestDigest is null) ||
            PendingVideoJobId is null && PendingVideoJobStatus is not null ||
            PendingVideoJobId is null && PendingVideoGuidance != "frames" ||
            !SafeAsset(FirstFrameAsset) || !SafeAsset(LastFrameAsset) || !SafeAsset(VideoAsset) ||
            !SafeAsset(ActualVideoLastFrameAsset))
        {
            throw new InvalidDataException("剧情分段数据无效，请检查标题、提示词、时长和素材。");
        }
        foreach (var archived in ArchivedVideos) archived.Validate();
        foreach (var archived in ArchivedFrames) archived.Validate();
    }

    private static bool SafeAsset(string? value) => value is null ||
        (!Path.IsPathFullyQualified(value) &&
         !value.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar).Contains(".."));

    private static bool SafeJob(string? value, int maximum) =>
        value is null || value.Length is > 0 && value.Length <= maximum;

    private static bool SafeDigest(string? value) => value is null ||
        value.Length == 64 && value.All(character => char.IsAsciiHexDigit(character));
}

public sealed record StoryArchivedFrameDocument(
    string Asset,
    bool IsLastFrame,
    string Label,
    DateTimeOffset ArchivedAt)
{
    public void Validate()
    {
        if (string.IsNullOrWhiteSpace(Asset) || Path.IsPathFullyQualified(Asset) ||
            Asset.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar).Contains("..") ||
            string.IsNullOrWhiteSpace(Label) || Label.Length > 200)
            throw new InvalidDataException("剧情画面历史版本数据无效。");
    }
}

public sealed record StoryArchivedVideoDocument(
    string Asset,
    string? ActualLastFrameAsset,
    string Label,
    DateTimeOffset ArchivedAt)
{
    public void Validate()
    {
        if (!SafeAsset(Asset) || ActualLastFrameAsset is not null && !SafeAsset(ActualLastFrameAsset) ||
            string.IsNullOrWhiteSpace(Label) || Label.Length > 200)
            throw new InvalidDataException("剧情视频历史版本数据无效。");
    }

    private static bool SafeAsset(string value) => !string.IsNullOrWhiteSpace(value) &&
        !Path.IsPathFullyQualified(value) &&
        !value.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar).Contains("..");
}

public enum StorySegmentKind
{
    Story,
    Transition,
}

public sealed record StorySegmentKindOption(StorySegmentKind Kind, string Name);

public enum StoryResourceKind
{
    Character,
    Scene,
    Prop,
}

public sealed record StoryResourceDocument(
    string Id,
    StoryResourceKind Kind,
    string Name,
    string Description,
    string ImagePrompt,
    string? ImageAsset)
{
    public void Validate()
    {
        if (string.IsNullOrWhiteSpace(Id) || Id.Length > 80 ||
            string.IsNullOrWhiteSpace(Name) || Name.Length > 200 ||
            Description.Length > 8_000 || ImagePrompt.Length > 7_000 ||
            ImageAsset is not null && (Path.IsPathFullyQualified(ImageAsset) ||
                ImageAsset.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar).Contains("..")))
        {
            throw new InvalidDataException("剧情角色、场景或道具数据无效。");
        }
    }
}

public static class StoryStudioOptions
{
    public static IReadOnlyList<string> Ratios { get; } = ["16:9", "9:16", "1:1", "4:3", "3:4", "21:9"];
}

public sealed record StoryProjectCard(StoryProjectDocument Project)
{
    public Guid Id => Project.Id;
    public string Title => Project.Title;
    public string Description => string.IsNullOrWhiteSpace(Project.Description) ? "暂无描述" : Project.Description;
    public string ProgressLabel => Project.Segments.Count == 0
        ? "待分段"
        : $"{Project.CompletedCount}/{Project.Segments.Count} 段完成 · {Project.TotalSeconds} 秒";
    public string UpdatedAtLabel => Project.UpdatedAt.ToLocalTime().ToString("MM-dd HH:mm");
}
