using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Notepad;

public sealed partial class NotepadViewModel
{
    [ObservableProperty]
    private bool _isEditorReadOnly;

    [RelayCommand]
    private Task DeleteFolderAsync(NotepadFolderItem? folder)
    {
        var path = NormalizeFolder(folder?.Path);
        // The root is a navigation entry, never a recursive deletion target.
        return path.Length == 0 ? Task.CompletedTask : DeleteCoreAsync(null, path);
    }

    [RelayCommand]
    private Task DeleteNoteAsync(NotepadNote? note) => note is null
        ? Task.CompletedTask
        : DeleteCoreAsync(note.Id, null);

    private async Task DeleteCoreAsync(string? noteId, string? folder)
    {
        if (!CanReload) return;
        var token = _sessionCancellation!.Token;
        bool IncludesFolder(string path) => folder is not null &&
            (NormalizeFolder(path) == folder || NormalizeFolder(path).StartsWith(folder + "/", StringComparison.Ordinal));
        bool IncludesNote(NotepadNote note) => folder is null ? note.Id == noteId : IncludesFolder(note.Folder);

        IsLoading = true;
        // Only the confirmed deletion's editor is frozen. Other notes remain
        // editable and their drafts must survive both deletion and folder refresh.
        IsEditorReadOnly = SelectedNote is { } selected && IncludesNote(selected);
        ErrorMessage = null;
        ActionMessage = null;
        var deleted = false;
        try
        {
            if (folder is null) await _service.DeleteNoteAsync(noteId!, token).ConfigureAwait(false);
            else await _service.DeleteFolderAsync(folder, true, token).ConfigureAwait(false);
            deleted = true;

            var applied = false;
            await InvokeForSessionAsync(() =>
            {
                _allNotes.RemoveAll(note => IncludesNote(note));
                if (SelectedNote is { } current && IncludesNote(current)) ResetEditor();
                if (folder is not null)
                {
                    ReplaceFolders(Folders.Where(item => !IncludesFolder(item.Path)).ToArray());
                    if (IncludesFolder(SelectedFolder)) SelectedFolder = string.Empty;
                }
                ApplyFilter();
                IsEditorReadOnly = false;
                ActionMessage = folder is null
                    ? L("笔记已删除。", "Note deleted.")
                    : L("文件夹及其中的笔记已删除。", "The folder and its notes were deleted.");
                applied = true;
            }, token).ConfigureAwait(false);
            if (!applied) return;

            // Remove the confirmed targets immediately. A note-list reread could
            // reintroduce stale entries or replace unrelated editing/search state.
            var folders = await _service.ListFoldersAsync(token).ConfigureAwait(false);
            await InvokeForSessionAsync(() => ReplaceFolders(NormalizeFolders(folders)
                .Where(path => !IncludesFolder(path)).Select(static path => new NotepadFolderItem(path))), token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await InvokeForSessionAsync(() => ErrorMessage = deleted
                ? L($"删除已完成，但文件夹列表刷新失败：{exception.Message}。可点击刷新重试，无需重复删除。",
                    $"Deletion completed, but folders could not be refreshed: {exception.Message}. Retry Refresh without deleting again.")
                : exception.Message, token).ConfigureAwait(false);
        }
        finally
        {
            await InvokeForSessionAsync(() =>
            {
                IsEditorReadOnly = false;
                IsLoading = false;
            }, token, allowCanceled: true).ConfigureAwait(false);
        }
    }
}
