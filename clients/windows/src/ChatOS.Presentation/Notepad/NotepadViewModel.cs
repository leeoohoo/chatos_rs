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

    public NotepadFolderItem? SelectedFolderItem => Folders.FirstOrDefault(folder => folder.Path == SelectedFolder);

    public bool IsRootFolderSelected => SelectedFolder.Length == 0;

    public bool HasNotes => Notes.Count > 0;

    public bool HasSelection => SelectedNote is not null;

    // Search responses replace metadata instances. Bind list selection to the
    // current result by ID, independently of the note retained in the editor.
    public NotepadNote? SelectedResultNote => Notes.FirstOrDefault(note => note.Id == SelectedNote?.Id);

    public bool IsDirty => HasSelection &&
        (Title != _savedTitle || TagsText != _savedTags || Content != _savedContent);

    public bool CanSave => CanReload && IsDirty;

    public bool IsBusy => IsLoading || IsSaving || IsSearching;

    public bool CanReload => IsOpen && !IsBusy && _sessionCancellation is { IsCancellationRequested: false };

    public bool CanClose => !IsSaving;

    public bool CanDiscardChanges => IsOpen && IsDirty && !IsSaving;

    public bool IsPreviewVisible => HasSelection && EditorMode is NotepadEditorMode.Preview or NotepadEditorMode.Split;

    public bool IsEditorVisible => HasSelection && EditorMode is NotepadEditorMode.Edit or NotepadEditorMode.Split;

    public bool IsSplitMode => EditorMode == NotepadEditorMode.Split;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyCanExecuteChangedFor(nameof(SaveCommand))]
    [NotifyPropertyChangedFor(nameof(CanReload))]
    [NotifyCanExecuteChangedFor(nameof(RefreshCommand))]
    [NotifyCanExecuteChangedFor(nameof(CreateNoteCommand))]
    [NotifyCanExecuteChangedFor(nameof(SearchCommand))]
    [NotifyPropertyChangedFor(nameof(CanSearch))]
    [NotifyPropertyChangedFor(nameof(CanDiscardChanges))]
    [NotifyCanExecuteChangedFor(nameof(DiscardChangesCommand))]
    private bool _isOpen;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyCanExecuteChangedFor(nameof(SaveCommand))]
    [NotifyPropertyChangedFor(nameof(IsBusy))]
    [NotifyPropertyChangedFor(nameof(CanReload))]
    [NotifyCanExecuteChangedFor(nameof(RefreshCommand))]
    [NotifyCanExecuteChangedFor(nameof(CreateNoteCommand))]
    [NotifyCanExecuteChangedFor(nameof(SearchCommand))]
    [NotifyPropertyChangedFor(nameof(CanSearch))]
    private bool _isLoading;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyCanExecuteChangedFor(nameof(SaveCommand))]
    [NotifyPropertyChangedFor(nameof(IsBusy))]
    [NotifyPropertyChangedFor(nameof(CanClose))]
    [NotifyPropertyChangedFor(nameof(CanReload))]
    [NotifyCanExecuteChangedFor(nameof(RefreshCommand))]
    [NotifyCanExecuteChangedFor(nameof(CreateNoteCommand))]
    [NotifyCanExecuteChangedFor(nameof(SearchCommand))]
    [NotifyPropertyChangedFor(nameof(CanSearch))]
    [NotifyPropertyChangedFor(nameof(CanDiscardChanges))]
    [NotifyCanExecuteChangedFor(nameof(DiscardChangesCommand))]
    private bool _isSaving;

    [ObservableProperty]
    private string _searchQuery = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(SelectedFolderItem))]
    [NotifyPropertyChangedFor(nameof(IsRootFolderSelected))]
    private string _selectedFolder = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasSelection))]
    [NotifyPropertyChangedFor(nameof(SelectedResultNote))]
    [NotifyPropertyChangedFor(nameof(IsDirty))]
    [NotifyPropertyChangedFor(nameof(IsPreviewVisible))]
    [NotifyPropertyChangedFor(nameof(IsEditorVisible))]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyCanExecuteChangedFor(nameof(SaveCommand))]
    [NotifyPropertyChangedFor(nameof(CanDiscardChanges))]
    [NotifyCanExecuteChangedFor(nameof(DiscardChangesCommand))]
    private NotepadNote? _selectedNote;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsDirty))]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyCanExecuteChangedFor(nameof(SaveCommand))]
    [NotifyPropertyChangedFor(nameof(CanDiscardChanges))]
    [NotifyCanExecuteChangedFor(nameof(DiscardChangesCommand))]
    private string _title = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsDirty))]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyCanExecuteChangedFor(nameof(SaveCommand))]
    [NotifyPropertyChangedFor(nameof(CanDiscardChanges))]
    [NotifyCanExecuteChangedFor(nameof(DiscardChangesCommand))]
    private string _tagsText = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsDirty))]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyCanExecuteChangedFor(nameof(SaveCommand))]
    [NotifyPropertyChangedFor(nameof(CanDiscardChanges))]
    [NotifyCanExecuteChangedFor(nameof(DiscardChangesCommand))]
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
        if (IsOpen && IsSaving && _sessionCancellation is { IsCancellationRequested: false }) return;

        CancelSearch();
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        IsSaving = false;
        IsEditorReadOnly = false;
        IsOpen = true;
        await LoadAsync(forceInitialize: false, _sessionCancellation.Token).ConfigureAwait(false);
    }

    public Task CloseAsync(CancellationToken cancellationToken = default)
    {
        CancelSearch();
        var session = _sessionCancellation;
        _sessionCancellation = null;
        session?.Cancel();
        session?.Dispose();
        IsOpen = false;
        IsSaving = false;
        IsLoading = false;
        IsEditorReadOnly = false;
        return Task.CompletedTask;
    }

    [RelayCommand(CanExecute = nameof(CanReload))]
    private Task RefreshAsync() => !CanReload ? Task.CompletedTask : WithSession(token => LoadAsync(true, token));

    [RelayCommand]
    private void SelectFolder(NotepadFolderItem? folder)
    {
        SelectedFolder = folder?.Path ?? string.Empty;
        ApplyFilter();
    }

    [RelayCommand]
    private async Task SelectNoteAsync(NotepadNote? note)
    {
        if (note is null || !CanReload || note.Id == SelectedNote?.Id)
        {
            return;
        }

        var token = _sessionCancellation!.Token;
        if (IsDirty && (!await SaveCoreAsync(token).ConfigureAwait(false) || IsDirty))
        {
            return;
        }

        if (IsCurrentSession(token))
        {
            await LoadNoteAsync(note.Id, token).ConfigureAwait(false);
        }
    }

    [RelayCommand(CanExecute = nameof(CanSave))]
    private Task SaveAsync() => WithSession(async token => _ = await SaveCoreAsync(token).ConfigureAwait(false));

    [RelayCommand(CanExecute = nameof(CanDiscardChanges))]
    private void DiscardChanges()
    {
        if (!CanDiscardChanges) return;
        CancelSearch();
        Title = _savedTitle;
        TagsText = _savedTags;
        Content = _savedContent;
        ErrorMessage = null;
        ActionMessage = L("未保存的更改已放弃。", "Unsaved changes discarded.");
        NotifySavedStateChanged();
    }

    [RelayCommand]
    private Task CreateFolderAsync(string? value)
    {
        if (!CanReload) return Task.CompletedTask;
        ErrorMessage = null;
        ActionMessage = null;
        var folder = NormalizeFolder(value);
        if (folder.Length == 0)
        {
            ErrorMessage = L("文件夹名称不能为空。", "Folder name cannot be empty.");
            return Task.CompletedTask;
        }

        if (SelectedFolder.Length > 0 && folder != SelectedFolder &&
            !folder.StartsWith(SelectedFolder + "/", StringComparison.Ordinal))
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

    public void Dispose()
    {
        _ = CloseAsync();
    }

    private async Task LoadAsync(bool forceInitialize, CancellationToken token)
    {
        if (!IsCurrentSession(token)) return;
        IsLoading = true;
        ErrorMessage = null;
        ActionMessage = null;
        try
        {
            if (!_initialized || forceInitialize)
            {
                await _service.InitializeAsync(token).ConfigureAwait(false);
                await InvokeForSessionAsync(() => _initialized = true, token).ConfigureAwait(false);
            }

            if (!IsCurrentSession(token)) return;
            await ReloadResourcesAsync(token, preserveSelection: true).ConfigureAwait(false);
            string? firstId = null;
            await InvokeForSessionAsync(() =>
            {
                if (SelectedNote is null) firstId = Notes.FirstOrDefault()?.Id;
            }, token).ConfigureAwait(false);
            if (firstId is not null)
            {
                await ReadNoteCoreAsync(firstId, token).ConfigureAwait(false);
            }
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await InvokeForSessionAsync(() => ErrorMessage = exception.Message, token).ConfigureAwait(false);
        }
        finally
        {
            await InvokeForSessionAsync(() => IsLoading = false, token, allowCanceled: true).ConfigureAwait(false);
        }
    }

    private async Task ReloadResourcesAsync(CancellationToken token, bool preserveSelection)
    {
        string? selectedId = null;
        var query = string.Empty;
        var captured = false;
        await InvokeForSessionAsync(() =>
        {
            selectedId = preserveSelection ? SelectedNote?.Id : null;
            query = SearchQuery.Trim();
            captured = true;
        }, token).ConfigureAwait(false);
        if (!captured) return;

        var foldersTask = _service.ListFoldersAsync(token);
        var notesTask = _service.ListNotesAsync(query.Length == 0 ? null : query, 500, token);
        await Task.WhenAll(foldersTask, notesTask).ConfigureAwait(false);
        await InvokeForSessionAsync(() =>
        {
            ReplaceFolders(NormalizeFolders(foldersTask.Result).Select(static path => new NotepadFolderItem(path)));
            ApplyNoteResults(notesTask.Result, query);
            if (selectedId is not null && SelectedNote?.Id == selectedId)
            {
                if (_allNotes.FirstOrDefault(note => note.Id == selectedId) is { } refreshed)
                {
                    SelectedNote = refreshed;
                }
                else if (query.Length == 0 && !IsDirty)
                {
                    ResetEditor();
                }
            }
        }, token).ConfigureAwait(false);
    }

    private async Task LoadNoteAsync(string id, CancellationToken token)
    {
        var started = false;
        await InvokeForSessionAsync(() =>
        {
            if (IsBusy || IsDirty) return;
            IsLoading = true;
            ErrorMessage = null;
            ActionMessage = null;
            started = true;
        }, token).ConfigureAwait(false);
        if (!started) return;

        try
        {
            await ReadNoteCoreAsync(id, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await InvokeForSessionAsync(() => ErrorMessage = exception.Message, token).ConfigureAwait(false);
        }
        finally
        {
            await InvokeForSessionAsync(() => IsLoading = false, token, allowCanceled: true).ConfigureAwait(false);
        }
    }

    private async Task ReadNoteCoreAsync(string id, CancellationToken token)
    {
        string? previousId = null;
        var previousTitle = string.Empty;
        var previousTags = string.Empty;
        var previousContent = string.Empty;
        var captured = false;
        await InvokeForSessionAsync(() =>
        {
            if (IsDirty) return;
            previousId = SelectedNote?.Id;
            previousTitle = Title;
            previousTags = TagsText;
            previousContent = Content;
            captured = true;
        }, token).ConfigureAwait(false);
        if (!captured) return;

        var detail = await _service.FetchNoteAsync(id, token).ConfigureAwait(false);
        await InvokeForSessionAsync(() =>
        {
            // A detail read must not replace input typed into the previous note
            // while the request or its UI callback was waiting.
            if (SelectedNote?.Id != previousId || Title != previousTitle || TagsText != previousTags || Content != previousContent)
            {
                ActionMessage = L("新的修改已保留在当前笔记中。保存后可再次打开目标笔记。",
                    "Newer edits were kept in the current note. Save them before opening the requested note again.");
                return;
            }
            ApplyDetail(detail);
        }, token).ConfigureAwait(false);
    }

    private async Task<bool> SaveCoreAsync(CancellationToken token)
    {
        if (SelectedNote is null)
        {
            return true;
        }

        if (IsBusy || !IsCurrentSession(token))
        {
            return false;
        }

        // Reject the same invalid title as the host without sending an update.
        // Automatic save callers must also stay on the current draft.
        if (string.IsNullOrWhiteSpace(Title))
        {
            ActionMessage = null;
            ErrorMessage = L("笔记标题不能为空。", "Note title cannot be empty.");
            return false;
        }

        var noteId = SelectedNote.Id;
        var submittedTitle = Title;
        var submittedTags = TagsText;
        var submittedContent = Content;
        IsSaving = true;
        ErrorMessage = null;
        ActionMessage = null;
        try
        {
            var detail = await _service.UpdateNoteAsync(
                noteId,
                new NotepadNoteUpdate(submittedTitle.Trim(), submittedContent, Tags: ParseTags(submittedTags)),
                token).ConfigureAwait(false);
            var applied = false;
            await InvokeForSessionAsync(() =>
            {
                var existing = _allNotes.FindIndex(note => note.Id == noteId);
                if (existing >= 0) _allNotes[existing] = detail.Note;
                else if (!IsSearchActive) _allNotes.Add(detail.Note);
                if (SelectedNote?.Id == noteId)
                {
                    // Save responses may contain metadata only. Keep the submitted body and
                    // any edits made while saving, rather than rebuilding the editor buffer.
                    SelectedNote = detail.Note;
                    var normalizedTags = string.Join(", ", detail.Note.Tags);
                    // Match macOS: normalize metadata only if the whole editor
                    // still matches the submitted snapshot. A newer edit keeps
                    // all three input buffers intact until the next save.
                    if (Title == submittedTitle && TagsText == submittedTags && Content == submittedContent)
                    {
                        Title = detail.Note.Title;
                        TagsText = normalizedTags;
                    }
                    _savedTitle = detail.Note.Title;
                    _savedTags = normalizedTags;
                    _savedContent = submittedContent;
                    NotifySavedStateChanged();
                    ActionMessage = IsDirty
                        ? L("先前的修改已保存；当前仍有未保存的修改。", "Earlier changes were saved; newer edits are still unsaved.")
                        : L("笔记已保存。", "Note saved.");
                }
                ApplyFilter();
                applied = true;
            }, token).ConfigureAwait(false);
            return applied;
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
            return false;
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await InvokeForSessionAsync(() => ErrorMessage = exception.Message, token).ConfigureAwait(false);
            return false;
        }
        finally
        {
            await InvokeForSessionAsync(() => IsSaving = false, token, allowCanceled: true).ConfigureAwait(false);
        }
    }

    private bool IsCurrentSession(CancellationToken token, bool allowCanceled = false) =>
        (allowCanceled || !token.IsCancellationRequested) && _sessionCancellation is { } session && session.Token == token;

    private Task InvokeForSessionAsync(Action action, CancellationToken token, bool allowCanceled = false) =>
        _dispatcher.InvokeAsync(() =>
        {
            if (IsCurrentSession(token, allowCanceled)) action();
        });

    private void NotifySavedStateChanged()
    {
        OnPropertyChanged(nameof(IsDirty));
        OnPropertyChanged(nameof(CanSave));
        OnPropertyChanged(nameof(CanDiscardChanges));
        SaveCommand.NotifyCanExecuteChanged();
        DiscardChangesCommand.NotifyCanExecuteChanged();
    }

    private async Task MutateAsync(
        string success,
        Func<CancellationToken, Task> mutation,
        Func<CancellationToken, Task>? refresh = null,
        Action? after = null)
    {
        if (!CanReload)
        {
            return;
        }

        var token = _sessionCancellation!.Token;
        IsLoading = true;
        ErrorMessage = null;
        ActionMessage = null;
        try
        {
            await mutation(token).ConfigureAwait(false);
            if (!IsCurrentSession(token)) return;
            if (refresh is not null)
            {
                await refresh(token).ConfigureAwait(false);
            }

            await InvokeForSessionAsync(() =>
            {
                after?.Invoke();
                ApplyFilter();
                ActionMessage = success;
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await InvokeForSessionAsync(() => ErrorMessage = exception.Message, token).ConfigureAwait(false);
        }
        finally
        {
            await InvokeForSessionAsync(() => IsLoading = false, token, allowCanceled: true).ConfigureAwait(false);
        }
    }

    private Task WithSession(Func<CancellationToken, Task> action) =>
        _sessionCancellation is null ? Task.CompletedTask : action(_sessionCancellation.Token);

    private void ApplyDetail(NotepadNoteDetail detail, bool includeInResults = true)
    {
        var existing = _allNotes.FindIndex(note => note.Id == detail.Note.Id);
        if (existing >= 0) _allNotes[existing] = detail.Note;
        else if (includeInResults) _allNotes.Add(detail.Note);
        SelectedNote = detail.Note;
        SelectedFolder = NormalizeFolder(detail.Note.Folder);
        Title = detail.Note.Title;
        TagsText = string.Join(", ", detail.Note.Tags);
        Content = detail.Content;
        _savedTitle = Title;
        _savedTags = TagsText;
        _savedContent = Content;
        ApplyFilter();
        NotifySavedStateChanged();
    }

    private void ApplyFilter()
    {
        var folder = NormalizeFolder(SelectedFolder);
        Replace(Notes, IsSearchActive ? _allNotes : _allNotes.Where(note => NormalizeFolder(note.Folder) == folder));
        OnPropertyChanged(nameof(SelectedResultNote));
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
        NotifySavedStateChanged();
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

    private void ReplaceFolders(IEnumerable<NotepadFolderItem> values)
    {
        Replace(Folders, values);
        // Native collection consumers must receive the new items before the
        // selected item binding points at one of those instances.
        OnPropertyChanged(nameof(SelectedFolderItem));
    }

    private static void Replace<T>(ObservableCollection<T> target, IEnumerable<T> values)
    {
        target.Clear();
        foreach (var value in values) target.Add(value);
    }

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;
}
