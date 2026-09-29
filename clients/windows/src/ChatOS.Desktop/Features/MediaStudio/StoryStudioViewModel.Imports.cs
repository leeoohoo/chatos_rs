namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel
{
    public bool CanImportResourceAsset => !IsBusy && _current is not null && SelectedResource is not null;
    public bool CanImportSegmentAsset => !IsBusy && _current is not null && SelectedSegment is not null;

    public async Task ImportResourceImageAsync(
        string sourcePath,
        CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        var project = _current;
        var resource = SelectedResource;
        var session = _session;
        if (!CanImportResourceAsset || owner is null || project is null || resource is null) return;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            await PersistCurrentAsync(cancellationToken);
            var context = new ResourceGenerationContext(owner, project.Id, resource, session);
            var relative = await _store.ImportAssetAsync(
                owner,
                project.Id,
                $"resource-{resource.Id}",
                sourcePath,
                false,
                cancellationToken);
            EnsureResourceContext(context);
            resource.SetImage(relative, _store.ResolveAssetPath(owner, project.Id, relative)!);
            await PersistCurrentAsync(cancellationToken);
            StatusMessage = $"已导入{resource.KindLabel}“{resource.Name}”的参考图";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
        finally { IsBusy = false; }
    }

    public Task ImportFirstFrameAsync(string sourcePath, CancellationToken cancellationToken = default) =>
        ImportSegmentAssetAsync(sourcePath, SegmentAssetKind.FirstFrame, cancellationToken);

    public Task ImportLastFrameAsync(string sourcePath, CancellationToken cancellationToken = default) =>
        ImportSegmentAssetAsync(sourcePath, SegmentAssetKind.LastFrame, cancellationToken);

    public Task ImportVideoAsync(string sourcePath, CancellationToken cancellationToken = default) =>
        ImportSegmentAssetAsync(sourcePath, SegmentAssetKind.Video, cancellationToken);

    private async Task ImportSegmentAssetAsync(
        string sourcePath,
        SegmentAssetKind kind,
        CancellationToken cancellationToken)
    {
        var context = CaptureGenerationContext();
        if (!CanImportSegmentAsset || context is null) return;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            await PersistCurrentAsync(cancellationToken);
            var isVideo = kind == SegmentAssetKind.Video;
            var relative = await _store.ImportAssetAsync(
                context.Owner,
                context.ProjectId,
                context.Segment.Id,
                sourcePath,
                isVideo,
                cancellationToken);
            EnsureContext(context);
            var fullPath = _store.ResolveAssetPath(context.Owner, context.ProjectId, relative)!;
            if (isVideo) context.Segment.SetVideo(relative, fullPath);
            else context.Segment.SetFrame(kind == SegmentAssetKind.LastFrame, relative, fullPath);
            await PersistCurrentAsync(cancellationToken);
            StatusMessage = $"已为 {context.Segment.Title} 导入{AssetKindLabel(kind)}";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
        finally { IsBusy = false; }
    }

    private static string AssetKindLabel(SegmentAssetKind kind) => kind switch
    {
        SegmentAssetKind.FirstFrame => "首帧",
        SegmentAssetKind.LastFrame => "尾帧",
        _ => "视频",
    };

    private enum SegmentAssetKind
    {
        FirstFrame,
        LastFrame,
        Video,
    }
}
