using System.Collections.ObjectModel;
using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed class StoryPromptAuditItem
{
    public StoryPromptAuditItem(string key, string name, string purpose, string content)
    {
        Key = key;
        Name = name;
        Purpose = purpose;
        Content = content;
    }

    public string Key { get; }
    public string Name { get; }
    public string Purpose { get; }
    public string Content { get; }
}

public sealed partial class StoryStudioViewModel
{
    public ObservableCollection<StoryPromptAuditItem> PromptAuditItems { get; } = [];

    private void RefreshPromptAudit()
    {
        PromptAuditItems.Clear();
        foreach (var definition in StoryPromptCatalog.Definitions)
        {
            PromptAuditItems.Add(new StoryPromptAuditItem(
                definition.Key,
                definition.Name,
                definition.Purpose,
                RenderPromptAuditContent(definition)));
        }
    }

    private string RenderPromptAuditContent(StoryPromptDefinition definition) => definition.Key switch
    {
        StoryPromptCatalog.PlanningSystemKey => StoryPromptCatalog.PlanningSystem,
        StoryPromptCatalog.PlanningUserKey when _current is not null =>
            StoryPromptCatalog.RenderPlanningUser(new StoryPlanningRequest(
                ProjectTextModel?.Id ?? string.Empty,
                ProjectTitle,
                ProjectDescription,
                ProjectSource,
                VisualStyle,
                ProjectRatio)),
        StoryPromptCatalog.OptimizeSourceKey when _current is not null =>
            RenderOptimizationAudit(StoryOptimizationTarget.Source),
        StoryPromptCatalog.OptimizeStyleKey when _current is not null =>
            RenderOptimizationAudit(StoryOptimizationTarget.VisualStyle),
        StoryPromptCatalog.RefineSegmentKey when SelectedSegment is { } refinementSegment =>
            RenderSegmentRefinementAudit(refinementSegment),
        StoryPromptCatalog.ResourceImageKey when SelectedResource is { } resource =>
            StoryPromptCatalog.RenderResourceImage(
                VisualStyle,
                resource.ImagePrompt,
                resource.KindLabel,
                resource.Name,
                ProjectRatio),
        StoryPromptCatalog.FirstFrameKey when SelectedSegment is { } firstSegment =>
            StoryPromptCatalog.RenderFrame(
                VisualStyle,
                firstSegment.ImagePrompt,
                false,
                ProjectRatio,
                BuildContinuityContext(firstSegment)),
        StoryPromptCatalog.LastFrameKey when SelectedSegment is { } lastSegment =>
            StoryPromptCatalog.RenderFrame(
                VisualStyle,
                lastSegment.ImagePrompt,
                true,
                ProjectRatio,
                BuildContinuityContext(lastSegment)),
        StoryPromptCatalog.VideoKey when SelectedSegment is { } videoSegment =>
            StoryPromptCatalog.RenderVideo(
                videoSegment.VideoPrompt,
                BuildContinuityContext(videoSegment)),
        _ => definition.Template,
    };

    private string RenderOptimizationAudit(StoryOptimizationTarget target)
    {
        var request = new StoryOptimizationRequest(
            ProjectTextModel?.Id ?? string.Empty,
            ProjectTitle,
            ProjectDescription,
            ProjectSource,
            VisualStyle,
            target);
        var system = target == StoryOptimizationTarget.Source
            ? StoryPromptCatalog.OptimizeSourceSystem
            : StoryPromptCatalog.OptimizeStyleSystem;
        return $"{system}\n\n用户上下文：\n{StoryPromptCatalog.RenderOptimizationUser(request)}";
    }

    private string RenderSegmentRefinementAudit(StorySegmentEditor segment)
    {
        var request = BuildSegmentRefinementRequest(
            segment,
            ProjectTextModel?.Id ?? string.Empty);
        return $"{StoryPromptCatalog.RefineSegmentSystem}\n\n用户上下文：\n{StoryPromptCatalog.RenderSegmentRefinementUser(request)}";
    }

    partial void OnProjectTitleChanged(string value) => RefreshPlanningInputs();
    partial void OnProjectDescriptionChanged(string value) => RefreshPlanningInputs();
    partial void OnProjectSourceChanged(string value) => RefreshPlanningInputs();
    partial void OnVisualStyleChanged(string value) => RefreshPlanningInputs();
    partial void OnProjectRatioChanged(string value) => RefreshPlanningInputs();
    partial void OnProjectTextModelChanged(MediaGenerationModel? value) => RefreshPlanningInputs();

    private void RefreshPlanningInputs()
    {
        RefreshPromptAudit();
        NotifyPlanningRunsChanged();
        NotifyOptimizationChanged();
        NotifySegmentRefinementChanged();
    }
}
