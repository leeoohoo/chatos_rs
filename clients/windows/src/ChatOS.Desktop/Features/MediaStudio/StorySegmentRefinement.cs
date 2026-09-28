using System.Security.Cryptography;
using System.Text;
using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel
{
    private StorySegmentRefinementSuggestion? _segmentRefinementSuggestion;
    private string? _segmentRefinementSegmentId;
    private string? _segmentRefinementBaseDigest;

    public bool HasSegmentRefinement => _segmentRefinementSuggestion is not null;
    public bool CanRefineSelectedSegment => CanSave && SelectedSegment is { } segment &&
        !string.IsNullOrWhiteSpace(segment.Narrative) && SegmentHasNoMedia(segment);
    public bool CanApplySegmentRefinement => !IsBusy &&
        _segmentRefinementSuggestion is not null && SelectedSegment is { } segment &&
        segment.Id == _segmentRefinementSegmentId && SegmentHasNoMedia(segment) &&
        _segmentRefinementBaseDigest == CurrentSegmentRefinementDigest(segment);
    public string SegmentRefinementTitle => SelectedSegment is { } segment
        ? $"{segment.NumberLabel} · {segment.Title} 的 AI 镜头候选"
        : "AI 镜头候选";
    public string SegmentRefinementRationale => _segmentRefinementSuggestion?.Rationale ?? string.Empty;
    public string SegmentRefinementPreview => _segmentRefinementSuggestion is { } value
        ? $"衔接起点\n{value.ContinuityIn}\n\n镜头计划\n{value.ShotPlan}\n\n衔接终点\n{value.ContinuityOut}\n\n画面提示词\n{value.ImagePrompt}\n\n视频提示词\n{value.VideoPrompt}"
        : string.Empty;

    public async Task RefineSelectedSegmentAsync(CancellationToken cancellationToken = default)
    {
        var segment = SelectedSegment;
        var model = ProjectTextModel;
        var project = _current;
        if (!CanRefineSelectedSegment || segment is null || model is null || project is null) return;
        var session = _session;
        var digest = CurrentSegmentRefinementDigest(segment);
        var request = BuildSegmentRefinementRequest(segment, model.Id);
        IsBusy = true;
        ErrorMessage = null;
        StatusMessage = $"文本模型正在细化 {segment.NumberLabel} 的镜头计划…";
        try
        {
            await PersistCurrentAsync(cancellationToken);
            var suggestion = await _planner.RefineSegmentAsync(request, cancellationToken);
            if (_session != session || _current?.Id != project.Id || SelectedSegment != segment) return;
            if (digest != CurrentSegmentRefinementDigest(segment))
                throw new InvalidOperationException("分段内容已改变，本次细化建议未覆盖当前编辑内容。");
            _segmentRefinementSuggestion = suggestion;
            _segmentRefinementSegmentId = segment.Id;
            _segmentRefinementBaseDigest = digest;
            NotifySegmentRefinementChanged();
            StatusMessage = "单段镜头候选已生成，确认采用前不会修改分段";
        }
        catch (OperationCanceledException)
        {
            StatusMessage = "已停止单段镜头细化，当前分段未被修改";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
            StatusMessage = "单段细化失败，当前分段未被修改";
        }
        finally
        {
            IsBusy = false;
        }
    }

    public async Task ApplySegmentRefinementAsync(CancellationToken cancellationToken = default)
    {
        var segment = SelectedSegment;
        var suggestion = _segmentRefinementSuggestion;
        if (!CanApplySegmentRefinement || segment is null || suggestion is null) return;
        var previous = new StorySegmentRefinementSuggestion(
            segment.ImagePrompt, segment.VideoPrompt, segment.ContinuityIn,
            segment.ContinuityOut, segment.ShotPlan, string.Empty);
        var wasRefined = segment.IsRefined;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            ApplySegmentRefinement(segment, suggestion);
            await PersistCurrentAsync(cancellationToken);
            ClearSegmentRefinement();
            StatusMessage = $"已应用并保存 {segment.NumberLabel} 的 AI 镜头计划";
        }
        catch (OperationCanceledException)
        {
            ApplySegmentRefinement(segment, previous, wasRefined);
            StatusMessage = "已停止保存镜头计划，并恢复修改前内容";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ApplySegmentRefinement(segment, previous, wasRefined);
            ErrorMessage = exception.Message;
            StatusMessage = "镜头计划保存失败，已恢复修改前内容";
        }
        finally
        {
            IsBusy = false;
        }
    }

    public void ClearSegmentRefinement()
    {
        _segmentRefinementSuggestion = null;
        _segmentRefinementSegmentId = null;
        _segmentRefinementBaseDigest = null;
        NotifySegmentRefinementChanged();
    }

    private async Task RefineSegmentCoreAsync(
        StorySegmentEditor segment,
        string owner,
        Guid projectId,
        Guid session,
        MediaGenerationModel textModel,
        CancellationToken cancellationToken)
    {
        EnsureBatchContext(owner, projectId, session);
        if (!Segments.Contains(segment) || segment.IsRefined || !SegmentHasNoMedia(segment)) return;
        var previous = new StorySegmentRefinementSuggestion(
            segment.ImagePrompt, segment.VideoPrompt, segment.ContinuityIn,
            segment.ContinuityOut, segment.ShotPlan, string.Empty);
        var wasRefined = segment.IsRefined;
        try
        {
            var request = BuildSegmentRefinementRequest(segment, textModel.Id);
            var suggestion = await _planner.RefineSegmentAsync(request, cancellationToken);
            EnsureBatchContext(owner, projectId, session);
            if (!Segments.Contains(segment) || !SegmentHasNoMedia(segment))
                throw new OperationCanceledException("分段或媒体状态已改变。");
            ApplySegmentRefinement(segment, suggestion);
            await PersistCurrentAsync(cancellationToken);
        }
        catch
        {
            ApplySegmentRefinement(segment, previous, wasRefined);
            throw;
        }
    }

    private static bool SegmentHasNoMedia(StorySegmentEditor segment) =>
        segment.FirstFramePath is null && segment.LastFramePath is null && segment.VideoPath is null;

    private static void ApplySegmentRefinement(
        StorySegmentEditor segment,
        StorySegmentRefinementSuggestion suggestion,
        bool isRefined = true)
    {
        segment.ImagePrompt = suggestion.ImagePrompt;
        segment.VideoPrompt = suggestion.VideoPrompt;
        segment.ContinuityIn = suggestion.ContinuityIn;
        segment.ContinuityOut = suggestion.ContinuityOut;
        segment.ShotPlan = suggestion.ShotPlan;
        segment.IsRefined = isRefined;
    }

    private string BuildSegmentResourceContext(StorySegmentEditor segment)
    {
        var ids = segment.ToDocument().ResourceIds.ToHashSet(StringComparer.Ordinal);
        var lines = Resources.Where(resource => ids.Contains(resource.Id)).Select(resource =>
            $"{resource.KindLabel} {resource.Id}：{resource.Name}；{resource.Description}；视觉约束：{resource.ImagePrompt}");
        var value = string.Join("\n", lines);
        return value[..Math.Min(value.Length, 16_000)];
    }

    private StorySegmentRefinementRequest BuildSegmentRefinementRequest(
        StorySegmentEditor segment,
        string modelConfigId) => new(
            modelConfigId,
            ProjectTitle.Trim(),
            ProjectSummary.Trim(),
            VisualStyle.Trim(),
            ProjectRatio,
            segment.Id,
            segment.Kind == StorySegmentKind.Transition ? "transition" : "story",
            segment.Title.Trim(),
            segment.Narrative.Trim(),
            segment.Seconds,
            segment.ImagePrompt.Trim(),
            segment.VideoPrompt.Trim(),
            BuildContinuityContext(segment),
            BuildSegmentResourceContext(segment));

    private string CurrentSegmentRefinementDigest(StorySegmentEditor segment)
    {
        var value = string.Join('\u001f',
            _current?.Id.ToString() ?? string.Empty,
            ProjectTitle, ProjectSummary, VisualStyle, ProjectRatio,
            segment.Id, segment.Kind.ToString(), segment.Title, segment.Narrative, segment.Seconds.ToString(),
            segment.ImagePrompt, segment.VideoPrompt, segment.ContinuityIn,
            segment.ContinuityOut, segment.ShotPlan, segment.ResourceIdsText,
            BuildContinuityContext(segment), BuildSegmentResourceContext(segment));
        return Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant();
    }

    private void NotifySegmentRefinementChanged()
    {
        OnPropertyChanged(nameof(HasSegmentRefinement));
        OnPropertyChanged(nameof(CanRefineSelectedSegment));
        OnPropertyChanged(nameof(CanApplySegmentRefinement));
        OnPropertyChanged(nameof(SegmentRefinementTitle));
        OnPropertyChanged(nameof(SegmentRefinementRationale));
        OnPropertyChanged(nameof(SegmentRefinementPreview));
    }
}
