using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.MediaStudio;

public enum StoryPlanningRunStatus
{
    Running,
    Paused,
    Failed,
    DraftReady,
    Applied,
    Abandoned,
}

public sealed record StoryPlanningRunDocument(
    Guid Id,
    Guid ProjectId,
    string BaseDigest,
    StoryPlanningRequest Request,
    StoryPlanningRunStatus Status,
    StoryPlanningResult? Draft,
    string? Error,
    DateTimeOffset CreatedAt,
    DateTimeOffset UpdatedAt)
{
    public bool CanResume => Status is StoryPlanningRunStatus.Running or
        StoryPlanningRunStatus.Paused or StoryPlanningRunStatus.Failed or
        StoryPlanningRunStatus.DraftReady;

    public void Validate()
    {
        if (Id == Guid.Empty || ProjectId == Guid.Empty || BaseDigest is not { Length: 64 } ||
            Request is null || !Enum.IsDefined(Status) ||
            string.IsNullOrWhiteSpace(Request.ModelConfigId) ||
            string.IsNullOrWhiteSpace(Request.Title) || Request.Title.Length > 120 ||
            Request.Description is null or { Length: > 4_000 } ||
            string.IsNullOrWhiteSpace(Request.Source) || Request.Source.Length > 80_000 ||
            Request.VisualStyle is null or { Length: > 2_000 } ||
            Request.CreativeRequirements is null or { Length: > 2_000 } ||
            !StoryStudioOptions.Ratios.Contains(Request.Ratio) || Request.MaximumSegments is < 1 or > 200 ||
            CreatedAt > UpdatedAt || Error is { Length: > 8_000 })
        {
            throw new InvalidDataException("剧情规划运行记录无效。");
        }
        if (Draft is not null) ValidateDraft(Draft);
    }

    private static void ValidateDraft(StoryPlanningResult draft)
    {
        if (draft.Summary is null or { Length: > 16_000 } ||
            draft.Resources is null or { Count: > 100 } ||
            draft.Segments is not { Count: > 0 and <= 200 })
            throw new InvalidDataException("剧情规划草稿无效。");
        if (draft.Resources.Any(resource => resource is null ||
                !Identifier(resource.Id) || resource.Kind is not ("character" or "scene" or "prop") ||
                string.IsNullOrWhiteSpace(resource.Name) || resource.Name.Length > 200 ||
                resource.Description is null or { Length: > 8_000 } ||
                string.IsNullOrWhiteSpace(resource.ImagePrompt) || resource.ImagePrompt.Length > 7_000))
            throw new InvalidDataException("剧情规划素材无效。");
        var resourceIds = draft.Resources.Select(resource => resource.Id).ToHashSet(StringComparer.Ordinal);
        if (resourceIds.Count != draft.Resources.Count || draft.Segments.Any(segment => segment is null ||
                segment.Kind is not ("story" or "transition") ||
                string.IsNullOrWhiteSpace(segment.Title) || segment.Title.Length > 200 ||
                string.IsNullOrWhiteSpace(segment.Narrative) || segment.Narrative.Length > 8_000 ||
                string.IsNullOrWhiteSpace(segment.ImagePrompt) || segment.ImagePrompt.Length > 7_000 ||
                string.IsNullOrWhiteSpace(segment.VideoPrompt) || segment.VideoPrompt.Length > 7_000 ||
                segment.Seconds is < 2 or > 15 || segment.ResourceIds is null ||
                segment.ResourceIds.Distinct(StringComparer.Ordinal).Count() != segment.ResourceIds.Count ||
                segment.ResourceIds.Any(id => !Identifier(id) || !resourceIds.Contains(id))))
            throw new InvalidDataException("剧情规划分段无效。");
    }

    private static bool Identifier(string? value) => value is { Length: > 0 and <= 80 } &&
        value.All(character => char.IsAsciiLetterOrDigit(character) || character is '-' or '_');
}

public sealed class StoryPlanningRunCard
{
    public StoryPlanningRunCard(StoryPlanningRunDocument run) => Run = run;

    public StoryPlanningRunDocument Run { get; }
    public Guid Id => Run.Id;
    public string StatusLabel => Run.Status switch
    {
        StoryPlanningRunStatus.Running => "运行中断，可恢复",
        StoryPlanningRunStatus.Paused => "已暂停，可恢复",
        StoryPlanningRunStatus.Failed => "失败，可重试",
        StoryPlanningRunStatus.DraftReady => "草稿已生成，可直接应用",
        StoryPlanningRunStatus.Applied => "已应用",
        _ => "已放弃",
    };
    public string UpdatedAtLabel => Run.UpdatedAt.ToLocalTime().ToString("MM-dd HH:mm");
    public string Detail => Run.Error ?? (Run.Draft is null
        ? "模型尚未返回完整草稿"
        : $"草稿包含 {Run.Draft.Resources.Count} 个素材、{Run.Draft.Segments.Count} 个分段");
}
