using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Notepad;

public sealed partial class NotepadViewModel : ObservableObject, IDisposable
{
    private readonly INotepadService _service;
    private readonly IUiDispatcher _dispatcher;
    private readonly LocalizationViewModel? _localization;
    private readonly List<NotepadNote> _allNotes = [];
    private CancellationTokenSource? _sessionCancellation;
    private bool _initialized;
    private string _savedTitle = string.Empty;
    private string _savedTags = string.Empty;
    private string _savedContent = string.Empty;

    public NotepadViewModel(INotepadService service, IUiDispatcher dispatcher, LocalizationViewModel? localization = null)
    {
        _service = service;
        _dispatcher = dispatcher;
        _localization = localization;
        Notes.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasNotes));
        Folders.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasFolders));
    }

    public ObservableCollection<NotepadFolderItem> Folders { get; } = [];

    public ObservableCollection<NotepadNote> Notes { get; } = [];

    public bool HasFolders => Folders.Count > 0;

    public bool HasNotes => Notes.Count > 0;

    public bool HasSelection => SelectedNote is not null;

    public bool IsDirty => HasSelection &&
        (Title != _savedTitle || TagsText != _savedTags || Content != _savedContent);

    public bool IsPreviewVisible => HasSelection && EditorMode is NotepadEditorMode.Preview or NotepadEditorMode.Split;

    public bool IsEditorVisible => HasSelection && EditorMode is NotepadEditorMode.Edit or NotepadEditorMode.Split;

    public bool IsSplitMode => EditorMode == NotepadEditorMode.Split;

    [ObservableProperty]
    private bool _isOpen;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private bool _isSaving;

    [ObservableProperty]
    private string _searchQuery = string.Empty;

    [ObservableProperty]
    private string _selectedFolder = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasSelection))]
    [NotifyPropertyChangedFor(nameof(IsDirty))]
    [NotifyPropertyChangedFor(nameof(IsPreviewVisible))]
    [NotifyPropertyChangedFor(nameof(IsEditorVisible))]
    private NotepadNote? _selectedNote;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsDirty))]
    private string _title = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsDirty))]
    private string _tagsText = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsDirty))]
    private string _content = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsPreviewVisible))]
    [NotifyPropertyChangedFor(nameof(IsEditorVisible))]
    [NotifyPropertyChangedFor(nameof(IsSplitMode))]
    private NotepadEditorMode _editorMode = NotepadEditorMode.Preview;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _actionMessage;

    public async Task OpenAsync(CancellationToken cancellationToken = default)
    {
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        IsOpen = true;
        await LoadAsync(forceInitialize: false, _sessionCancellation.Token).ConfigureAwait(false);
    }

    public Task CloseAsync(CancellationToken cancellationToken = default)
    {
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
        IsOpen = false;
        return Task.CompletedTask;
    }

    [RelayCommand]
    private Task RefreshAsync() => WithSession(token => LoadAsync(true, token));

    [RelayCommand]
    private Task SearchAsync() => WithSession(token => ReloadResourcesAsync(token, preserveSelection: true));

    [RelayCommand]
    private void SelectFolder(NotepadFolderItem? folder)
    {
        SelectedFolder = folder?.Path ?? string.Empty;
        ApplyFilter();
    }

    [RelayCommand]
    private async Task SelectNoteAsync(NotepadNote? note)
    {
        if (note is null || _sessionCancellation is null || note.Id == SelectedNote?.Id)
        {
            return;
        }

        if (IsDirty && !await SaveCoreAsync(_sessionCancellation.Token).ConfigureAwait(false))
        {
            return;
        }

        await LoadNoteAsync(note.Id, _sessionCancellation.Token).ConfigureAwait(false);
    }

    [RelayCommand]
    private Task SaveAsync() => WithSession(async token => _ = await SaveCoreAsync(token).ConfigureAwait(false));

    [RelayCommand]
    private Task CreateFolderAsync(string? value)
    {
        var folder = NormalizeFolder(value);
        if (folder.Length == 0)
        {
            ErrorMessage = L("文件夹名称不能为空。", "Folder name cannot be empty.");
            return Task.CompletedTask;
        }

        if (SelectedFolder.Length > 0 && !folder.StartsWith(SelectedFolder + "/", StringComparison.Ordinal))
        {
            folder = $"{SelectedFolder}/{folder}";
        }

        var target = folder;
        return MutateAsync(
            L("文件夹已创建。", "Folder created."),
            token => _service.CreateFolderAsync(target, token),
            token => ReloadResourcesAsync(token, true),
            () => SelectedFolder = target);
    }

    [RelayCommand]
    private Task RenameFolderAsync(NotepadFolderRenameRequest? request)
    {
        if (request is null)
        {
            return Task.CompletedTask;
        }

        var target = NormalizeFolder(request.To);
        if (target.Length == 0)
        {
            ErrorMessage = L("文件夹名称不能为空。", "Folder name cannot be empty.");
            return Task.CompletedTask;
        }

        return MutateAsync(
            L("文件夹已重命名。", "Folder renamed."),
            token => _service.RenameFolderAsync(request.From, target, token),
            token => ReloadResourcesAsync(token, true),
            () => SelectedFolder = target);
    }

    [RelayCommand]
    private Task DeleteFolderAsync(NotepadFolderItem? folder) => folder is null
        ? Task.CompletedTask
        : MutateAsync(
            L("文件夹及其中的笔记已删除。", "The folder and its notes were deleted."),
            token => _service.DeleteFolderAsync(folder.Path, true, token),
            token => ReloadResourcesAsync(token, false),
            ResetEditor);

    [RelayCommand]
    private Task CreateNoteAsync(NotepadNoteCreationRequest? request)
    {
        if (request is null)
        {
            return Task.CompletedTask;
        }

        var folder = NormalizeFolder(request.Folder ?? SelectedFolder);
        return MutateAsync(
            L("笔记已创建。", "Note created."),
            async token =>
            {
                var detail = await _service.CreateNoteAsync(
                    new NotepadNoteDraft(folder, request.Title.Trim(), string.Empty, []),
                    token).ConfigureAwait(false);
                await _dispatcher.InvokeAsync(() => ApplyDetail(detail), token).ConfigureAwait(false);
            },
            token => ReloadResourcesAsync(token, true));
    }

    [RelayCommand]
    private Task DeleteNoteAsync(NotepadNote? note) => note is null
        ? Task.CompletedTask
        : MutateAsync(
            L("笔记已删除。", "Note deleted."),
            token => _service.DeleteNoteAsync(note.Id, token),
            token => ReloadResourcesAsync(token, false),
            ResetEditor);

    public void Dispose()
    {
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
    }

    private async Task LoadAsync(bool forceInitialize, CancellationToken token)
    {
        IsLoading = true;
        ErrorMessage = null;
        try
        {
            if (!_initialized || forceInitialize)
            {
                await _service.InitializeAsync(token).ConfigureAwait(false);
                _initialized = true;
            }

            await ReloadResourcesAsync(token, preserveSelection: true).ConfigureAwait(false);
            if (SelectedNote is null && Notes.FirstOrDefault() is { } first)
            {
                await LoadNoteAsync(first.Id, token).ConfigureAwait(false);
            }
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsLoading = false).ConfigureAwait(false);
        }
    }

    private async Task ReloadResourcesAsync(CancellationToken token, bool preserveSelection)
    {
        var selectedId = preserveSelection ? SelectedNote?.Id : null;
        var foldersTask = _service.ListFoldersAsync(token);
        var notesTask = _service.ListNotesAsync(SearchQuery.Trim(), 500, token);
        await Task.WhenAll(foldersTask, notesTask).ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() =>
        {
            Replace(Folders, NormalizeFolders(foldersTask.Result).Select(static path => new NotepadFolderItem(path)));
            _allNotes.Clear();
            _allNotes.AddRange(notesTask.Result.OrderByDescending(static note => note.UpdatedAt ?? note.CreatedAt));
            ApplyFilter();
            if (selectedId is not null)
            {
                SelectedNote = _allNotes.FirstOrDefault(note => note.Id == selectedId);
                if (SelectedNote is null)
                {
                    ResetEditor();
                }
            }
        }, token).ConfigureAwait(false);
    }

    private async Task LoadNoteAsync(string id, CancellationToken token)
    {
        IsLoading = true;
        ErrorMessage = null;
        try
        {
            var detail = await _service.FetchNoteAsync(id, token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => ApplyDetail(detail), token).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsLoading = false).ConfigureAwait(false);
        }
    }

    private async Task<bool> SaveCoreAsync(CancellationToken token)
    {
        if (SelectedNote is null)
        {
            return true;
        }

        IsSaving = true;
        ErrorMessage = null;
        try
        {
            var detail = await _service.UpdateNoteAsync(
                SelectedNote.Id,
                new NotepadNoteUpdate(Title.Trim(), Content, Tags: ParseTags(TagsText)),
                token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                ApplyDetail(detail);
                ActionMessage = L("笔记已保存。", "Note saved.");
            }, token).ConfigureAwait(false);
            return true;
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
            return false;
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsSaving = false).ConfigureAwait(false);
        }
    }

    private async Task MutateAsync(
        string success,
        Func<CancellationToken, Task> mutation,
        Func<CancellationToken, Task>? refresh = null,
        Action? after = null)
    {
        if (_sessionCancellation is null || IsLoading || IsSaving)
        {
            return;
        }

        var token = _sessionCancellation.Token;
        IsLoading = true;
        ErrorMessage = null;
        ActionMessage = null;
        try
        {
            await mutation(token).ConfigureAwait(false);
            if (refresh is not null)
            {
                await refresh(token).ConfigureAwait(false);
            }

            await _dispatcher.InvokeAsync(() =>
            {
                after?.Invoke();
                ApplyFilter();
                ActionMessage = success;
            }, token).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsLoading = false).ConfigureAwait(false);
        }
    }

    private Task WithSession(Func<CancellationToken, Task> action) =>
        _sessionCancellation is null ? Task.CompletedTask : action(_sessionCancellation.Token);

    private void ApplyDetail(NotepadNoteDetail detail)
    {
        var existing = _allNotes.FindIndex(note => note.Id == detail.Note.Id);
        if (existing >= 0) _allNotes[existing] = detail.Note;
        else _allNotes.Add(detail.Note);
        SelectedNote = detail.Note;
        SelectedFolder = NormalizeFolder(detail.Note.Folder);
        Title = detail.Note.Title;
        TagsText = string.Join(", ", detail.Note.Tags);
        Content = detail.Content;
        _savedTitle = Title;
        _savedTags = TagsText;
        _savedContent = Content;
        ApplyFilter();
        OnPropertyChanged(nameof(IsDirty));
    }

    private void ApplyFilter()
    {
        var folder = NormalizeFolder(SelectedFolder);
        Replace(Notes, _allNotes.Where(note => NormalizeFolder(note.Folder) == folder));
    }

    private void ResetEditor()
    {
        SelectedNote = null;
        Title = string.Empty;
        TagsText = string.Empty;
        Content = string.Empty;
        _savedTitle = string.Empty;
        _savedTags = string.Empty;
        _savedContent = string.Empty;
        OnPropertyChanged(nameof(IsDirty));
    }

    private static string NormalizeFolder(string? value) => string.Join(
        '/',
        (value ?? string.Empty)
            .Trim()
            .Replace('\\', '/')
            .Split('/', StringSplitOptions.RemoveEmptyEntries)
            .Where(static part => part is not ("." or "..")));

    private static IReadOnlyList<string> NormalizeFolders(IEnumerable<string> values) => values
        .Select(NormalizeFolder)
        .Where(static value => value.Length > 0)
        .Distinct(StringComparer.OrdinalIgnoreCase)
        .OrderBy(static value => value, StringComparer.CurrentCultureIgnoreCase)
        .ToArray();

    private static IReadOnlyList<string> ParseTags(string value) => value
        .Split([',', '，', '\n'], StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
        .Distinct(StringComparer.CurrentCultureIgnoreCase)
        .ToArray();

    private static void Replace<T>(ObservableCollection<T> target, IEnumerable<T> values)
    {
        target.Clear();
        foreach (var value in values) target.Add(value);
    }

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;
}
