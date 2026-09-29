namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel
{
    public bool CanRestoreArchivedVideo => !IsBusy && SelectedSegment?.ArchivedVideos.Count > 0;

    public async Task RestoreArchivedVideoAsync(
        StoryArchivedVideoEditor archived,
        CancellationToken cancellationToken = default)
    {
        var segment = SelectedSegment;
        if (!CanRestoreArchivedVideo || segment is null || !segment.ArchivedVideos.Contains(archived)) return;
        if (archived.FilePath is not { Length: > 0 } path || !File.Exists(path))
        {
            ErrorMessage = "这个视频历史版本的本机文件已经不存在，无法恢复。";
            return;
        }
        var previous = segment.ToDocument();
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            segment.RestoreArchivedVideo(archived);
            await PersistCurrentAsync(cancellationToken);
            StatusMessage = $"已恢复 {segment.NumberLabel} 的视频版本：{archived.Label}";
        }
        catch (OperationCanceledException)
        {
            segment.RestoreVideoState(previous);
            StatusMessage = "已停止恢复视频版本，并回到恢复前状态";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            segment.RestoreVideoState(previous);
            ErrorMessage = exception.Message;
            StatusMessage = "视频版本恢复失败，已回到恢复前状态";
        }
        finally
        {
            IsBusy = false;
            NotifyVideoVersionsChanged();
        }
    }

    private void NotifyVideoVersionsChanged()
    {
        OnPropertyChanged(nameof(CanRestoreArchivedVideo));
    }
}
