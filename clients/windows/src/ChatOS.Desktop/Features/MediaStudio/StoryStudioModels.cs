namespace ChatOS.Desktop.Features.MediaStudio;

public sealed record StoryProjectDocument(
    Guid Id,
    int Version,
    string Title,
    string Description,
    string Source,
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
    public int TotalSeconds => Segments.Sum(segment => segment.Seconds);
    public int CompletedCount => Segments.Count(segment => !string.IsNullOrWhiteSpace(segment.VideoAsset));

    public void Validate()
    {
        if (Version != CurrentVersion || Id == Guid.Empty ||
            string.IsNullOrWhiteSpace(Title) || Title.Trim().Length > 120 ||
            Description.Length > 4_000 || Source.Length > 80_000 || VisualStyle.Length > 2_000 ||
            string.IsNullOrWhiteSpace(TextModelConfigId) ||
            string.IsNullOrWhiteSpace(ImageModelConfigId) ||
            string.IsNullOrWhiteSpace(VideoModelConfigId) ||
            !StoryStudioOptions.Ratios.Contains(Ratio) || Segments.Count > 200 ||
            Segments.Select(segment => segment.Id).Distinct(StringComparer.Ordinal).Count() != Segments.Count)
        {
            throw new InvalidDataException("剧情项目数据无效，请检查标题、模型、内容长度和分段。");
        }

        foreach (var segment in Segments) segment.Validate();
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
    public void Validate()
    {
        if (string.IsNullOrWhiteSpace(Id) || Id.Length > 80 ||
            string.IsNullOrWhiteSpace(Title) || Title.Length > 200 ||
            Narrative.Length > 8_000 || ImagePrompt.Length > 7_000 || VideoPrompt.Length > 7_000 ||
            Seconds is < 2 or > 30 ||
            !SafeAsset(FirstFrameAsset) || !SafeAsset(LastFrameAsset) || !SafeAsset(VideoAsset))
        {
            throw new InvalidDataException("剧情分段数据无效，请检查标题、提示词、时长和素材。");
        }
    }

    private static bool SafeAsset(string? value) => value is null ||
        (!Path.IsPathFullyQualified(value) &&
         !value.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar).Contains(".."));
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
