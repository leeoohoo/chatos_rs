using System.Security.Cryptography;
using System.Text;
using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel
{
    public bool CanResumeSelectedVideo => !IsBusy && SelectedSegment is { } segment &&
        ProjectVideoModel is { } model && segment.HasPendingVideoJob &&
        string.IsNullOrWhiteSpace(segment.VideoPath) && VideoJobMatches(segment, model);
    public bool CanAbandonSelectedVideoJob => !IsBusy && SelectedSegment?.HasPendingVideoJob == true;
    public string SelectedVideoJobLabel
    {
        get
        {
            var segment = SelectedSegment;
            if (segment is null || !segment.HasPendingVideoJob) return string.Empty;
            return VideoJobMatches(segment, ProjectVideoModel)
                ? $"已有任务 {ShortJobId(segment.PendingVideoJobId)} · {segment.PendingVideoJobStatus ?? "等待查询"}；可继续查询并下载，不会重复提交。"
                : $"已有任务 {ShortJobId(segment.PendingVideoJobId)}，但模型、提示词、时长或帧已改变；为避免重复扣费，已禁止重新提交。";
        }
    }

    public Task GenerateVideoAsync(CancellationToken cancellationToken = default) =>
        RunSelectedVideoAsync(false, cancellationToken);

    public Task ResumeSelectedVideoAsync(CancellationToken cancellationToken = default) =>
        RunSelectedVideoAsync(true, cancellationToken);

    public async Task AbandonSelectedVideoJobAsync(CancellationToken cancellationToken = default)
    {
        var segment = SelectedSegment;
        if (!CanAbandonSelectedVideoJob || segment is null) return;
        var id = segment.PendingVideoJobId!;
        var status = segment.PendingVideoJobStatus ?? "unknown";
        var digest = segment.PendingVideoRequestDigest!;
        segment.ClearPendingVideoJob();
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            await PersistCurrentAsync(cancellationToken);
            StatusMessage = "已清除本机视频任务记录，可以重新提交视频";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            segment.SetPendingVideoJob(id, status, digest);
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsBusy = false;
            NotifyVideoJobChanged();
        }
    }

    private async Task RunSelectedVideoAsync(bool resume, CancellationToken cancellationToken)
    {
        var context = CaptureGenerationContext();
        var model = ProjectVideoModel;
        if (context is null || model is null || (resume && !CanResumeSelectedVideo) ||
            (!resume && !CanGenerateVideo)) return;
        _generationCancellation?.Cancel();
        _generationCancellation?.Dispose();
        _generationCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var token = _generationCancellation.Token;
        IsBusy = true;
        ErrorMessage = null;
        VideoProgress = new VideoGenerationProgress(resume ? "querying" : "submitting");
        try
        {
            await PersistCurrentAsync(token);
            await GenerateVideoCoreAsync(context, model, token);
            StatusMessage = $"{context.Segment.Title} 的视频已生成";
        }
        catch (OperationCanceledException)
        {
            VideoProgress = null;
            StatusMessage = context.Segment.HasPendingVideoJob
                ? "已停止等待；任务记录已保存，稍后可继续查询和下载"
                : "已停止等待剧情视频";
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
            VideoProgress = new VideoGenerationProgress("failed");
        }
        finally
        {
            IsBusy = false;
            NotifyVideoJobChanged();
        }
    }

    private async Task GenerateVideoCoreAsync(
        GenerationContext context,
        MediaGenerationModel videoModel,
        CancellationToken cancellationToken)
    {
        EnsureContext(context);
        var profile = VideoGenerationProfile.ForModel(videoModel.ModelName);
        var seconds = profile.Durations.OrderBy(value => Math.Abs(value - context.Segment.Seconds)).First();
        context.Segment.Seconds = seconds;
        var prompt = StoryPromptCatalog.RenderVideo(
            context.Segment.VideoPrompt,
            BuildContinuityContext(context.Segment),
            CreativeRequirements);
        var first = await LoadFrameAsync(context.Segment.FirstFramePath, cancellationToken);
        var last = profile.SupportsLastFrame
            ? await LoadFrameAsync(context.Segment.LastFramePath, cancellationToken)
            : null;
        var request = new VideoGenerationRequest(
            videoModel.Id, prompt, profile.Sizes[0], seconds, first, last, null, ProjectRatio);
        var digest = VideoRequestDigest(context.Segment, videoModel, profile, prompt, seconds);
        var progress = new Progress<VideoGenerationProgress>(value =>
            HandleVideoProgress(context, digest, value));
        VideoGenerationResult result;
        if (context.Segment.HasPendingVideoJob)
        {
            if (!string.Equals(context.Segment.PendingVideoRequestDigest, digest, StringComparison.Ordinal))
                throw new InvalidOperationException("视频任务对应的模型、提示词、时长或帧已改变。请保留记录并恢复原设置，或明确清除任务记录后再提交。");
            result = await _media.ResumeVideoAsync(
                request, context.Segment.PendingVideoJobId!, progress, cancellationToken);
        }
        else
        {
            result = await _media.GenerateVideoAsync(request, progress, cancellationToken);
        }
        var history = await _history.SaveVideoAsync(context.Owner, prompt, result, cancellationToken);
        var relative = await _store.ImportAssetAsync(
            context.Owner, context.ProjectId, context.Segment.Id, history.FilePath, true, cancellationToken);
        EnsureContext(context);
        context.Segment.SetVideo(
            relative, _store.ResolveAssetPath(context.Owner, context.ProjectId, relative)!);
        await PersistCurrentAsync(cancellationToken);
        try
        {
            await ExtractVideoLastFrameCoreAsync(context, context.Segment.VideoPath!, cancellationToken);
            await PersistCurrentAsync(cancellationToken);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = $"视频已保存，但无法自动提取成片末帧：{exception.Message}";
        }
        VideoProgress = new VideoGenerationProgress("completed", 100, result.Id);
        NotifyBatchPlanChanged();
        NotifyVideoJobChanged();
    }

    private void HandleVideoProgress(
        GenerationContext context,
        string digest,
        VideoGenerationProgress value)
    {
        VideoProgress = value;
        if (string.IsNullOrWhiteSpace(value.JobId)) return;
        try
        {
            EnsureContext(context);
            var status = string.IsNullOrWhiteSpace(value.Status) ? "unknown" : value.Status;
            if (context.Segment.SetPendingVideoJob(value.JobId, status, digest))
                _ = PersistPendingVideoJobAsync(context);
            NotifyVideoJobChanged();
        }
        catch (OperationCanceledException)
        {
            // Ignore progress posted after the user changed project or account.
        }
    }

    private async Task PersistPendingVideoJobAsync(GenerationContext context)
    {
        try
        {
            EnsureContext(context);
            await PersistCurrentAsync(CancellationToken.None);
        }
        catch (Exception exception)
        {
            if (_session == context.Session && _current?.Id == context.ProjectId)
                ErrorMessage = $"保存视频任务记录失败：{exception.Message}";
        }
    }

    private bool VideoJobMatches(StorySegmentEditor segment, MediaGenerationModel? model)
    {
        if (model is null || string.IsNullOrWhiteSpace(segment.PendingVideoRequestDigest)) return false;
        var profile = VideoGenerationProfile.ForModel(model.ModelName);
        var seconds = profile.Durations.OrderBy(value => Math.Abs(value - segment.Seconds)).First();
        var prompt = StoryPromptCatalog.RenderVideo(
            segment.VideoPrompt, BuildContinuityContext(segment), CreativeRequirements);
        return string.Equals(
            segment.PendingVideoRequestDigest,
            VideoRequestDigest(segment, model, profile, prompt, seconds),
            StringComparison.Ordinal);
    }

    private string VideoRequestDigest(
        StorySegmentEditor segment,
        MediaGenerationModel model,
        VideoGenerationProfile profile,
        string prompt,
        int seconds)
    {
        var document = segment.ToDocument();
        var value = string.Join('\u001f',
            model.Id, profile.Sizes[0], seconds.ToString(), ProjectRatio, prompt,
            document.FirstFrameAsset ?? string.Empty,
            profile.SupportsLastFrame ? document.LastFrameAsset ?? string.Empty : string.Empty);
        return Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant();
    }

    private void NotifyVideoJobChanged()
    {
        OnPropertyChanged(nameof(CanGenerateVideo));
        OnPropertyChanged(nameof(CanResumeSelectedVideo));
        OnPropertyChanged(nameof(CanAbandonSelectedVideoJob));
        OnPropertyChanged(nameof(SelectedVideoJobLabel));
    }

    private static string ShortJobId(string? value) => string.IsNullOrWhiteSpace(value)
        ? "未知"
        : value[..Math.Min(value.Length, 16)];
}
