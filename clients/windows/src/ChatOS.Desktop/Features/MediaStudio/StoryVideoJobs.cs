using System.Security.Cryptography;
using System.Text;
using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel
{
    public bool CanResumeSelectedVideo => !IsBusy && SelectedSegment is { } segment &&
        ProjectVideoModel is { } model && segment.HasPendingVideoJob &&
        VideoJobMatches(segment, model);
    public bool CanAbandonSelectedVideoJob => !IsBusy && SelectedSegment?.HasPendingVideoJob == true;
    public bool CanRegenerateFromCurrentVideo => !IsBusy &&
        SelectedSegment?.VideoPath is { Length: > 0 } path && ReferenceVideoCanLoad(path) &&
        ProjectVideoModel is { } model && VideoGenerationProfile.ForModel(model.ModelName).SupportsReferenceVideo;
    public bool CanGenerateFromPreviousVideo => CanGenerateVideo && SelectedSegment is { } segment &&
        string.IsNullOrWhiteSpace(segment.VideoPath) && !segment.HasPendingVideoJob &&
        ProjectVideoModel is { } model && CanUsePreviousVideo(segment, model);
    public string PreviousVideoGenerationSummary => SelectedSegment is { } segment &&
        PreviousSegment(segment) is { } previous
        ? $"{segment.NumberLabel} · {segment.Title} 可以把上一段“{previous.Title}”的完整成片作为延续参考。选择延续模式会从上一段结束时的运动、人物状态和光线自然接入；也可以只使用本段首尾帧。"
        : string.Empty;
    public string GenerateVideoActionLabel => SelectedSegment?.VideoPath is { Length: > 0 }
        ? "准备重新生成该段视频"
        : "生成该段视频";
    public string VideoRegenerationSummary
    {
        get
        {
            if (SelectedSegment is not { } segment || ProjectVideoModel is not { } model)
                return string.Empty;
            var guidance = CanRegenerateFromCurrentVideo
                ? "可选择参考当前成片重做，或仅按首尾帧重做。"
                : "当前模型或视频不支持原视频参考，将按首尾帧重做。";
            return $"将使用“{model.Name}”重新生成 {segment.NumberLabel} · {segment.Title}（约 {segment.Seconds} 秒）。这会再次调用视频模型；当前视频和成片末帧会进入历史版本，可随时恢复。{guidance}";
        }
    }
    public string SelectedVideoJobLabel
    {
        get
        {
            var segment = SelectedSegment;
            if (segment is null || !segment.HasPendingVideoJob) return string.Empty;
            return VideoJobMatches(segment, ProjectVideoModel)
                ? $"已有任务 {ShortJobId(segment.PendingVideoJobId)} · {segment.PendingVideoJobStatus ?? "等待查询"}；可继续查询并下载，不会重复提交。"
                : $"已有任务 {ShortJobId(segment.PendingVideoJobId)}，但模型、提示词、时长或参考输入已改变；为避免重复扣费，已禁止重新提交。";
        }
    }

    public Task GenerateVideoAsync(CancellationToken cancellationToken = default) =>
        RunSelectedVideoAsync(false, false, StoryVideoGuidance.Frames, null, cancellationToken);

    public Task GenerateVideoFromPreviousAsync(
        string expectedSegmentId,
        CancellationToken cancellationToken = default) =>
        RunSelectedVideoAsync(
            false, false, StoryVideoGuidance.PreviousVideo, expectedSegmentId, cancellationToken);

    public Task RegenerateSelectedVideoAsync(
        string confirmedSegmentId,
        bool useOriginalVideo,
        CancellationToken cancellationToken = default) =>
        RunSelectedVideoAsync(
            false, true,
            useOriginalVideo ? StoryVideoGuidance.SourceVideo : StoryVideoGuidance.Frames,
            confirmedSegmentId, cancellationToken);

    public Task ResumeSelectedVideoAsync(CancellationToken cancellationToken = default) =>
        RunSelectedVideoAsync(true, false, StoryVideoGuidance.Frames, null, cancellationToken);

    public async Task AbandonSelectedVideoJobAsync(CancellationToken cancellationToken = default)
    {
        var segment = SelectedSegment;
        if (!CanAbandonSelectedVideoJob || segment is null) return;
        var id = segment.PendingVideoJobId!;
        var status = segment.PendingVideoJobStatus ?? "unknown";
        var digest = segment.PendingVideoRequestDigest!;
        var guidance = segment.PendingVideoGuidance;
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
            segment.SetPendingVideoJob(id, status, digest, guidance);
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsBusy = false;
            NotifyVideoJobChanged();
        }
    }

    private async Task RunSelectedVideoAsync(
        bool resume,
        bool confirmedRegeneration,
        StoryVideoGuidance requestedGuidance,
        string? expectedSegmentId,
        CancellationToken cancellationToken)
    {
        var context = CaptureGenerationContext();
        var model = ProjectVideoModel;
        if (context is null || model is null || (resume && !CanResumeSelectedVideo) ||
            (!resume && !CanGenerateVideo)) return;
        if (expectedSegmentId is not null && expectedSegmentId != context.Segment.Id) return;
        if (!resume && context.Segment.VideoPath is { Length: > 0 })
        {
            if (!confirmedRegeneration || expectedSegmentId != context.Segment.Id)
            {
                StatusMessage = "该分段已有完成视频，请先确认重新生成；当前版本不会被直接覆盖。";
                return;
            }
        }
        var guidance = resume
            ? ParseGuidance(context.Segment.PendingVideoGuidance)
            : requestedGuidance;
        if (guidance == StoryVideoGuidance.SourceVideo && !CanUseSourceVideo(context.Segment, model))
        {
            ErrorMessage = "当前模型或视频不支持参考原成片重新生成；请选择仅按首尾帧重做。";
            return;
        }
        if (guidance == StoryVideoGuidance.PreviousVideo &&
            !CanUsePreviousVideo(context.Segment, model))
        {
            ErrorMessage = "上一段成片或当前模型不支持视频延续；请选择仅按本段首尾帧生成。";
            return;
        }
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
            await GenerateVideoCoreAsync(context, model, guidance, token);
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
        StoryVideoGuidance guidance,
        CancellationToken cancellationToken)
    {
        EnsureContext(context);
        var profile = VideoGenerationProfile.ForModel(videoModel.ModelName);
        var seconds = profile.Durations.OrderBy(value => Math.Abs(value - context.Segment.Seconds)).First();
        context.Segment.Seconds = seconds;
        var prompt = BuildVideoPrompt(context.Segment, guidance);
        var referencePath = guidance switch
        {
            StoryVideoGuidance.SourceVideo => context.Segment.VideoPath,
            StoryVideoGuidance.PreviousVideo => PreviousSegment(context.Segment)?.VideoPath,
            _ => null,
        };
        var referenceVideo = referencePath is null
            ? null
            : await LoadVideoReferenceAsync(referencePath, cancellationToken);
        var first = referenceVideo is null
            ? await LoadFrameAsync(context.Segment.FirstFramePath, cancellationToken)
            : null;
        var last = referenceVideo is null && profile.SupportsLastFrame
            ? await LoadFrameAsync(context.Segment.LastFramePath, cancellationToken)
            : null;
        var request = new VideoGenerationRequest(
            videoModel.Id, prompt, profile.Sizes[0], seconds, first, last, null, ProjectRatio);
        request = request with
        {
            ReferenceVideo = referenceVideo,
            ReferencePurpose = referenceVideo is null
                ? VideoGenerationReferencePurpose.Reference
                : guidance == StoryVideoGuidance.PreviousVideo
                    ? VideoGenerationReferencePurpose.Extend
                    : VideoGenerationReferencePurpose.Edit,
        };
        var digest = VideoRequestDigest(context.Segment, videoModel, profile, prompt, seconds, guidance);
        var progress = new Progress<VideoGenerationProgress>(value =>
            HandleVideoProgress(context, digest, guidance, value));
        VideoGenerationResult result;
        if (context.Segment.HasPendingVideoJob)
        {
            if (!VideoRequestDigestMatches(
                    context.Segment.PendingVideoRequestDigest!, context.Segment, videoModel,
                    profile, prompt, seconds, guidance))
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
        StoryVideoGuidance guidance,
        VideoGenerationProgress value)
    {
        VideoProgress = value;
        if (string.IsNullOrWhiteSpace(value.JobId)) return;
        try
        {
            EnsureContext(context);
            var status = string.IsNullOrWhiteSpace(value.Status) ? "unknown" : value.Status;
            if (context.Segment.SetPendingVideoJob(value.JobId, status, digest, GuidanceKey(guidance)))
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
        var guidance = ParseGuidance(segment.PendingVideoGuidance);
        if (guidance == StoryVideoGuidance.SourceVideo && !CanUseSourceVideo(segment, model)) return false;
        if (guidance == StoryVideoGuidance.PreviousVideo && !CanUsePreviousVideo(segment, model)) return false;
        var profile = VideoGenerationProfile.ForModel(model.ModelName);
        var seconds = profile.Durations.OrderBy(value => Math.Abs(value - segment.Seconds)).First();
        var prompt = BuildVideoPrompt(segment, guidance);
        return VideoRequestDigestMatches(
            segment.PendingVideoRequestDigest!, segment, model, profile, prompt, seconds, guidance);
    }

    private string VideoRequestDigest(
        StorySegmentEditor segment,
        MediaGenerationModel model,
        VideoGenerationProfile profile,
        string prompt,
        int seconds,
        StoryVideoGuidance guidance)
    {
        var document = segment.ToDocument();
        var value = string.Join('\u001f',
            model.Id, profile.Sizes[0], seconds.ToString(), ProjectRatio, prompt,
            guidance == StoryVideoGuidance.Frames ? document.FirstFrameAsset ?? string.Empty : string.Empty,
            guidance == StoryVideoGuidance.Frames && profile.SupportsLastFrame
                ? document.LastFrameAsset ?? string.Empty
                : string.Empty,
            GuidanceKey(guidance),
            ReferenceVideoAsset(segment, guidance));
        return Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant();
    }

    private bool VideoRequestDigestMatches(
        string storedDigest,
        StorySegmentEditor segment,
        MediaGenerationModel model,
        VideoGenerationProfile profile,
        string prompt,
        int seconds,
        StoryVideoGuidance guidance)
    {
        if (string.Equals(
                storedDigest,
                VideoRequestDigest(segment, model, profile, prompt, seconds, guidance),
                StringComparison.Ordinal)) return true;
        if (guidance != StoryVideoGuidance.Frames) return false;
        var document = segment.ToDocument();
        var legacyValue = string.Join('\u001f',
            model.Id, profile.Sizes[0], seconds.ToString(), ProjectRatio, prompt,
            document.FirstFrameAsset ?? string.Empty,
            profile.SupportsLastFrame ? document.LastFrameAsset ?? string.Empty : string.Empty);
        var legacyDigest = Convert.ToHexString(
            SHA256.HashData(Encoding.UTF8.GetBytes(legacyValue))).ToLowerInvariant();
        return string.Equals(storedDigest, legacyDigest, StringComparison.Ordinal);
    }

    private void NotifyVideoJobChanged()
    {
        OnPropertyChanged(nameof(CanGenerateVideo));
        OnPropertyChanged(nameof(CanResumeSelectedVideo));
        OnPropertyChanged(nameof(CanAbandonSelectedVideoJob));
        OnPropertyChanged(nameof(SelectedVideoJobLabel));
        OnPropertyChanged(nameof(CanRegenerateFromCurrentVideo));
        OnPropertyChanged(nameof(CanGenerateFromPreviousVideo));
        OnPropertyChanged(nameof(PreviousVideoGenerationSummary));
        OnPropertyChanged(nameof(GenerateVideoActionLabel));
        OnPropertyChanged(nameof(VideoRegenerationSummary));
    }

    private string BuildVideoPrompt(StorySegmentEditor segment, StoryVideoGuidance guidance)
    {
        var continuity = BuildContinuityContext(segment);
        if (guidance == StoryVideoGuidance.SourceVideo)
            continuity = $"{continuity}\n参考视频就是本段当前成片。保留未要求改变的人物、场景、构图与节奏，并按视频提示词和附加创作要求完成修改。".Trim();
        else if (guidance == StoryVideoGuidance.PreviousVideo)
            continuity = $"{continuity}\n参考视频就是紧邻本段之前的完整成片。延续它结尾的镜头方向、运动速度、人物动作和光线变化，从其结束状态自然进入本段，不要重演上一段内容。".Trim();
        return StoryPromptCatalog.RenderVideo(segment.VideoPrompt, continuity, CreativeRequirements);
    }

    private static async Task<VideoGenerationInputVideo> LoadVideoReferenceAsync(
        string path,
        CancellationToken cancellationToken)
    {
        if (!ReferenceVideoCanLoad(path))
            throw new InvalidDataException("参考视频为空、格式不受支持或超过 47 MB。");
        var mimeType = Path.GetExtension(path).Equals(".mov", StringComparison.OrdinalIgnoreCase)
            ? "video/quicktime"
            : "video/mp4";
        var bytes = await File.ReadAllBytesAsync(path, cancellationToken);
        return new VideoGenerationInputVideo(Path.GetFileName(path), mimeType, Convert.ToBase64String(bytes));
    }

    private static bool ReferenceVideoCanLoad(string path)
    {
        var extension = Path.GetExtension(path).ToLowerInvariant();
        var file = new FileInfo(path);
        return extension is ".mp4" or ".mov" && file.Exists && file.Length is > 0 and <= 47 * 1024 * 1024;
    }

    private static bool CanUseSourceVideo(StorySegmentEditor segment, MediaGenerationModel model) =>
        segment.VideoPath is { Length: > 0 } path && ReferenceVideoCanLoad(path) &&
        VideoGenerationProfile.ForModel(model.ModelName).SupportsReferenceVideo;

    private bool CanUsePreviousVideo(StorySegmentEditor segment, MediaGenerationModel model) =>
        PreviousSegment(segment)?.VideoPath is { Length: > 0 } path && ReferenceVideoCanLoad(path) &&
        VideoGenerationProfile.ForModel(model.ModelName).SupportsReferenceVideo;

    private StorySegmentEditor? PreviousSegment(StorySegmentEditor segment)
    {
        var index = Segments.IndexOf(segment);
        return index > 0 ? Segments[index - 1] : null;
    }

    private string ReferenceVideoAsset(StorySegmentEditor segment, StoryVideoGuidance guidance) =>
        guidance switch
        {
            StoryVideoGuidance.SourceVideo => segment.ToDocument().VideoAsset ?? string.Empty,
            StoryVideoGuidance.PreviousVideo => PreviousSegment(segment)?.ToDocument().VideoAsset ?? string.Empty,
            _ => string.Empty,
        };

    private static StoryVideoGuidance ParseGuidance(string value) => value switch
    {
        "source-video" => StoryVideoGuidance.SourceVideo,
        "previous-video" => StoryVideoGuidance.PreviousVideo,
        _ => StoryVideoGuidance.Frames,
    };

    private static string GuidanceKey(StoryVideoGuidance guidance) => guidance switch
    {
        StoryVideoGuidance.SourceVideo => "source-video",
        StoryVideoGuidance.PreviousVideo => "previous-video",
        _ => "frames",
    };

    private enum StoryVideoGuidance
    {
        Frames,
        SourceVideo,
        PreviousVideo,
    }

    private static string ShortJobId(string? value) => string.IsNullOrWhiteSpace(value)
        ? "未知"
        : value[..Math.Min(value.Length, 16)];
}
