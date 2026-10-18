using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Notepad;

public sealed partial class NotepadViewModel
{
    [RelayCommand]
    private async Task RenameFolderAsync(NotepadFolderRenameRequest? request)
    {
        if (request is null || !CanReload) return;
        ErrorMessage = null;
        ActionMessage = null;
        var source = NormalizeFolder(request.From);
        var target = NormalizeFolder(request.To);
        if (source.Length == 0)
        {
            ErrorMessage = L("根目录不能重命名。", "The root folder cannot be renamed.");
            return;
        }
        if (target.Length == 0)
        {
            ErrorMessage = L("文件夹名称不能为空。", "Folder name cannot be empty.");
            return;
        }
        if (source == target) return;
        if (target.StartsWith(source + "/", StringComparison.Ordinal))
        {
            ErrorMessage = L("不能将文件夹移动到它自己的子目录中。", "A folder cannot be moved into its own subfolder.");
            return;
        }

        string MapFolder(string path)
        {
            var normalized = NormalizeFolder(path);
            return normalized == source || normalized.StartsWith(source + "/", StringComparison.Ordinal)
                ? target + normalized[source.Length..] : path;
        }
        NotepadNote MapNote(NotepadNote note) => note with { Folder = MapFolder(note.Folder) };
        IReadOnlyList<string> MapFolders(IEnumerable<string> paths)
        {
            var mapped = paths.Select(MapFolder).ToList();
            for (var parent = target; parent.Length > 0;)
            {
                mapped.Add(parent);
                var separator = parent.LastIndexOf('/');
                parent = separator < 0 ? string.Empty : parent[..separator];
            }
            return NormalizeFolders(mapped);
        }

        var token = _sessionCancellation!.Token;
        // Refresh the search that was actually applied, retaining any unsubmitted
        // text in the search box and the independent editor buffer.
        var query = _appliedSearchQuery;
        IsLoading = true;
        ErrorMessage = null;
        ActionMessage = null;
        var renamed = false;
        try
        {
            await _service.RenameFolderAsync(source, target, token).ConfigureAwait(false);
            renamed = true;
            var applied = false;
            await InvokeForSessionAsync(() =>
            {
                for (var index = 0; index < _allNotes.Count; index++) _allNotes[index] = MapNote(_allNotes[index]);
                if (SelectedNote is { } selected) SelectedNote = MapNote(selected);
                SelectedFolder = MapFolder(SelectedFolder);
                ReplaceFolders(MapFolders(Folders.Select(static item => item.Path)).Select(static path => new NotepadFolderItem(path)));
                ApplyFilter();
                ActionMessage = L("文件夹已重命名。", "Folder renamed.");
                applied = true;
            }, token).ConfigureAwait(false);
            if (!applied) return;

            var foldersTask = _service.ListFoldersAsync(token);
            var notesTask = _service.ListNotesAsync(query.Length == 0 ? null : query, 500, token);
            await Task.WhenAll(foldersTask, notesTask).ConfigureAwait(false);
            await InvokeForSessionAsync(() =>
            {
                ReplaceFolders(MapFolders(foldersTask.Result).Select(static path => new NotepadFolderItem(path)));
                ApplyNoteResults(notesTask.Result.Select(MapNote).ToArray(), query);
                // A renamed note may no longer match the search. Retain it in the
                // editor without inserting it into the results or reloading text.
                if (_allNotes.FirstOrDefault(note => note.Id == SelectedNote?.Id) is { } refreshed)
                    SelectedNote = refreshed;
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await InvokeForSessionAsync(() => ErrorMessage = renamed
                ? L($"重命名已完成，但列表刷新失败：{exception.Message}。可点击刷新重试，无需重复重命名。",
                    $"Rename completed, but lists could not be refreshed: {exception.Message}. Retry Refresh without renaming again.")
                : exception.Message, token).ConfigureAwait(false);
        }
        finally
        {
            await InvokeForSessionAsync(() => IsLoading = false, token, allowCanceled: true).ConfigureAwait(false);
        }
    }
}
