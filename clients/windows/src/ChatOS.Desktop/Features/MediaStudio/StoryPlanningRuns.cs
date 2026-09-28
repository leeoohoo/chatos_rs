using System.Collections.ObjectModel;
using System.Security.Cryptography;
using System.Text;
using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel
{
    private bool _planningRunsReady;

    public ObservableCollection<StoryPlanningRunCard> PlanningRuns { get; } = [];

    public StoryPlanningRunDocument? ResumablePlanningRun => PlanningRuns
        .Select(card => card.Run)
        .FirstOrDefault(run => run.CanResume);

    public bool CanResumePlanning => !IsBusy && _planningRunsReady && _current is not null &&
        Segments.Count == 0 && Resources.Count == 0 &&
        ResumablePlanningRun is { } run && run.BaseDigest == CurrentPlanningDigest();
    public bool CanAbandonPlanning => !IsBusy && _planningRunsReady && ResumablePlanningRun is not null;
    public string PlanningRunSummary => !_planningRunsReady
        ? "规划记录 · 正在读取…"
        : PlanningRuns.Count == 0
        ? "规划记录 · 暂无记录"
        : $"规划记录 · {PlanningRuns.Count} 次 · {(ResumablePlanningRun is null ? "没有待恢复" : "有待恢复运行")}";
    public string ResumePlanningLabel => ResumablePlanningRun switch
    {
        { Draft: not null } => "应用已保存草稿（不调用模型）",
        { Status: StoryPlanningRunStatus.Failed } => "重试失败的规划（会调用模型）",
        { } => "恢复规划（可能调用模型）",
        _ => "没有可恢复的规划",
    };

    private async Task StartPlanningRunAsync(CancellationToken cancellationToken)
    {
        var owner = _ownerUserId;
        var project = _current;
        var textModel = ProjectTextModel;
        if (!CanPlan || owner is null || project is null || textModel is null) return;
        var now = DateTimeOffset.UtcNow;
        var request = new StoryPlanningRequest(
            textModel.Id,
            ProjectTitle.Trim(),
            ProjectDescription.Trim(),
            ProjectSource.Trim(),
            VisualStyle.Trim(),
            ProjectRatio);
        var run = new StoryPlanningRunDocument(
            Guid.NewGuid(), project.Id, CurrentPlanningDigest(), request,
            StoryPlanningRunStatus.Running, null, null, now, now);
        await ExecutePlanningRunAsync(owner, run, cancellationToken);
    }

    public async Task ResumePlanningAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        var run = ResumablePlanningRun;
        if (!CanResumePlanning || owner is null || run is null) return;
        await ExecutePlanningRunAsync(owner, run, cancellationToken);
    }

    public async Task AbandonPlanningAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        var run = ResumablePlanningRun;
        if (!CanAbandonPlanning || owner is null || run is null) return;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var abandoned = run with
            {
                Status = StoryPlanningRunStatus.Abandoned,
                Error = "用户已放弃；正式项目未被修改。",
                UpdatedAt = DateTimeOffset.UtcNow,
            };
            await _store.SavePlanningRunAsync(owner, abandoned, cancellationToken);
            PublishPlanningRun(abandoned);
            StatusMessage = "已放弃中断的规划记录，正式项目保持不变";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = $"放弃规划记录失败：{exception.Message}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    private async Task ExecutePlanningRunAsync(
        string owner,
        StoryPlanningRunDocument initial,
        CancellationToken cancellationToken)
    {
        var run = initial with
        {
            Status = StoryPlanningRunStatus.Running,
            Error = null,
            UpdatedAt = DateTimeOffset.UtcNow,
        };
        var applied = false;
        IsBusy = true;
        ErrorMessage = null;
        StatusMessage = run.Draft is null
            ? "文本模型正在生成可恢复的全剧规划…"
            : "正在应用已保存的规划草稿，不会再次调用模型…";
        try
        {
            await _store.SavePlanningRunAsync(owner, run, cancellationToken);
            PublishPlanningRun(run);
            var result = run.Draft;
            if (result is null)
            {
                result = await _planner.PlanAsync(run.Request, cancellationToken);
                run = run with
                {
                    Draft = result,
                    Status = StoryPlanningRunStatus.DraftReady,
                    UpdatedAt = DateTimeOffset.UtcNow,
                };
                await _store.SavePlanningRunAsync(owner, run, cancellationToken);
                PublishPlanningRun(run);
            }
            EnsurePlanningRunCanApply(run);
            await ApplyPlanningDraftAsync(result, cancellationToken);
            applied = true;
            run = run with
            {
                Status = StoryPlanningRunStatus.Applied,
                Error = null,
                UpdatedAt = DateTimeOffset.UtcNow,
            };
            await _store.SavePlanningRunAsync(owner, run, cancellationToken);
            PublishPlanningRun(run);
            StatusMessage = $"AI 已完成全剧规划，共 {Segments.Count} 个分段";
        }
        catch (OperationCanceledException)
        {
            run = run with
            {
                Status = applied ? StoryPlanningRunStatus.Applied : StoryPlanningRunStatus.Paused,
                Error = applied ? null : "规划已暂停，可从本机记录恢复。",
                UpdatedAt = DateTimeOffset.UtcNow,
            };
            await SavePlanningRunBestEffortAsync(owner, run);
            StatusMessage = applied ? "规划已应用" : "规划已暂停，可稍后恢复";
        }
        catch (Exception exception)
        {
            run = run with
            {
                Status = applied ? StoryPlanningRunStatus.Applied : StoryPlanningRunStatus.Failed,
                Error = exception.Message,
                UpdatedAt = DateTimeOffset.UtcNow,
            };
            await SavePlanningRunBestEffortAsync(owner, run);
            ErrorMessage = exception.Message;
            StatusMessage = applied
                ? "规划已应用，但运行记录更新失败"
                : "规划失败，运行记录和已有草稿已保留";
        }
        finally
        {
            IsBusy = false;
            NotifyPlanningRunsChanged();
        }
    }

    private async Task ApplyPlanningDraftAsync(
        StoryPlanningResult result,
        CancellationToken cancellationToken)
    {
        if (Segments.Count > 0 || Resources.Count > 0)
            throw new InvalidOperationException("当前项目已有分段或素材，不能覆盖应用旧规划草稿。");
        var previousSummary = ProjectSummary;
        try
        {
            ProjectSummary = result.Summary;
            foreach (var resource in result.Resources)
            {
                Resources.Add(new StoryResourceEditor(new StoryResourceDocument(
                    resource.Id,
                    resource.Kind switch
                    {
                        "character" => StoryResourceKind.Character,
                        "scene" => StoryResourceKind.Scene,
                        _ => StoryResourceKind.Prop,
                    },
                    resource.Name, resource.Description, resource.ImagePrompt, null), _ => null));
            }
            foreach (var plan in result.Segments)
            {
                Segments.Add(new StorySegmentEditor(new StorySegmentDocument(
                    $"segment-{Guid.NewGuid():N}", plan.Title, plan.Narrative,
                    plan.ImagePrompt, plan.VideoPrompt, plan.Seconds, null, null, null)
                {
                    Kind = plan.Kind == "transition" ? StorySegmentKind.Transition : StorySegmentKind.Story,
                    ResourceIds = plan.ResourceIds,
                }, _ => null));
            }
            SelectedSegment = Segments.FirstOrDefault();
            SelectedResource = Resources.FirstOrDefault();
            FillMissingContinuity();
            await PersistCurrentAsync(cancellationToken);
        }
        catch
        {
            Segments.Clear();
            Resources.Clear();
            ProjectSummary = previousSummary;
            throw;
        }
    }

    private void EnsurePlanningRunCanApply(StoryPlanningRunDocument run)
    {
        if (_current?.Id != run.ProjectId || run.BaseDigest != CurrentPlanningDigest())
            throw new InvalidOperationException("项目内容已改变，不能覆盖应用旧规划；请放弃记录后重新规划。");
    }

    private async Task LoadPlanningRunsAsync(string owner, Guid projectId, Guid session)
    {
        try
        {
            var runs = await _store.LoadPlanningRunsAsync(owner, projectId);
            if (_session != session || _ownerUserId != owner || _current?.Id != projectId) return;
            PlanningRuns.Clear();
            foreach (var run in runs) PlanningRuns.Add(new StoryPlanningRunCard(run));
        }
        catch (Exception exception)
        {
            if (_session == session && _current?.Id == projectId)
                ErrorMessage = $"读取剧情规划记录失败：{exception.Message}";
        }
        finally
        {
            if (_session == session && _ownerUserId == owner && _current?.Id == projectId)
            {
                _planningRunsReady = true;
                NotifyPlanningRunsChanged();
            }
        }
    }

    private async Task SavePlanningRunBestEffortAsync(string owner, StoryPlanningRunDocument run)
    {
        try { await _store.SavePlanningRunAsync(owner, run); }
        catch { /* The project or provider error remains the primary user-facing failure. */ }
        PublishPlanningRun(run);
    }

    private void PublishPlanningRun(StoryPlanningRunDocument run)
    {
        var index = PlanningRuns.ToList().FindIndex(card => card.Id == run.Id);
        if (index >= 0) PlanningRuns.RemoveAt(index);
        PlanningRuns.Insert(0, new StoryPlanningRunCard(run));
        NotifyPlanningRunsChanged();
    }

    private string CurrentPlanningDigest()
    {
        var value = string.Join('\u001f',
            _current?.Id.ToString() ?? string.Empty,
            ProjectTitle.Trim(),
            ProjectDescription.Trim(),
            ProjectSource.Trim(),
            VisualStyle.Trim(),
            ProjectRatio,
            ProjectTextModel?.Id ?? string.Empty);
        return Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant();
    }

    private void NotifyPlanningRunsChanged()
    {
        OnPropertyChanged(nameof(ResumablePlanningRun));
        OnPropertyChanged(nameof(CanResumePlanning));
        OnPropertyChanged(nameof(CanAbandonPlanning));
        OnPropertyChanged(nameof(PlanningRunSummary));
        OnPropertyChanged(nameof(ResumePlanningLabel));
        OnPropertyChanged(nameof(CanQuickSplit));
        OnPropertyChanged(nameof(CanPlan));
        OnPropertyChanged(nameof(CanOptimizeStorySource));
        OnPropertyChanged(nameof(CanOptimizeVisualStyle));
    }
}
