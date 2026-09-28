using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel
{
    private StoryOptimizationTarget? _optimizationTarget;
    private StoryOptimizationSuggestion? _optimizationSuggestion;
    private string? _optimizationBaseDigest;

    public bool HasOptimizationSuggestion => _optimizationSuggestion is not null;
    public bool CanOptimizeStorySource => CanOptimize(StoryOptimizationTarget.Source);
    public bool CanOptimizeVisualStyle => CanOptimize(StoryOptimizationTarget.VisualStyle);
    public bool CanApplyOptimization => !IsBusy && _optimizationSuggestion is not null &&
        _optimizationBaseDigest == CurrentPlanningDigest() && Segments.Count == 0;
    public string OptimizationTitle => _optimizationTarget == StoryOptimizationTarget.VisualStyle
        ? "AI 画面风格优化建议"
        : "AI 剧情原文优化建议";
    public string OptimizationText => _optimizationSuggestion?.OptimizedText ?? string.Empty;
    public string OptimizationRationale => _optimizationSuggestion?.Rationale ?? string.Empty;

    public Task OptimizeStorySourceAsync(CancellationToken cancellationToken = default) =>
        OptimizeAsync(StoryOptimizationTarget.Source, cancellationToken);

    public Task OptimizeVisualStyleAsync(CancellationToken cancellationToken = default) =>
        OptimizeAsync(StoryOptimizationTarget.VisualStyle, cancellationToken);

    public void ApplyOptimizationSuggestion()
    {
        if (!CanApplyOptimization || _optimizationSuggestion is null || _optimizationTarget is null) return;
        if (_optimizationTarget == StoryOptimizationTarget.Source)
            ProjectSource = _optimizationSuggestion.OptimizedText;
        else
            VisualStyle = _optimizationSuggestion.OptimizedText;
        ClearOptimizationSuggestion();
        StatusMessage = "已采用 AI 优化候选，请检查后保存项目";
    }

    public void ClearOptimizationSuggestion()
    {
        _optimizationTarget = null;
        _optimizationSuggestion = null;
        _optimizationBaseDigest = null;
        NotifyOptimizationChanged();
    }

    private bool CanOptimize(StoryOptimizationTarget target)
    {
        if (IsBusy || !_planningRunsReady || _current is null || ProjectTextModel is null ||
            Segments.Count > 0 || ResumablePlanningRun is not null)
            return false;
        var value = target == StoryOptimizationTarget.Source ? ProjectSource : VisualStyle;
        return !string.IsNullOrWhiteSpace(value);
    }

    private async Task OptimizeAsync(
        StoryOptimizationTarget target,
        CancellationToken cancellationToken)
    {
        if (!CanOptimize(target) || ProjectTextModel is null || _current is null) return;
        var session = _session;
        var projectId = _current.Id;
        var digest = CurrentPlanningDigest();
        var request = new StoryOptimizationRequest(
            ProjectTextModel.Id,
            ProjectTitle.Trim(),
            ProjectDescription.Trim(),
            ProjectSource.Trim(),
            VisualStyle.Trim(),
            target);
        IsBusy = true;
        ErrorMessage = null;
        StatusMessage = target == StoryOptimizationTarget.Source
            ? "文本模型正在生成剧情原文优化候选…"
            : "文本模型正在生成画面风格优化候选…";
        try
        {
            var suggestion = await _planner.OptimizeAsync(request, cancellationToken);
            if (_session != session || _current?.Id != projectId)
                return;
            if (digest != CurrentPlanningDigest())
                throw new InvalidOperationException("项目内容已改变，本次优化建议未覆盖当前编辑内容。");
            _optimizationTarget = target;
            _optimizationSuggestion = suggestion;
            _optimizationBaseDigest = digest;
            NotifyOptimizationChanged();
            StatusMessage = "AI 优化候选已生成，确认采用前不会覆盖当前内容";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
            StatusMessage = "AI 优化失败，当前内容未被修改";
        }
        finally
        {
            IsBusy = false;
        }
    }

    private void NotifyOptimizationChanged()
    {
        OnPropertyChanged(nameof(HasOptimizationSuggestion));
        OnPropertyChanged(nameof(CanOptimizeStorySource));
        OnPropertyChanged(nameof(CanOptimizeVisualStyle));
        OnPropertyChanged(nameof(CanApplyOptimization));
        OnPropertyChanged(nameof(OptimizationTitle));
        OnPropertyChanged(nameof(OptimizationText));
        OnPropertyChanged(nameof(OptimizationRationale));
    }
}
