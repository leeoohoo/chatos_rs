using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Notepad;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class NotepadDeletionTests
{
    [Theory]
    [InlineData(false, "title")]
    [InlineData(false, "tags")]
    [InlineData(false, "body")]
    [InlineData(true, "title")]
    [InlineData(true, "tags")]
    [InlineData(true, "body")]
    public async Task DeletingElsewhereKeepsExistingAndNewEdits(bool folder, string field)
    {
        var service = new DeletionService { PendingDelete = Completion() };
        using var model = await OpenAsync(service);
        model.Content = "existing draft";
        var deleting = DeleteAsync(model, service, folder);
        Assert.True(model.IsLoading);
        Assert.False(model.IsEditorReadOnly);
        if (field == "title") model.Title = "new title";
        else if (field == "tags") model.TagsText = "new, tags";
        else model.Content = "new body";
        service.PendingDelete.SetResult();
        await deleting;

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal(field == "title" ? "new title" : "One", model.Title);
        Assert.Equal(field == "tags" ? "new, tags" : "", model.TagsText);
        Assert.Equal(field == "body" ? "new body" : "existing draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.False(model.IsBusy);
        Assert.Empty(service.Updates);
        Assert.Equal(1, service.NoteReads);
        Assert.Null(model.ErrorMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CurrentNoteIsReadOnlyOnlyUntilConfirmedDeletionIsApplied(bool folder)
    {
        var service = new DeletionService();
        using var model = await OpenAsync(service);
        await model.SelectNoteCommand.ExecuteAsync(service.Notes.Single(note => note.Id == "two"));
        model.Content = "confirmed deletion includes this draft";
        service.PendingDelete = Completion();
        var deleting = DeleteAsync(model, service, folder);
        Assert.True(model.IsEditorReadOnly);
        Assert.True(model.IsDirty);
        Assert.False(model.CanSave);

        var foldersStarted = Completion();
        var folders = Completion<IReadOnlyList<string>>();
        service.Folders = _ => { foldersStarted.SetResult(); return folders.Task; };
        service.PendingDelete.SetResult();
        await foldersStarted.Task.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Null(model.SelectedNote);
        Assert.Empty(model.Content);
        Assert.False(model.IsDirty);
        Assert.False(model.IsEditorReadOnly);
        Assert.True(model.IsLoading);
        Assert.NotNull(model.ActionMessage);
        folders.SetResult(["work", "work/child", "workshop"]);
        await deleting;
        Assert.DoesNotContain(model.Notes, note => note.Id == "two");
        Assert.Empty(service.Updates);
    }

    [Fact]
    public async Task FolderDeletionRemovesDescendantsButKeepsSimilarPrefixAndTheirEditor()
    {
        var service = new DeletionService();
        using var model = await OpenAsync(service);
        await model.SelectNoteCommand.ExecuteAsync(service.Notes.Single(note => note.Id == "four"));
        model.Content = "workshop draft";
        await model.DeleteFolderCommand.ExecuteAsync(new NotepadFolderItem(" /work\\ "));

        Assert.Equal("workshop", model.SelectedFolder);
        Assert.Equal("four", model.SelectedNote?.Id);
        Assert.Equal("workshop draft", model.Content);
        Assert.Equal("four", Assert.Single(model.Notes).Id);
        Assert.Equal("workshop", Assert.Single(model.Folders).Path);
        Assert.Equal(["folder:work"], service.Deletions);
        Assert.DoesNotContain(service.Notes, note => note.Id is "two" or "three");
    }

    [Fact]
    public async Task DeletedFolderSelectionReturnsToRootWithoutResettingUnrelatedDraft()
    {
        var service = new DeletionService { PendingDelete = Completion() };
        using var model = await OpenAsync(service);
        model.Content = "draft";
        var deleting = DeleteAsync(model, service, true);
        model.SelectFolderCommand.Execute(new NotepadFolderItem("work/child"));
        service.PendingDelete.SetResult();
        await deleting;

        Assert.Empty(model.SelectedFolder);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("draft", model.Content);
        Assert.Equal("one", Assert.Single(model.Notes).Id);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task FailedDeletionKeepsSelectionDraftAndResourcesAndAllowsRetry(bool folder)
    {
        var service = new DeletionService { DeleteError = new IOException("delete denied") };
        using var model = await OpenAsync(service);
        await model.SelectNoteCommand.ExecuteAsync(service.Notes.Single(note => note.Id == "two"));
        model.Title = "draft title";
        model.Content = "draft";
        await DeleteAsync(model, service, folder);

        Assert.Equal("two", model.SelectedNote?.Id);
        Assert.Equal("draft title", model.Title);
        Assert.Equal("draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.False(model.IsEditorReadOnly);
        Assert.False(model.IsBusy);
        Assert.Equal("delete denied", model.ErrorMessage);
        Assert.Null(model.ActionMessage);
        Assert.Equal(3, model.Folders.Count);
        Assert.Equal(1, service.FolderReads);
        service.DeleteError = null;
        await DeleteAsync(model, service, folder);
        Assert.Null(model.SelectedNote);
        Assert.Null(model.ErrorMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task RefreshFailureKeepsSuccessfulDeletionAndRetryDoesNotDeleteAgain(bool folder)
    {
        var service = new DeletionService();
        using var model = await OpenAsync(service);
        await model.SelectNoteCommand.ExecuteAsync(service.Notes.Single(note => note.Id == "two"));
        service.Folders = _ => throw new IOException("folder list unavailable");
        await DeleteAsync(model, service, folder);

        Assert.Null(model.SelectedNote);
        Assert.Empty(model.Content);
        Assert.NotNull(model.ActionMessage);
        Assert.Contains("删除已完成", model.ErrorMessage);
        Assert.Contains("无需重复删除", model.ErrorMessage);
        Assert.False(model.IsEditorReadOnly);
        if (folder) Assert.Equal("workshop", Assert.Single(model.Folders).Path);
        service.Folders = null;
        await model.RefreshCommand.ExecuteAsync(null);
        Assert.Single(service.Deletions);
        Assert.Null(model.ErrorMessage);
        Assert.DoesNotContain(service.Notes, note => note.Id == "two");
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task DeletionPreservesSearchAndDoesNotRefillFilteredResults(bool folder)
    {
        var service = new DeletionService();
        using var model = await OpenAsync(service);
        model.SearchQuery = "Two";
        await model.SearchCommand.ExecuteAsync(null);
        model.Content = "outside search draft";
        await DeleteAsync(model, service, folder);

        Assert.Empty(model.Notes);
        Assert.True(model.IsSearchActive);
        Assert.Equal("Two", model.SearchQuery);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("outside search draft", model.Content);
        Assert.Equal(2, service.NoteReads);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldDeletionCannotAffectReopenedSessionsDraftErrorOrLoading(bool fail)
    {
        var service = new DeletionService { PendingDelete = Completion() };
        using var model = await OpenAsync(service);
        var deleting = DeleteAsync(model, service, true);
        await model.CloseAsync();
        var folders = Completion<IReadOnlyList<string>>();
        service.Folders = _ => folders.Task;
        var opening = model.OpenAsync();
        model.Content = "new session draft";
        model.ErrorMessage = "new session error";
        model.ActionMessage = "new action";
        if (fail) service.PendingDelete.SetException(new IOException("old failure"));
        else service.PendingDelete.SetResult();
        await deleting;

        Assert.True(model.IsLoading);
        Assert.False(model.IsEditorReadOnly);
        Assert.Equal("new session draft", model.Content);
        Assert.Equal("new session error", model.ErrorMessage);
        Assert.Equal("new action", model.ActionMessage);
        Assert.Equal(2, service.FolderReads);
        Assert.Equal(3, model.Folders.Count);
        folders.SetResult(["new folder"]);
        await opening;
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldFolderRefreshCannotChangeNewSessionsResourcesOrEndLoading(bool fail)
    {
        var service = new DeletionService();
        using var model = await OpenAsync(service);
        var oldFolders = Completion<IReadOnlyList<string>>();
        service.Folders = _ => oldFolders.Task;
        var deleting = DeleteAsync(model, service, true);
        await model.CloseAsync();
        var newFolders = Completion<IReadOnlyList<string>>();
        service.Folders = _ => newFolders.Task;
        var opening = model.OpenAsync();
        model.ErrorMessage = "current error";
        if (fail) oldFolders.SetException(new IOException("old list failure"));
        else oldFolders.SetResult(["stale"]);
        await deleting;

        Assert.True(model.IsLoading);
        Assert.Equal("current error", model.ErrorMessage);
        Assert.Equal("workshop", Assert.Single(model.Folders).Path);
        newFolders.SetResult(["new folder"]);
        await opening;
        Assert.Equal("new folder", Assert.Single(model.Folders).Path);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CancellationReleasesReadOnlyAndDoesNotReportFailureOrRefresh(bool folder)
    {
        using var cancellation = new CancellationTokenSource();
        var service = new DeletionService { PendingDelete = Completion() };
        using var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync(cancellation.Token);
        await model.SelectNoteCommand.ExecuteAsync(service.Notes.Single(note => note.Id == "two"));
        var deleting = DeleteAsync(model, service, folder);
        Assert.True(model.IsEditorReadOnly);
        cancellation.Cancel();
        service.PendingDelete.SetCanceled(cancellation.Token);
        await deleting;

        Assert.False(model.IsEditorReadOnly);
        Assert.False(model.IsBusy);
        Assert.Equal("two", model.SelectedNote?.Id);
        Assert.Null(model.ErrorMessage);
        Assert.Null(model.ActionMessage);
        Assert.Equal(1, service.FolderReads);
    }

    [Theory]
    [InlineData("")]
    [InlineData(" / ")]
    [InlineData(".\\..")]
    public async Task RootCannotBeRecursivelyDeleted(string path)
    {
        var service = new DeletionService();
        using var model = await OpenAsync(service);
        await model.DeleteFolderCommand.ExecuteAsync(new NotepadFolderItem(path));
        Assert.Empty(service.Deletions);
        Assert.Equal("one", model.SelectedNote?.Id);
    }

    [Fact]
    public async Task BusyDeletionRejectsOtherMutationsAndClosedDeletionDoesNotStart()
    {
        var service = new DeletionService { PendingDelete = Completion() };
        using var model = await OpenAsync(service);
        var deleting = DeleteAsync(model, service, false);
        await model.DeleteFolderCommand.ExecuteAsync(new NotepadFolderItem("work"));
        await model.CreateFolderCommand.ExecuteAsync("new folder");
        await model.SelectNoteCommand.ExecuteAsync(service.Notes.Single(note => note.Id == "four"));
        await model.RefreshCommand.ExecuteAsync(null);
        Assert.Single(service.Deletions);
        Assert.Equal("one", model.SelectedNote?.Id);
        service.PendingDelete.SetResult();
        await deleting;
        await model.CloseAsync();
        await model.DeleteFolderCommand.ExecuteAsync(new NotepadFolderItem("workshop"));
        Assert.Single(service.Deletions);
    }

    [Fact]
    public async Task QueuedDeletionCallbackChecksSessionBeforeRemovingResources()
    {
        var service = new DeletionService { PendingDelete = Completion() };
        var dispatcher = new DeferredDispatcher();
        using var model = new NotepadViewModel(service, dispatcher);
        await model.OpenAsync();
        var deleting = DeleteAsync(model, service, true);
        dispatcher.DeferNext = true;
        service.PendingDelete.SetResult();
        await dispatcher.Queued.Task.WaitAsync(TimeSpan.FromSeconds(5));
        await model.CloseAsync();
        model.Content = "kept draft";
        dispatcher.Release();
        await deleting;
        Assert.Equal(3, model.Folders.Count);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("kept draft", model.Content);
        Assert.Equal(1, service.FolderReads);
    }

    private static Task DeleteAsync(NotepadViewModel model, DeletionService service, bool folder) => folder
        ? model.DeleteFolderCommand.ExecuteAsync(new NotepadFolderItem("work"))
        : model.DeleteNoteCommand.ExecuteAsync(service.Notes.Single(note => note.Id == "two"));
    private static TaskCompletionSource Completion() => new(TaskCreationOptions.RunContinuationsAsynchronously);
    private static TaskCompletionSource<T> Completion<T>() => new(TaskCreationOptions.RunContinuationsAsynchronously);
    private static async Task<NotepadViewModel> OpenAsync(DeletionService service)
    {
        var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync();
        return model;
    }

    private sealed class DeferredDispatcher : IUiDispatcher
    {
        private Action? _pending;
        private readonly TaskCompletionSource _released = Completion();
        public bool DeferNext { get; set; }
        public TaskCompletionSource Queued { get; } = Completion();
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

    private sealed class DeletionService : INotepadService
    {
        public List<NotepadNote> Notes { get; } =
        [
            new("one", "One", "", [], null, null, "one.md"),
            new("two", "Two", "work", [], null, null, "two.md"),
            new("three", "Three", "work/child", [], null, null, "three.md"),
            new("four", "Four", "workshop", [], null, null, "four.md"),
        ];
        private readonly List<string> _folders = ["work", "work/child", "workshop"];
        public TaskCompletionSource? PendingDelete { get; set; }
        public Exception? DeleteError { get; set; }
        public Func<CancellationToken, Task<IReadOnlyList<string>>>? Folders { get; set; }
        public List<string> Deletions { get; } = [];
        public List<NotepadNoteUpdate> Updates { get; } = [];
        public int NoteReads { get; private set; }
        public int FolderReads { get; private set; }
        public Task InitializeAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task<IReadOnlyList<string>> ListFoldersAsync(CancellationToken cancellationToken = default)
        {
            FolderReads++;
            return Folders?.Invoke(cancellationToken) ?? Task.FromResult<IReadOnlyList<string>>(_folders.ToArray());
        }
        public Task<IReadOnlyList<NotepadNote>> ListNotesAsync(string? query, int limit = 500, CancellationToken cancellationToken = default)
        {
            NoteReads++;
            return Task.FromResult<IReadOnlyList<NotepadNote>>(Notes.Where(note => query is null || note.Title.Contains(query)).ToArray());
        }
        public Task<NotepadNoteDetail> FetchNoteAsync(string id, CancellationToken cancellationToken = default) =>
            Task.FromResult(new NotepadNoteDetail(Notes.Single(note => note.Id == id), "# " + id));
        public Task<NotepadNoteDetail> UpdateNoteAsync(string id, NotepadNoteUpdate update, CancellationToken cancellationToken = default)
        {
            Updates.Add(update);
            return Task.FromResult(new NotepadNoteDetail(Notes.Single(note => note.Id == id), update.Content!));
        }
        public async Task DeleteNoteAsync(string id, CancellationToken cancellationToken = default)
        {
            Deletions.Add("note:" + id);
            if (DeleteError is { } error) throw error;
            if (PendingDelete is { } pending) await pending.Task;
            Notes.RemoveAll(note => note.Id == id);
        }
        public async Task DeleteFolderAsync(string folder, bool recursive, CancellationToken cancellationToken = default)
        {
            Assert.True(recursive);
            Deletions.Add("folder:" + folder);
            if (DeleteError is { } error) throw error;
            if (PendingDelete is { } pending) await pending.Task;
            Notes.RemoveAll(note => note.Folder == folder || note.Folder.StartsWith(folder + "/", StringComparison.Ordinal));
            _folders.RemoveAll(path => path == folder || path.StartsWith(folder + "/", StringComparison.Ordinal));
        }
        public Task CreateFolderAsync(string folder, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task RenameFolderAsync(string from, string to, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<NotepadNoteDetail> CreateNoteAsync(NotepadNoteDraft draft, CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }
}
