namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel
{
    public bool CanRestoreArchivedFrame => !IsBusy &&
        SelectedSegment is { HasPendingVideoJob: false } segment && segment.ArchivedFrames.Count > 0;

    public async Task RestoreArchivedFrameAsync(
        StoryArchivedFrameEditor archived,
        CancellationToken cancellationToken = default)
    {
        var segment = SelectedSegment;
        if (!CanRestoreArchivedFrame || segment is null || !segment.ArchivedFrames.Contains(archived)) return;
        if (archived.FilePath is not { Length: > 0 } path || !File.Exists(path))
        {
            ErrorMessage = "这个画面历史版本的本机文件已经不存在，无法恢复。";
            return;
        }
        var previous = segment.ToDocument();
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            segment.RestoreArchivedFrame(archived);
            await PersistCurrentAsync(cancellationToken);
            StatusMessage = $"已恢复 {segment.NumberLabel} 的{archived.RoleLabel}版本";
        }
        catch (OperationCanceledException)
        {
            segment.RestoreFrameState(previous);
            StatusMessage = "已停止恢复画面版本，并回到恢复前状态";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            segment.RestoreFrameState(previous);
            ErrorMessage = exception.Message;
            StatusMessage = "画面版本恢复失败，已回到恢复前状态";
        }
        finally
        {
            IsBusy = false;
            OnPropertyChanged(nameof(CanRestoreArchivedFrame));
        }
    }
}
