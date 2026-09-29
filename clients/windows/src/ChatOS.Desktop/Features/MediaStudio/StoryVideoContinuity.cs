namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel
{
    public bool CanExtractSelectedVideoLastFrame => !IsBusy &&
        SelectedSegment?.VideoPath is { Length: > 0 } path && File.Exists(path);

    public async Task ExtractSelectedVideoLastFrameAsync(CancellationToken cancellationToken = default)
    {
        var context = CaptureGenerationContext();
        if (!CanExtractSelectedVideoLastFrame || context?.Segment.VideoPath is not { } path) return;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            await ExtractVideoLastFrameCoreAsync(context, path, cancellationToken);
            await PersistCurrentAsync(cancellationToken);
            StatusMessage = "已提取成片末帧；生成下一段首帧时会自动作为连续性参考";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = $"无法提取成片末帧：{exception.Message}";
        }
        finally
        {
            IsBusy = false;
            NotifyVideoContinuityChanged();
        }
    }

    private async Task ExtractVideoLastFrameCoreAsync(
        GenerationContext context,
        string videoPath,
        CancellationToken cancellationToken)
    {
        EnsureContext(context);
        var frame = await StoryVideoFrameExtractor.ExtractLastFrameAsync(videoPath, cancellationToken);
        var relative = await _store.ImportFrameBytesAsync(
            context.Owner,
            context.ProjectId,
            context.Segment.Id,
            frame.Bytes,
            frame.MimeType,
            cancellationToken);
        EnsureContext(context);
        context.Segment.SetActualVideoLastFrame(
            relative,
            _store.ResolveAssetPath(context.Owner, context.ProjectId, relative)!);
    }

    private void NotifyVideoContinuityChanged()
    {
        OnPropertyChanged(nameof(CanExtractSelectedVideoLastFrame));
    }
}
