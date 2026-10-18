using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Notepad;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class NotepadCreationTests
{
    [Fact]
    public async Task CreatingSavesTheCurrentDraftBeforeOpeningTheNewNote()
    {
        var service = new CreationService();
        using var model = await OpenAsync(service);
        model.Title = "  Saved title  ";
        model.TagsText = "one，two, ONE";
        model.Content = "# Draft\r\n\r\nKeep spaces.  \r\n";

        await model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("  New note  ", "work\\child"));

        Assert.Equal(["save:one", "create"], service.Mutations);
        var update = Assert.Single(service.Updates);
        Assert.Equal("Saved title", update.Title);
        Assert.Equal(["one", "two"], update.Tags);
        Assert.Equal("# Draft\r\n\r\nKeep spaces.  \r\n", update.Content);
        Assert.Equal("created-1", model.SelectedNote?.Id);
        Assert.Equal("New note", model.Title);
        Assert.Equal("work/child", model.SelectedFolder);
        Assert.Empty(model.Content);
        Assert.False(model.IsDirty);
        Assert.False(model.IsBusy);
        Assert.Contains(model.Notes, note => note.Id == "created-1");
        Assert.Equal(1, service.NoteReads);
        Assert.Equal(2, service.FolderReads);
    }

    [Fact]
    public async Task SaveFailureKeepsDraftAndDoesNotCreateUntilRetrySucceeds()
    {
        var service = new CreationService { SaveError = new IOException("save conflict") };
        using var model = await OpenAsync(service);
        model.Content = "unsaved draft";

        await model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New"));

        Assert.Empty(service.Drafts);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("unsaved draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.Equal("save conflict", model.ErrorMessage);
        Assert.True(model.CreateNoteCommand.CanExecute(null));
        service.SaveError = null;
        await model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New"));
        Assert.Single(service.Drafts);
        Assert.Equal("created-1", model.SelectedNote?.Id);
        Assert.Null(model.ErrorMessage);
    }

    [Theory]
    [InlineData("title")]
    [InlineData("tags")]
    [InlineData("body")]
    public async Task NewEditsDuringAutomaticSavePreventCreation(string field)
    {
        var service = new CreationService { PendingSave = Completion<NotepadNoteDetail>() };
        using var model = await OpenAsync(service);
        model.Content = "submitted draft";
        var creating = model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New"));
        Assert.True(model.IsSaving);
        Edit(model, field);

        service.CompleteSave();
        await creating;

        Assert.Empty(service.Drafts);
        Assert.Equal("one", model.SelectedNote?.Id);
        AssertEdit(model, field);
        Assert.True(model.IsDirty);
        Assert.False(model.IsBusy);
        Assert.Contains("未保存", model.ActionMessage);
    }

    [Theory]
    [InlineData("title")]
    [InlineData("tags")]
    [InlineData("body")]
    public async Task NewEditsDuringCreationStayInTheCurrentEditorAndTheNewNoteRemainsAvailable(string field)
    {
        var service = new CreationService { PendingCreate = Completion<NotepadNoteDetail>() };
        using var model = await OpenAsync(service);
        var creating = model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New"));
        Assert.True(model.IsLoading);
        Edit(model, field);

        service.CompleteCreate();
        await creating;

        Assert.Equal("one", model.SelectedNote?.Id);
        AssertEdit(model, field);
        Assert.True(model.IsDirty);
        Assert.Contains(model.Notes, note => note.Id == "created-1");
        Assert.Contains("新修改已保留", model.ActionMessage);
        await model.SelectNoteCommand.ExecuteAsync(model.Notes.Single(note => note.Id == "created-1"));
        Assert.Single(service.Updates);
        Assert.Equal("created-1", model.SelectedNote?.Id);
        Assert.False(model.IsDirty);
    }

    [Fact]
    public async Task DestinationIsCapturedBeforeAutomaticSaveAndCreationWaitsForSave()
    {
        var service = new CreationService { PendingSave = Completion<NotepadNoteDetail>() };
        using var model = await OpenAsync(service);
        model.SelectFolderCommand.Execute(new NotepadFolderItem("work"));
        model.Content = "draft";
        var creating = model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New"));
        Assert.Empty(service.Drafts);
        model.SelectFolderCommand.Execute(null);

        service.CompleteSave();
        await creating;

        Assert.Equal("work", Assert.Single(service.Drafts).Folder);
        Assert.Equal("work", model.SelectedFolder);
    }

    [Fact]
    public async Task CreationFailurePreservesTheSavedDraftAndAllowsRetry()
    {
        var service = new CreationService { CreateError = new IOException("creation failed") };
        using var model = await OpenAsync(service);
        model.Content = "saved before create";

        await model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New"));

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("saved before create", model.Content);
        Assert.False(model.IsDirty);
        Assert.Equal("creation failed", model.ErrorMessage);
        Assert.Null(model.ActionMessage);
        Assert.False(model.IsBusy);
        service.CreateError = null;
        await model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("Retry"));
        Assert.Equal("created-2", model.SelectedNote?.Id);
        Assert.Single(service.Updates);
        Assert.Null(model.ErrorMessage);
    }

    [Fact]
    public async Task FolderRefreshFailureDoesNotUndoCreationAndRefreshDoesNotCreateAgain()
    {
        var service = new CreationService();
        using var model = await OpenAsync(service);
        service.FolderError = new IOException("folder read failed");

        await model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New", "work/child"));

        Assert.Equal("created-1", model.SelectedNote?.Id);
        Assert.Equal("New", model.Title);
        Assert.Contains(model.Notes, note => note.Id == "created-1");
        Assert.Equal("work", Assert.Single(model.Folders).Path);
        Assert.Contains("已创建", model.ActionMessage);
        Assert.Contains("已创建", model.ErrorMessage);
        Assert.Contains("folder read failed", model.ErrorMessage);
        Assert.Contains("无需重复新建", model.ErrorMessage);
        Assert.False(model.IsBusy);
        service.FolderError = null;
        await model.RefreshCommand.ExecuteAsync(null);
        Assert.Single(service.Drafts);
        Assert.Null(model.ErrorMessage);
        Assert.Equal("created-1", model.SelectedNote?.Id);
    }

    [Fact]
    public async Task TypingIntoCreatedNoteWhileFoldersLoadIsPreserved()
    {
        var service = new CreationService();
        using var model = await OpenAsync(service);
        service.PendingFolders = Completion<IReadOnlyList<string>>();
        var creating = model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New"));
        Assert.Equal("created-1", model.SelectedNote?.Id);
        model.Content = "typed into the new note";

        service.PendingFolders.SetResult(["work", "new folder"]);
        await creating;

        Assert.Equal("typed into the new note", model.Content);
        Assert.True(model.IsDirty);
        Assert.False(model.IsBusy);
        Assert.Equal(2, model.Folders.Count);
        Assert.Equal(1, service.NoteReads);
    }

    [Fact]
    public async Task CreationCannotDuplicateOrStartOtherOperationsWhilePending()
    {
        var service = new CreationService { PendingCreate = Completion<NotepadNoteDetail>() };
        using var model = await OpenAsync(service);
        var creating = model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New"));
        Assert.False(model.CreateNoteCommand.CanExecute(null));
        await model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("Duplicate"));
        await model.SaveCommand.ExecuteAsync(null);
        await model.RefreshCommand.ExecuteAsync(null);
        await model.SearchCommand.ExecuteAsync(null);
        await model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        await model.CreateFolderCommand.ExecuteAsync("blocked");
        Assert.Single(service.Drafts);
        Assert.Empty(service.Updates);
        Assert.Equal(1, service.NoteReads);
        Assert.Equal(1, service.DetailReads);

        service.CompleteCreate();
        await creating;
        Assert.False(model.IsBusy);
        Assert.True(model.CreateNoteCommand.CanExecute(null));
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CreationPreservesActiveSearchResultsAndAnyNewDraft(bool editDuringCreate)
    {
        var service = new CreationService { PendingCreate = Completion<NotepadNoteDetail>() };
        using var model = await OpenAsync(service);
        model.SearchQuery = "Two";
        await model.SearchCommand.ExecuteAsync(null);
        var creating = model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("Unmatched"));
        if (editDuringCreate) model.Content = "new draft";

        service.CompleteCreate();
        await creating;

        Assert.True(model.IsSearchActive);
        Assert.Equal("Two", model.SearchQuery);
        Assert.Equal("two", Assert.Single(model.Notes).Id);
        Assert.Equal(editDuringCreate ? "one" : "created-1", model.SelectedNote?.Id);
        Assert.Equal(editDuringCreate, model.IsDirty);
        model.SearchQuery = "";
        await model.SearchCommand.ExecuteAsync(null);
        Assert.Contains(model.Notes, note => note.Id == "created-1");
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task LateCreationAfterReopenCannotChangeNewSessionOrEndItsLoading(bool fail)
    {
        var service = new CreationService { PendingCreate = Completion<NotepadNoteDetail>() };
        using var model = await OpenAsync(service);
        var creating = model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("Old"));
        await model.CloseAsync();
        service.PendingFolders = Completion<IReadOnlyList<string>>();
        var opening = model.OpenAsync();
        model.Content = "new session draft";
        model.ErrorMessage = "new session error";
        model.ActionMessage = "new session action";

        if (fail) service.PendingCreate.SetException(new IOException("old failure"));
        else service.CompleteCreate();
        await creating;

        Assert.True(model.IsLoading);
        Assert.True(model.IsBusy);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("new session draft", model.Content);
        Assert.Equal("new session error", model.ErrorMessage);
        Assert.Equal("new session action", model.ActionMessage);
        Assert.Equal(2, service.FolderReads);
        service.PendingFolders.SetResult(["work"]);
        await opening;
        Assert.False(model.IsBusy);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CanceledCreationClearsProgressWithoutApplyingLateResultOrError(bool fail)
    {
        var service = new CreationService { PendingCreate = Completion<NotepadNoteDetail>() };
        using var cancellation = new CancellationTokenSource();
        using var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync(cancellation.Token);
        var creating = model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New"));
        model.Content = "draft";
        cancellation.Cancel();
        if (fail) service.PendingCreate.SetException(new IOException("canceled failure"));
        else service.CompleteCreate();
        await creating;

        Assert.False(model.IsBusy);
        Assert.False(model.CreateNoteCommand.CanExecute(null));
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.Null(model.ErrorMessage);
        Assert.Null(model.ActionMessage);
        Assert.Equal(1, service.FolderReads);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task DisposalRejectsLateCreationSuccessAndFailure(bool fail)
    {
        var service = new CreationService { PendingCreate = Completion<NotepadNoteDetail>() };
        using var model = await OpenAsync(service);
        var creating = model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New"));
        model.Content = "kept draft";
        model.Dispose();
        if (fail) service.PendingCreate.SetException(new IOException("late failure"));
        else service.CompleteCreate();
        await creating;

        Assert.False(model.IsOpen);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("kept draft", model.Content);
        Assert.Null(model.ErrorMessage);
        Assert.Null(model.ActionMessage);
        Assert.Equal(1, service.FolderReads);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task QueuedCreationCallbackChecksSessionWhenActuallyApplied(bool fail)
    {
        var service = new CreationService { PendingCreate = Completion<NotepadNoteDetail>() };
        var dispatcher = new DeferredDispatcher();
        using var model = new NotepadViewModel(service, dispatcher);
        await model.OpenAsync();
        var creating = model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("Old"));
        dispatcher.DeferNext = true;
        if (fail) service.PendingCreate.SetException(new IOException("queued old failure"));
        else service.CompleteCreate();
        await dispatcher.Queued.Task.WaitAsync(TimeSpan.FromSeconds(5));
        await model.CloseAsync();
        service.PendingFolders = Completion<IReadOnlyList<string>>();
        var opening = model.OpenAsync();
        model.Content = "new session draft";
        model.ErrorMessage = "new error";
        model.ActionMessage = "new action";

        dispatcher.Release();
        await creating;

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("new session draft", model.Content);
        Assert.Equal("new error", model.ErrorMessage);
        Assert.Equal("new action", model.ActionMessage);
        Assert.True(model.IsLoading);
        Assert.Equal(2, service.FolderReads);
        service.PendingFolders.SetResult(["work"]);
        await opening;
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldFolderRefreshCannotReplaceNewSessionsFoldersOrEndItsLoading(bool fail)
    {
        var service = new CreationService();
        using var model = await OpenAsync(service);
        var oldFolders = Completion<IReadOnlyList<string>>();
        service.PendingFolders = oldFolders;
        var creating = model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("New"));
        Assert.Equal("created-1", model.SelectedNote?.Id);
        await model.CloseAsync();
        service.PendingFolders = Completion<IReadOnlyList<string>>();
        var opening = model.OpenAsync();
        model.Content = "new draft";
        model.ErrorMessage = "new error";

        if (fail) oldFolders.SetException(new IOException("old folder failure"));
        else oldFolders.SetResult(["old folder"]);
        await creating;

        Assert.True(model.IsLoading);
        Assert.Equal("new draft", model.Content);
        Assert.Equal("new error", model.ErrorMessage);
        Assert.Equal("work", Assert.Single(model.Folders).Path);
        service.PendingFolders.SetResult(["new folder"]);
        await opening;
        Assert.Equal("new folder", Assert.Single(model.Folders).Path);
    }

    private sealed class DeferredDispatcher : IUiDispatcher
    {
        private Action? _pending;
        private readonly TaskCompletionSource _released = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public bool DeferNext { get; set; }
        public TaskCompletionSource Queued { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
        {
            if (!DeferNext) { cancellationToken.ThrowIfCancellationRequested(); action(); return Task.CompletedTask; }
            DeferNext = false;
            _pending = action;
            Queued.SetResult();
            return _released.Task;
        }
        public void Release() { _pending!(); _released.SetResult(); }
    }

    private static void Edit(NotepadViewModel model, string field)
    {
        if (field == "title") model.Title = "new title";
        else if (field == "tags") model.TagsText = "new, tags";
        else model.Content = "new body";
    }

    private static void AssertEdit(NotepadViewModel model, string field) =>
        Assert.Equal(field == "title" ? "new title" : field == "tags" ? "new, tags" : "new body",
            field == "title" ? model.Title : field == "tags" ? model.TagsText : model.Content);

    private static TaskCompletionSource<T> Completion<T>() => new(TaskCreationOptions.RunContinuationsAsynchronously);

    private static async Task<NotepadViewModel> OpenAsync(CreationService service)
    {
        var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync();
        return model;
    }

    private sealed class CreationService : INotepadService
    {
        public List<NotepadNote> Notes { get; } =
        [
            new("one", "One", "", [], null, null, "one.md"),
            new("two", "Two", "work", [], null, null, "two.md"),
        ];
        public List<string> Mutations { get; } = [];
        public List<NotepadNoteDraft> Drafts { get; } = [];
        public List<NotepadNoteUpdate> Updates { get; } = [];
        public TaskCompletionSource<NotepadNoteDetail>? PendingSave { get; set; }
        public TaskCompletionSource<NotepadNoteDetail>? PendingCreate { get; set; }
        public TaskCompletionSource<IReadOnlyList<string>>? PendingFolders { get; set; }
        public Exception? SaveError { get; set; }
        public Exception? CreateError { get; set; }
        public Exception? FolderError { get; set; }
        public int NoteReads { get; private set; }
        public int FolderReads { get; private set; }
        public int DetailReads { get; private set; }

        public void CompleteSave() => PendingSave!.SetResult(SavedDetail());
        public void CompleteCreate() => PendingCreate!.SetResult(CreatedDetail());
        private NotepadNoteDetail SavedDetail() => new(Notes[0] with { Title = Updates[^1].Title!, Tags = Updates[^1].Tags! }, "");
        private NotepadNoteDetail CreatedDetail() => new(new NotepadNote($"created-{Drafts.Count}", Drafts[^1].Title, Drafts[^1].Folder, [], null, null, "created.md"), "");
        public Task InitializeAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task<IReadOnlyList<string>> ListFoldersAsync(CancellationToken cancellationToken = default)
        {
            FolderReads++;
            if (FolderError is not null) return Task.FromException<IReadOnlyList<string>>(FolderError);
            return PendingFolders?.Task ?? Task.FromResult<IReadOnlyList<string>>(["work"]);
        }
        public Task<IReadOnlyList<NotepadNote>> ListNotesAsync(string? query, int limit = 500, CancellationToken cancellationToken = default)
        {
            NoteReads++;
            return Task.FromResult<IReadOnlyList<NotepadNote>>(Notes.Where(note => query is null || note.Title.Contains(query, StringComparison.OrdinalIgnoreCase)).ToArray());
        }
        public Task<NotepadNoteDetail> FetchNoteAsync(string id, CancellationToken cancellationToken = default)
        {
            DetailReads++;
            return Task.FromResult(new NotepadNoteDetail(Notes.Single(note => note.Id == id), id == "one" ? "# One" : ""));
        }
        public Task<NotepadNoteDetail> UpdateNoteAsync(string id, NotepadNoteUpdate update, CancellationToken cancellationToken = default)
        {
            Mutations.Add($"save:{id}");
            Updates.Add(update);
            if (SaveError is not null) return Task.FromException<NotepadNoteDetail>(SaveError);
            return PendingSave?.Task ?? Task.FromResult(SavedDetail());
        }
        public async Task<NotepadNoteDetail> CreateNoteAsync(NotepadNoteDraft draft, CancellationToken cancellationToken = default)
        {
            Mutations.Add("create");
            Drafts.Add(draft);
            if (CreateError is not null) throw CreateError;
            var detail = PendingCreate is not null ? await PendingCreate.Task : CreatedDetail();
            Notes.Add(detail.Note);
            return detail;
        }
        public Task CreateFolderAsync(string folder, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task RenameFolderAsync(string from, string to, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DeleteFolderAsync(string folder, bool recursive, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DeleteNoteAsync(string id, CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }
}
