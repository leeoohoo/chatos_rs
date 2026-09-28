using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel
{
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanStartBatch))]
    [NotifyPropertyChangedFor(nameof(BatchPlanLabel))]
    private bool _batchRefinements = true;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanStartBatch))]
    [NotifyPropertyChangedFor(nameof(BatchPlanLabel))]
    private bool _batchResources = true;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanStartBatch))]
    [NotifyPropertyChangedFor(nameof(BatchPlanLabel))]
    private bool _batchFirstFrames = true;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanStartBatch))]
    [NotifyPropertyChangedFor(nameof(BatchPlanLabel))]
    private bool _batchLastFrames;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanStartBatch))]
    [NotifyPropertyChangedFor(nameof(BatchPlanLabel))]
    private bool _batchVideos;

    [ObservableProperty] private bool _isBatchRunning;
    [ObservableProperty] private int _batchCompleted;
    [ObservableProperty] private int _batchFailed;
    [ObservableProperty] private int _batchSkipped;
    [ObservableProperty] private int _batchTotal;
    [ObservableProperty] private string _batchCurrentLabel = string.Empty;

    public bool CanStartBatch => !IsBusy && _current is not null &&
        ProjectImageModel is not null && ProjectVideoModel is not null &&
        CreateBatchWork().Any(item => item.CanRun);

    public string BatchPlanLabel
    {
        get
        {
            var work = CreateBatchWork();
            if (work.Count == 0) return "所选阶段没有缺失项，当前已全部完成。";
            var runnable = work.Count(item => item.CanRun);
            var skipped = work.Count - runnable;
            var calls = runnable == 0 ? "没有可执行的模型调用" : $"预计调用模型 {runnable} 次";
            var continuity = BatchVideos
                ? "；含视频的批次会逐段完成，以便下一段继承上一段成片末帧"
                : string.Empty;
            return skipped == 0
                ? $"{calls}；只生成缺失项，每完成一项立即保存{continuity}。"
                : $"{calls}；另有 {skipped} 项缺少提示词，将跳过{continuity}。";
        }
    }

    public string BatchProgressLabel => BatchTotal == 0
        ? string.Empty
        : $"{BatchCompleted + BatchFailed + BatchSkipped}/{BatchTotal} · 成功 {BatchCompleted} · 失败 {BatchFailed} · 跳过 {BatchSkipped}";

    public async Task RunBatchAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        var project = _current;
        var imageModel = ProjectImageModel;
        var videoModel = ProjectVideoModel;
        var textModel = ProjectTextModel;
        var session = _session;
        if (!CanStartBatch || owner is null || project is null || imageModel is null ||
            videoModel is null || textModel is null) return;

        _generationCancellation?.Cancel();
        _generationCancellation?.Dispose();
        var source = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _generationCancellation = source;
        var token = source.Token;
        IsBusy = true;
        IsBatchRunning = true;
        ErrorMessage = null;
        BatchCompleted = 0;
        BatchFailed = 0;
        BatchSkipped = 0;
        try
        {
            ClearSegmentRefinement();
            await PersistCurrentAsync(token);
            var work = CreateBatchWork();
            BatchTotal = work.Count;
            NotifyBatchProgressChanged();
            for (var index = 0; index < work.Count; index++)
            {
                token.ThrowIfCancellationRequested();
                EnsureBatchContext(owner, project.Id, session);
                var item = work[index];
                BatchCurrentLabel = $"{index + 1}/{work.Count} · {item.Label}";
                StatusMessage = $"批量生产：{BatchCurrentLabel}";
                if (!item.CanRun)
                {
                    BatchSkipped++;
                    NotifyBatchProgressChanged();
                    continue;
                }

                try
                {
                    await ExecuteBatchItemAsync(
                        item, owner, project.Id, session, textModel, imageModel, videoModel, token);
                    BatchCompleted++;
                }
                catch (OperationCanceledException)
                {
                    throw;
                }
                catch (Exception exception)
                {
                    BatchFailed++;
                    ErrorMessage = $"{item.Label}失败：{exception.Message}";
                }
                NotifyBatchProgressChanged();
            }

            BatchCurrentLabel = string.Empty;
            StatusMessage = BatchFailed == 0
                ? $"批量生产完成，共完成 {BatchCompleted} 项"
                : $"批量生产结束：完成 {BatchCompleted}，失败 {BatchFailed}；再次启动会继续未完成项";
        }
        catch (OperationCanceledException)
        {
            BatchCurrentLabel = string.Empty;
            StatusMessage = $"批量生产已停止；已完成 {BatchCompleted} 项，再次启动会继续未完成项";
        }
        finally
        {
            if (ReferenceEquals(_generationCancellation, source))
            {
                _generationCancellation = null;
                source.Dispose();
            }
            IsBatchRunning = false;
            IsBusy = false;
            NotifyBatchPlanChanged();
            NotifyBatchProgressChanged();
        }
    }

    private async Task ExecuteBatchItemAsync(
        BatchWorkItem item,
        string owner,
        Guid projectId,
        Guid session,
        MediaGenerationModel textModel,
        MediaGenerationModel imageModel,
        MediaGenerationModel videoModel,
        CancellationToken cancellationToken)
    {
        switch (item.Kind)
        {
            case BatchWorkKind.Refinement:
                await RefineSegmentCoreAsync(
                    item.Segment!, owner, projectId, session, textModel, cancellationToken);
                break;
            case BatchWorkKind.Resource:
                await GenerateResourceImageCoreAsync(
                    new ResourceGenerationContext(owner, projectId, item.Resource!, session),
                    imageModel,
                    cancellationToken);
                break;
            case BatchWorkKind.FirstFrame:
                await GenerateFrameCoreAsync(
                    new GenerationContext(owner, projectId, item.Segment!, session),
                    imageModel,
                    false,
                    cancellationToken);
                break;
            case BatchWorkKind.LastFrame:
                await GenerateFrameCoreAsync(
                    new GenerationContext(owner, projectId, item.Segment!, session),
                    imageModel,
                    true,
                    cancellationToken);
                break;
            case BatchWorkKind.Video:
                await GenerateVideoCoreAsync(
                    new GenerationContext(owner, projectId, item.Segment!, session),
                    videoModel,
                    cancellationToken);
                break;
        }
    }

    private List<BatchWorkItem> CreateBatchWork()
    {
        if (_current is null) return [];
        var work = new List<BatchWorkItem>();
        if (BatchRefinements)
        {
            work.AddRange(Segments
                .Where(segment => !segment.IsRefined && SegmentHasNoMedia(segment))
                .Select(segment => new BatchWorkItem(
                    BatchWorkKind.Refinement,
                    $"镜头细化 · {segment.NumberLabel} {segment.Title}",
                    !string.IsNullOrWhiteSpace(segment.Narrative),
                    null,
                    segment)));
        }
        if (BatchResources)
        {
            work.AddRange(Resources
                .Where(resource => string.IsNullOrWhiteSpace(resource.ImagePath))
                .Select(resource => new BatchWorkItem(
                    BatchWorkKind.Resource,
                    $"参考图 · {resource.KindLabel}“{resource.Name}”",
                    !string.IsNullOrWhiteSpace(resource.ImagePrompt),
                    resource,
                    null)));
        }
        if (BatchVideos)
        {
            AddContinuousSegmentWork(work);
        }
        else
        {
            AddSegmentWork(work, BatchFirstFrames, BatchWorkKind.FirstFrame, "首帧",
                segment => segment.FirstFramePath, segment => segment.ImagePrompt);
            AddSegmentWork(work, BatchLastFrames, BatchWorkKind.LastFrame, "尾帧",
                segment => segment.LastFramePath, segment => segment.ImagePrompt);
        }
        return work;
    }

    private void AddContinuousSegmentWork(ICollection<BatchWorkItem> work)
    {
        foreach (var segment in Segments)
        {
            AddSegmentWorkItem(work, segment, BatchFirstFrames, BatchWorkKind.FirstFrame, "首帧",
                segment.FirstFramePath, segment.ImagePrompt);
            AddSegmentWorkItem(work, segment, BatchLastFrames, BatchWorkKind.LastFrame, "尾帧",
                segment.LastFramePath, segment.ImagePrompt);
            AddSegmentWorkItem(work, segment, true, BatchWorkKind.Video, "视频",
                segment.VideoPath, segment.VideoPrompt);
        }
    }

    private void AddSegmentWork(
        ICollection<BatchWorkItem> work,
        bool enabled,
        BatchWorkKind kind,
        string label,
        Func<StorySegmentEditor, string?> asset,
        Func<StorySegmentEditor, string> prompt)
    {
        if (!enabled) return;
        foreach (var segment in Segments)
            AddSegmentWorkItem(work, segment, true, kind, label, asset(segment), prompt(segment));
    }

    private static void AddSegmentWorkItem(
        ICollection<BatchWorkItem> work,
        StorySegmentEditor segment,
        bool enabled,
        BatchWorkKind kind,
        string label,
        string? asset,
        string prompt)
    {
        if (!enabled || !string.IsNullOrWhiteSpace(asset)) return;
        work.Add(new BatchWorkItem(
            kind,
            $"{label} · {segment.NumberLabel} {segment.Title}",
            !string.IsNullOrWhiteSpace(prompt),
            null,
            segment));
    }

    private void EnsureBatchContext(string owner, Guid projectId, Guid session)
    {
        if (_ownerUserId != owner || _current?.Id != projectId || _session != session)
            throw new OperationCanceledException("剧情项目或登录账户已切换。");
    }

    private void NotifyBatchPlanChanged()
    {
        OnPropertyChanged(nameof(CanStartBatch));
        OnPropertyChanged(nameof(BatchPlanLabel));
    }

    private void NotifyBatchProgressChanged() => OnPropertyChanged(nameof(BatchProgressLabel));

    partial void OnBatchResourcesChanged(bool value) => NotifyBatchPlanChanged();
    partial void OnBatchRefinementsChanged(bool value) => NotifyBatchPlanChanged();
    partial void OnBatchFirstFramesChanged(bool value) => NotifyBatchPlanChanged();
    partial void OnBatchLastFramesChanged(bool value) => NotifyBatchPlanChanged();
    partial void OnBatchVideosChanged(bool value) => NotifyBatchPlanChanged();

    private enum BatchWorkKind
    {
        Refinement,
        Resource,
        FirstFrame,
        LastFrame,
        Video,
    }

    private sealed record BatchWorkItem(
        BatchWorkKind Kind,
        string Label,
        bool CanRun,
        StoryResourceEditor? Resource,
        StorySegmentEditor? Segment);
}
