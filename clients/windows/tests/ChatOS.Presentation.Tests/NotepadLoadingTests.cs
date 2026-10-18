using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Notepad;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class NotepadLoadingTests
{
    [Theory]
    [InlineData("title")]
    [InlineData("tags")]
    [InlineData("body")]
    public async Task EditingDuringDetailReadKeepsCurrentNoteAndAllowsSavingBeforeRetry(string field)
    {
        var service = new LoadingService();
        using var model = await OpenAsync(service);
        var pending = Completion<NotepadNoteDetail>();
        service.Fetch = (_, _) => pending.Task;
        var selecting = model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        Assert.True(model.IsLoading);
        if (field == "title") model.Title = "new title";
        else if (field == "tags") model.TagsText = "new, tags";
        else model.Content = "new body";

        pending.SetResult(service.Detail("two"));
        await selecting;

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal(field == "title" ? "new title" : "One", model.Title);
        Assert.Equal(field == "tags" ? "new, tags" : "", model.TagsText);
        Assert.Equal(field == "body" ? "new body" : "# One", model.Content);
        Assert.True(model.IsDirty);
        Assert.False(model.IsBusy);
        Assert.Contains("已保留", model.ActionMessage);
        service.Fetch = null;
        await model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        Assert.Single(service.Updates);
        Assert.Equal("two", model.SelectedNote?.Id);
        Assert.Equal("# Two", model.Content);
    }

    [Fact]
    public async Task UnchangedEditorCanSwitchAndFailedReadCanRetry()
    {
        var service = new LoadingService();
        using var model = await OpenAsync(service);
        service.Fetch = (_, _) => Task.FromException<NotepadNoteDetail>(new IOException("read failed"));
        model.ActionMessage = "old success";

        await model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("# One", model.Content);
        Assert.Equal("read failed", model.ErrorMessage);
        Assert.Null(model.ActionMessage);
        Assert.False(model.IsBusy);
        service.Fetch = null;
        await model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        Assert.Equal("two", model.SelectedNote?.Id);
        Assert.Null(model.ErrorMessage);
        Assert.False(model.IsDirty);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldDetailCompletionCannotChangeReopenedSessionOrEndNewDetailRead(bool fail)
    {
        var service = new LoadingService();
        using var model = await OpenAsync(service);
        var old = Completion<NotepadNoteDetail>();
        service.Fetch = (_, _) => old.Task;
        var oldSelecting = model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        await model.CloseAsync();
        await model.OpenAsync();
        var current = Completion<NotepadNoteDetail>();
        service.Fetch = (_, _) => current.Task;
        var selecting = model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        model.Content = "new session draft";
        model.ErrorMessage = "new error";
        model.ActionMessage = "new action";
        if (fail) old.SetException(new IOException("old failure"));
        else old.SetResult(service.Detail("two"));
        await oldSelecting;

        Assert.True(model.IsLoading);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("new session draft", model.Content);
        Assert.Equal("new error", model.ErrorMessage);
        Assert.Equal("new action", model.ActionMessage);
        current.SetResult(service.Detail("two"));
        await selecting;
        Assert.False(model.IsBusy);
        Assert.Equal("new session draft", model.Content);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task DetailCancellationEndsProgressAndDoesNotPropagateOrApplyLateResponse(bool serviceThrowsCancellation)
    {
        var service = new LoadingService();
        using var cancellation = new CancellationTokenSource();
        using var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync(cancellation.Token);
        var pending = Completion<NotepadNoteDetail>();
        service.Fetch = (_, _) => pending.Task;
        var selecting = model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        cancellation.Cancel();
        if (serviceThrowsCancellation) pending.SetCanceled(cancellation.Token);
        else pending.SetResult(service.Detail("two"));
        await selecting;

        Assert.False(model.IsBusy);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("# One", model.Content);
        Assert.Null(model.ErrorMessage);
        Assert.False(model.CreateNoteCommand.CanExecute(null));
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task DisposedDetailReadDoesNotChangeTheEditorOrStatus(bool fail)
    {
        var service = new LoadingService();
        using var model = await OpenAsync(service);
        var pending = Completion<NotepadNoteDetail>();
        service.Fetch = (_, _) => pending.Task;
        var selecting = model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        model.Content = "kept draft";
        model.Dispose();
        if (fail) pending.SetException(new IOException("disposed failure"));
        else pending.SetResult(service.Detail("two"));
        await selecting;

        Assert.False(model.IsOpen);
        Assert.False(model.IsBusy);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("kept draft", model.Content);
        Assert.Null(model.ErrorMessage);
        Assert.Null(model.ActionMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task QueuedDetailCallbackChecksBothSessionAndEditorAtApplicationTime(bool reopen)
    {
        var service = new LoadingService();
        var dispatcher = new DeferredDispatcher();
        using var model = new NotepadViewModel(service, dispatcher);
        await model.OpenAsync();
        var pending = Completion<NotepadNoteDetail>();
        service.Fetch = (_, _) => pending.Task;
        var selecting = model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        dispatcher.DeferNext = true;
        pending.SetResult(service.Detail("two"));
        await dispatcher.Queued.Task.WaitAsync(TimeSpan.FromSeconds(5));
        if (reopen) { await model.CloseAsync(); await model.OpenAsync(); }
        model.Content = "queued draft";
        model.ErrorMessage = "current error";

        dispatcher.Release();
        await selecting;

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("queued draft", model.Content);
        Assert.Equal("current error", model.ErrorMessage);
        Assert.True(model.IsDirty);
        Assert.False(model.IsBusy);
    }

    [Theory]
    [InlineData("initialize", false)]
    [InlineData("initialize", true)]
    [InlineData("resources", false)]
    [InlineData("resources", true)]
    public async Task OldFullLoadCannotChangeNewSessionOrStartAStaleFollowupRead(string stage, bool fail)
    {
        var service = new LoadingService();
        using var model = await OpenAsync(service);
        var oldInitialize = Completion();
        var oldNotes = Completion<IReadOnlyList<NotepadNote>>();
        if (stage == "initialize") service.Initialize = _ => oldInitialize.Task;
        else service.List = (_, _) => oldNotes.Task;
        var refreshing = model.RefreshCommand.ExecuteAsync(null);
        await model.CloseAsync();
        service.Initialize = null;
        var newFolders = Completion<IReadOnlyList<string>>();
        service.Folders = _ => newFolders.Task;
        service.List = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>(service.Notes);
        var opening = model.OpenAsync();
        model.Content = "current draft";
        model.ErrorMessage = "current error";
        var reads = service.NoteReads;
        if (stage == "initialize")
        {
            if (fail) oldInitialize.SetException(new IOException("old initialize error"));
            else oldInitialize.SetResult();
        }
        else if (fail) oldNotes.SetException(new IOException("old list error"));
        else oldNotes.SetResult([]);
        await refreshing.WaitAsync(TimeSpan.FromSeconds(5));

        Assert.True(model.IsLoading);
        Assert.Equal("current draft", model.Content);
        Assert.Equal("current error", model.ErrorMessage);
        Assert.Equal(reads, service.NoteReads);
        Assert.Contains(model.Notes, note => note.Id == "one");
        newFolders.SetResult(["current folder"]);
        await opening;
        Assert.False(model.IsBusy);
        Assert.Equal("current folder", Assert.Single(model.Folders).Path);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task QueuedResourceCallbackCannotReplaceNewSessionResources(bool fail)
    {
        var service = new LoadingService();
        var dispatcher = new DeferredDispatcher();
        using var model = new NotepadViewModel(service, dispatcher);
        await model.OpenAsync();
        var pending = Completion<IReadOnlyList<NotepadNote>>();
        service.List = (_, _) => pending.Task;
        var refreshing = model.RefreshCommand.ExecuteAsync(null);
        dispatcher.DeferNext = true;
        if (fail) pending.SetException(new IOException("queued error"));
        else pending.SetResult([]);
        await dispatcher.Queued.Task.WaitAsync(TimeSpan.FromSeconds(5));
        await model.CloseAsync();
        service.List = null;
        await model.OpenAsync();
        model.Content = "new draft";
        model.ErrorMessage = "new error";

        dispatcher.Release();
        await refreshing;

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("one", Assert.Single(model.Notes).Id);
        Assert.Equal("new draft", model.Content);
        Assert.Equal("new error", model.ErrorMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldMutationDoesNotRefreshOrResetReopenedEditor(bool fail)
    {
        var service = new LoadingService();
        using var model = await OpenAsync(service);
        var pending = Completion();
        service.Mutation = _ => pending.Task;
        var deleting = model.DeleteNoteCommand.ExecuteAsync(service.Notes[0]);
        await model.CloseAsync();
        var currentFolders = Completion<IReadOnlyList<string>>();
        service.Folders = _ => currentFolders.Task;
        var opening = model.OpenAsync();
        model.Content = "new draft";
        model.ErrorMessage = "new error";
        model.ActionMessage = "new action";
        var reads = service.NoteReads;
        if (fail) pending.SetException(new IOException("old mutation failure"));
        else pending.SetResult();
        await deleting;

        Assert.True(model.IsLoading);
        Assert.Equal(reads, service.NoteReads);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("new draft", model.Content);
        Assert.Equal("new error", model.ErrorMessage);
        Assert.Equal("new action", model.ActionMessage);
        currentFolders.SetResult(["work"]);
        await opening;
    }

    [Fact]
    public async Task InitialDetailFailureCanBeRetriedWithoutLeavingLoadingActive()
    {
        var service = new LoadingService { Fetch = (_, _) => Task.FromException<NotepadNoteDetail>(new IOException("initial read failed")) };
        using var model = await OpenAsync(service);
        Assert.Null(model.SelectedNote);
        Assert.Equal("initial read failed", model.ErrorMessage);
        Assert.False(model.IsBusy);
        Assert.Equal("one", Assert.Single(model.Notes).Id);

        service.Fetch = null;
        await model.SelectNoteCommand.ExecuteAsync(service.Notes[0]);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("# One", model.Content);
        Assert.Null(model.ErrorMessage);
    }

    [Fact]
    public async Task CurrentResourceFailurePreservesDraftAndResourcesAndCanRetry()
    {
        var service = new LoadingService();
        using var model = await OpenAsync(service);
        model.Content = "draft";
        service.List = (_, _) => Task.FromException<IReadOnlyList<NotepadNote>>(new IOException("list failed"));
        await model.RefreshCommand.ExecuteAsync(null);

        Assert.False(model.IsBusy);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("one", Assert.Single(model.Notes).Id);
        Assert.Equal("work", Assert.Single(model.Folders).Path);
        Assert.Equal("draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.Equal("list failed", model.ErrorMessage);
        service.List = null;
        await model.RefreshCommand.ExecuteAsync(null);
        Assert.Null(model.ErrorMessage);
        Assert.Equal("draft", model.Content);
        Assert.True(model.IsDirty);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CanceledMutationDoesNotRefreshOrReportSuccessAndEndsProgress(bool serviceThrowsCancellation)
    {
        var service = new LoadingService();
        using var cancellation = new CancellationTokenSource();
        using var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync(cancellation.Token);
        var pending = Completion();
        service.Mutation = _ => pending.Task;
        var mutation = model.CreateFolderCommand.ExecuteAsync("new folder");
        model.Content = "draft";
        cancellation.Cancel();
        if (serviceThrowsCancellation) pending.SetCanceled(cancellation.Token);
        else pending.SetResult();
        await mutation;

        Assert.False(model.IsBusy);
        Assert.Equal(1, service.NoteReads);
        Assert.Equal("draft", model.Content);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Null(model.ErrorMessage);
        Assert.Null(model.ActionMessage);
    }

    [Fact]
    public async Task ClosingEndsLoadingImmediatelyAndKeepsDraft()
    {
        var service = new LoadingService();
        using var model = await OpenAsync(service);
        var pending = Completion<NotepadNoteDetail>();
        service.Fetch = (_, _) => pending.Task;
        var selecting = model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        model.Content = "draft";
        await model.CloseAsync();
        var busyAfterClose = model.IsBusy;
        pending.SetResult(service.Detail("two"));
        try { await selecting; } catch (OperationCanceledException) { }

        Assert.False(busyAfterClose);
        Assert.Equal("draft", model.Content);
        Assert.True(model.IsDirty);
    }

    private static TaskCompletionSource Completion() => new(TaskCreationOptions.RunContinuationsAsynchronously);
    private static TaskCompletionSource<T> Completion<T>() => new(TaskCreationOptions.RunContinuationsAsynchronously);
    private static async Task<NotepadViewModel> OpenAsync(LoadingService service)
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

    private sealed class LoadingService : INotepadService
    {
        public NotepadNote[] Notes { get; } =
        [
            new("one", "One", "", [], null, null, "one.md"),
            new("two", "Two", "work", [], null, null, "two.md"),
        ];
        public Func<CancellationToken, Task>? Initialize { get; set; }
        public Func<CancellationToken, Task<IReadOnlyList<string>>>? Folders { get; set; }
        public Func<string?, CancellationToken, Task<IReadOnlyList<NotepadNote>>>? List { get; set; }
        public Func<string, CancellationToken, Task<NotepadNoteDetail>>? Fetch { get; set; }
        public Func<CancellationToken, Task>? Mutation { get; set; }
        public int NoteReads { get; private set; }
        public List<NotepadNoteUpdate> Updates { get; } = [];
        public NotepadNoteDetail Detail(string id) => new(Notes.Single(note => note.Id == id), id == "one" ? "# One" : "# Two");
        public Task InitializeAsync(CancellationToken cancellationToken = default) => Initialize?.Invoke(cancellationToken) ?? Task.CompletedTask;
        public Task<IReadOnlyList<string>> ListFoldersAsync(CancellationToken cancellationToken = default) => Folders?.Invoke(cancellationToken) ?? Task.FromResult<IReadOnlyList<string>>(["work"]);
        public Task<IReadOnlyList<NotepadNote>> ListNotesAsync(string? query, int limit = 500, CancellationToken cancellationToken = default)
        {
            NoteReads++;
            return List?.Invoke(query, cancellationToken) ?? Task.FromResult<IReadOnlyList<NotepadNote>>(Notes);
        }
        public Task<NotepadNoteDetail> FetchNoteAsync(string id, CancellationToken cancellationToken = default) => Fetch?.Invoke(id, cancellationToken) ?? Task.FromResult(Detail(id));
        public Task<NotepadNoteDetail> UpdateNoteAsync(string id, NotepadNoteUpdate update, CancellationToken cancellationToken = default)
        {
            Updates.Add(update);
            return Task.FromResult(new NotepadNoteDetail(Notes.Single(note => note.Id == id) with { Title = update.Title!, Tags = update.Tags! }, ""));
        }
        public Task CreateFolderAsync(string folder, CancellationToken cancellationToken = default) => Mutation?.Invoke(cancellationToken) ?? Task.CompletedTask;
        public Task RenameFolderAsync(string from, string to, CancellationToken cancellationToken = default) => Mutation?.Invoke(cancellationToken) ?? Task.CompletedTask;
        public Task DeleteFolderAsync(string folder, bool recursive, CancellationToken cancellationToken = default) => Mutation?.Invoke(cancellationToken) ?? Task.CompletedTask;
        public Task DeleteNoteAsync(string id, CancellationToken cancellationToken = default) => Mutation?.Invoke(cancellationToken) ?? Task.CompletedTask;
        public Task<NotepadNoteDetail> CreateNoteAsync(NotepadNoteDraft draft, CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }
}
