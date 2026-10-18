using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Notepad;

public sealed partial class NotepadViewModel
{
    [RelayCommand(CanExecute = nameof(CanReload))]
    private async Task CreateNoteAsync(NotepadNoteCreationRequest? request)
    {
        if (request is null || !CanReload) return;

        var token = _sessionCancellation!.Token;
        // Capture the requested destination before saving, while folder navigation
        // is still available. Never replace a draft that failed to save.
        var draft = new NotepadNoteDraft(NormalizeFolder(request.Folder ?? SelectedFolder), request.Title.Trim(), string.Empty, []);
        if (IsDirty && (!await SaveCoreAsync(token).ConfigureAwait(false) || IsDirty)) return;

        var started = false;
        string? previousId = null;
        await InvokeForSessionAsync(() =>
        {
            if (!CanReload || IsDirty) return;
            previousId = SelectedNote?.Id;
            IsLoading = true;
            ErrorMessage = null;
            ActionMessage = null;
            started = true;
        }, token).ConfigureAwait(false);
        if (!started) return;

        var created = false;
        try
        {
            var detail = await _service.CreateNoteAsync(draft, token).ConfigureAwait(false);
            created = true;
            var applied = false;
            await InvokeForSessionAsync(() =>
            {
                if (!IsDirty && SelectedNote?.Id == previousId)
                {
                    // Keep an active search's results intact, just as saving a
                    // note outside those results does. The editor is independent.
                    ApplyDetail(detail, includeInResults: !IsSearchActive);
                    ActionMessage = IsSearchActive
                        ? L("笔记已创建。清空搜索后可在目录中查看。", "Note created. Clear the search to find it in its folder.")
                        : L("笔记已创建。", "Note created.");
                }
                else
                {
                    // Typing stays available during creation. Leave newer edits
                    // in place and make the created note available for later.
                    if (!IsSearchActive) _allNotes.Add(detail.Note);
                    ApplyFilter();
                    ActionMessage = IsSearchActive
                        ? L("笔记已创建；当前笔记的新修改已保留。保存并清空搜索后可打开新笔记。",
                            "Note created; newer edits to the current note were kept. Save them and clear the search to open the new note.")
                        : L("笔记已创建；当前笔记的新修改已保留，请保存后再打开新笔记。",
                            "Note created; newer edits to the current note were kept. Save them before opening the new note.");
                }
                applied = true;
            }, token).ConfigureAwait(false);
            if (!applied) return;

            // The create response already supplies the note. Refresh only folders,
            // so a stale or failed list read cannot undo a successful creation.
            var folders = await _service.ListFoldersAsync(token).ConfigureAwait(false);
            await InvokeForSessionAsync(() =>
                ReplaceFolders(NormalizeFolders(folders).Select(static path => new NotepadFolderItem(path))), token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await InvokeForSessionAsync(() => ErrorMessage = created
                ? L($"笔记已创建，但文件夹列表刷新失败：{exception.Message}。可点击刷新重试，无需重复新建。",
                    $"Note created, but folders could not be refreshed: {exception.Message}. Retry Refresh without creating another note.")
                : exception.Message, token).ConfigureAwait(false);
        }
        finally
        {
            await InvokeForSessionAsync(() => IsLoading = false, token, allowCanceled: true).ConfigureAwait(false);
        }
    }
}
