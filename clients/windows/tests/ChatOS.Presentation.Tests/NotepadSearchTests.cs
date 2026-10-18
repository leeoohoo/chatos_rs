using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Notepad;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class NotepadSearchTests
{
    [Fact]
    public async Task SearchRebindsSelectionToCurrentResultInstanceWithoutReplacingDraft()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        var editorNote = model.SelectedNote;
        model.Content = "unsaved draft";
        var result = service.AllNotes[0] with { Title = "Updated metadata" };
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([result]);
        var notifiedSelections = new List<NotepadNote?>();
        model.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(model.SelectedResultNote)) notifiedSelections.Add(model.SelectedResultNote);
        };

        model.SearchQuery = "match";
        await model.SearchCommand.ExecuteAsync(null);

        Assert.Same(result, model.SelectedResultNote);
        Assert.Same(Assert.Single(model.Notes), notifiedSelections.Last());
        Assert.Same(editorNote, model.SelectedNote);
        Assert.Equal("One", model.Title);
        Assert.Equal("unsaved draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.Equal(0, service.Saves);
    }

    [Fact]
    public async Task HiddenEditorSelectionReturnsWhenFolderOrSearchIncludesItAgain()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        var editorNote = model.SelectedNote;
        model.Content = "unsaved draft";
        model.SelectFolderCommand.Execute(new NotepadFolderItem("work"));
        Assert.Null(model.SelectedResultNote);
        Assert.Same(editorNote, model.SelectedNote);

        model.SearchQuery = "global";
        await model.SearchCommand.ExecuteAsync(null);
        Assert.Same(model.Notes.Single(note => note.Id == "one"), model.SelectedResultNote);

        model.SearchQuery = "";
        await model.SearchCommand.ExecuteAsync(null);
        Assert.Null(model.SelectedResultNote);
        model.SelectFolderCommand.Execute(null);
        Assert.Same(Assert.Single(model.Notes), model.SelectedResultNote);
        Assert.Equal("unsaved draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.Equal(0, service.Saves);
    }

    [Fact]
    public async Task SwitchingGlobalResultSelectsDestinationAndClearKeepsItsFolder()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        model.SearchQuery = "global";
        await model.SearchCommand.ExecuteAsync(null);
        model.Content = "save before switching";

        await model.SelectNoteCommand.ExecuteAsync(model.Notes.Single(note => note.Id == "two"));

        Assert.Equal(1, service.Saves);
        Assert.Equal("two", model.SelectedResultNote?.Id);
        Assert.Equal("work", model.SelectedFolder);
        Assert.False(model.IsDirty);
        model.SearchQuery = "";
        await model.SearchCommand.ExecuteAsync(null);
        Assert.Same(Assert.Single(model.Notes), model.SelectedResultNote);
        Assert.Equal("two", model.SelectedResultNote?.Id);
    }

    [Fact]
    public async Task EmptySearchClearsOnlyListSelectionAndNotifiesAfterResultsReturn()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        var selections = new List<NotepadNote?>();
        model.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(model.SelectedResultNote)) selections.Add(model.SelectedResultNote);
        };
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([]);
        model.SearchQuery = "missing";
        await model.SearchCommand.ExecuteAsync(null);
        Assert.Null(selections.Last());
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("# One", model.Content);

        service.Search = null;
        model.SearchQuery = "";
        await model.SearchCommand.ExecuteAsync(null);
        Assert.Same(Assert.Single(model.Notes), selections.Last());
    }

    [Fact]
    public async Task SaveStartingDuringDebounceDoesNotDropLatestQuery()
    {
        var service = new SearchService();
        var dispatcher = new ObservingDispatcher();
        using var model = new NotepadViewModel(service, dispatcher);
        await model.OpenAsync();
        model.SearchQuery = "after save";
        var pending = model.ScheduleSearchCommand.ExecutionTask!;
        model.IsSaving = true;
        var waitedForSave = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        dispatcher.AfterInvoke = () => { if (model.IsSaving) waitedForSave.TrySetResult(); };
        await waitedForSave.Task.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Equal(1, service.NoteReads);
        Assert.False(pending.IsCompleted);
        model.IsSaving = false;
        await pending.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Equal("after save", service.LastQuery);
        Assert.Equal(2, service.NoteReads);
    }

    private sealed class ObservingDispatcher : IUiDispatcher
    {
        public Action? AfterInvoke { get; set; }
        public Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            action();
            AfterInvoke?.Invoke();
            return Task.CompletedTask;
        }
    }

    [Fact]
    public async Task TypingDebouncesToLatestQueryWithoutSavingDraft()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        model.Content = "unsaved draft";
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([]);
        model.SearchQuery = "first";
        var first = model.ScheduleSearchCommand.ExecutionTask!;
        model.SearchQuery = "  latest  ";
        var latest = model.ScheduleSearchCommand.ExecutionTask!;
        Assert.Equal(1, service.NoteReads);
        await Task.WhenAll(first, latest).WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Equal(2, service.NoteReads);
        Assert.Equal("latest", service.LastQuery);
        Assert.Empty(model.Notes);
        Assert.True(model.IsSearchActive);
        Assert.Equal("unsaved draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.Equal(0, service.Saves);
    }

    [Fact]
    public async Task ClearingQueryAutomaticallyRestoresFolderResults()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        model.SearchQuery = "global";
        await model.ScheduleSearchCommand.ExecutionTask!.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Equal(2, model.Notes.Count);
        model.SearchQuery = "";
        await model.ScheduleSearchCommand.ExecutionTask!.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Null(service.LastQuery);
        Assert.False(model.IsSearchActive);
        Assert.Equal("one", Assert.Single(model.Notes).Id);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CloseOrDisposeCancelsScheduledSearch(bool dispose)
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        model.SearchQuery = "pending";
        var pending = model.ScheduleSearchCommand.ExecutionTask!;
        if (dispose) model.Dispose(); else await model.CloseAsync();
        await pending.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Equal(1, service.NoteReads);
        Assert.False(model.IsBusy);
        Assert.Null(model.ErrorMessage);
    }

    [Fact]
    public async Task ManualSearchCancelsPendingDebounceWithoutDuplicateRequest()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        model.SearchQuery = "manual";
        var pending = model.ScheduleSearchCommand.ExecutionTask!;
        await model.SearchCommand.ExecuteAsync(null);
        await pending.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Equal(2, service.NoteReads);
        Assert.Equal("manual", service.LastQuery);
    }

    [Fact]
    public async Task NewTextImmediatelyCancelsOldSearchBeforeDebounceExpires()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        var response = Completion();
        CancellationToken oldToken = default;
        service.Search = (_, token) => { oldToken = token; return response.Task; };
        model.SearchQuery = "old";
        var oldSearch = model.SearchCommand.ExecuteAsync(null);
        model.SearchQuery = "new";
        var pending = model.ScheduleSearchCommand.ExecutionTask!;
        Assert.True(oldToken.IsCancellationRequested);
        response.SetResult([]);
        await oldSearch;
        Assert.Equal("one", Assert.Single(model.Notes).Id);
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([service.AllNotes[1]]);
        await pending.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Equal("new", service.LastQuery);
        Assert.Equal("two", Assert.Single(model.Notes).Id);
    }

    [Fact]
    public async Task DebouncedFailureIsShownAndNextTextRetries()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        service.Search = (_, _) => Task.FromException<IReadOnlyList<NotepadNote>>(new IOException("offline"));
        model.SearchQuery = "failed";
        await model.ScheduleSearchCommand.ExecutionTask!.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Equal("offline", model.ErrorMessage);
        Assert.Equal("one", Assert.Single(model.Notes).Id);
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([]);
        model.SearchQuery = "retry";
        await model.ScheduleSearchCommand.ExecutionTask!.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Null(model.ErrorMessage);
        Assert.Empty(model.Notes);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task SearchDoesNotResetEditorWhenCurrentNoteDoesNotMatch(bool dirty)
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        if (dirty) { model.Title = "draft title"; model.TagsText = "draft tags"; model.Content = "draft body"; }
        model.SearchQuery = "  match  ";
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([service.AllNotes[1]]);

        await model.SearchCommand.ExecuteAsync(null);

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal(dirty ? "draft title" : "One", model.Title);
        Assert.Equal(dirty ? "draft tags" : "", model.TagsText);
        Assert.Equal(dirty ? "draft body" : "# One", model.Content);
        Assert.Equal(dirty, model.IsDirty);
        Assert.Equal("two", Assert.Single(model.Notes).Id);
        Assert.Equal("match", service.LastQuery);
        Assert.Equal(1, service.FolderReads);
        Assert.Equal(1, service.DetailReads);
        Assert.Equal(0, service.Saves);
    }

    [Fact]
    public async Task SearchFindsNotesAcrossFoldersAndClearingReturnsToFolder()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        model.SelectFolderCommand.Execute(new NotepadFolderItem("work"));
        model.SearchQuery = "global";
        await model.SearchCommand.ExecuteAsync(null);
        Assert.Equal(2, model.Notes.Count);
        model.SearchQuery = "  ";
        await model.SearchCommand.ExecuteAsync(null);

        Assert.Null(service.LastQuery);
        Assert.Equal("two", Assert.Single(model.Notes).Id);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("# One", model.Content);
        Assert.False(model.IsDirty);
    }

    [Fact]
    public async Task SearchFailureKeepsResultsAndDraftAndCanRetry()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        model.Content = "draft";
        model.SearchQuery = "query";
        service.Search = (_, _) => Task.FromException<IReadOnlyList<NotepadNote>>(new IOException("search unavailable"));

        await model.SearchCommand.ExecuteAsync(null);

        Assert.Equal("search unavailable", model.ErrorMessage);
        Assert.Equal("one", Assert.Single(model.Notes).Id);
        Assert.Equal("draft", model.Content);
        Assert.True(model.IsDirty);
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([]);
        await model.SearchCommand.ExecuteAsync(null);
        Assert.Null(model.ErrorMessage);
        Assert.Empty(model.Notes);
        Assert.Equal("draft", model.Content);
        Assert.True(model.IsDirty);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldSearchCannotReplaceNewResultsOrErrors(bool fail)
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        var oldResponse = Completion();
        CancellationToken oldToken = default;
        service.Search = (_, token) => { oldToken = token; return oldResponse.Task; };
        model.SearchQuery = "old";
        var oldSearch = model.SearchCommand.ExecuteAsync(null);
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([service.AllNotes[1]]);
        model.SearchQuery = "new";
        await model.SearchCommand.ExecuteAsync(null);
        Assert.True(oldToken.IsCancellationRequested);
        model.ErrorMessage = "new status";

        if (fail) oldResponse.SetException(new IOException("old failure"));
        else oldResponse.SetResult([]);
        try { await oldSearch; } catch (OperationCanceledException) { }

        Assert.Equal("two", Assert.Single(model.Notes).Id);
        Assert.Equal("new status", model.ErrorMessage);
        Assert.True(model.CanReload);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldSearchDoesNotChangeReopenedSessionOrEndNewSearch(bool fail)
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        var oldResponse = Completion();
        service.Search = (_, _) => oldResponse.Task;
        model.SearchQuery = "old";
        var oldSearch = model.SearchCommand.ExecuteAsync(null);
        await model.CloseAsync();
        service.Search = null;
        await model.OpenAsync();
        var newResponse = Completion();
        service.Search = (_, _) => newResponse.Task;
        model.SearchQuery = "new";
        var newSearch = model.SearchCommand.ExecuteAsync(null);
        model.Content = "new draft";
        model.ErrorMessage = "new status";
        if (fail) oldResponse.SetException(new IOException("old failure"));
        else oldResponse.SetResult([]);
        try { await oldSearch; } catch (OperationCanceledException) { }

        Assert.True(model.IsBusy);
        Assert.Equal("new status", model.ErrorMessage);
        Assert.Equal("new draft", model.Content);
        newResponse.SetResult([service.AllNotes[1]]);
        await newSearch;
        Assert.False(model.IsBusy);
        Assert.Equal("two", Assert.Single(model.Notes).Id);
    }

    [Fact]
    public async Task SearchBusyStateBlocksSaveRefreshAndSelectionUntilCompletion()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        model.Content = "draft";
        var response = Completion();
        service.Search = (_, _) => response.Task;
        model.SearchQuery = "slow";
        var searching = model.SearchCommand.ExecuteAsync(null);
        Assert.True(model.IsBusy);
        Assert.False(model.SaveCommand.CanExecute(null));
        Assert.False(model.RefreshCommand.CanExecute(null));
        Assert.True(model.SearchCommand.CanExecute(null));

        await model.SaveCommand.ExecuteAsync(null);
        await model.RefreshCommand.ExecuteAsync(null);
        await model.SelectNoteCommand.ExecuteAsync(service.AllNotes[1]);
        Assert.Equal(0, service.Saves);
        Assert.Equal(2, service.NoteReads);
        Assert.Equal("one", model.SelectedNote?.Id);
        response.SetResult([]);
        await searching;

        Assert.False(model.IsBusy);
        Assert.True(model.SaveCommand.CanExecute(null));
        Assert.True(model.RefreshCommand.CanExecute(null));
        Assert.Equal("draft", model.Content);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CanceledSearchKeepsDraftAndEndsBusyState(bool fail)
    {
        var service = new SearchService();
        using var cancellation = new CancellationTokenSource();
        using var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync(cancellation.Token);
        model.Content = "draft";
        var response = Completion();
        service.Search = (_, _) => response.Task;
        model.SearchQuery = "slow";
        var searching = model.SearchCommand.ExecuteAsync(null);
        cancellation.Cancel();
        if (fail) response.SetException(new IOException("late error"));
        else response.SetResult([]);
        try { await searching; } catch (OperationCanceledException) { }

        Assert.False(model.IsBusy);
        Assert.Null(model.ErrorMessage);
        Assert.Equal("draft", model.Content);
        Assert.True(model.IsDirty);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task DisposedSearchDoesNotApplyLateResultsOrErrors(bool fail)
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        var response = Completion();
        service.Search = (_, _) => response.Task;
        model.SearchQuery = "slow";
        var searching = model.SearchCommand.ExecuteAsync(null);
        model.Content = "draft";
        model.Dispose();
        if (fail) response.SetException(new IOException("late error"));
        else response.SetResult([]);
        try { await searching; } catch (OperationCanceledException) { }
        Assert.False(model.IsBusy);
        Assert.Null(model.ErrorMessage);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("draft", model.Content);
        Assert.Equal("one", Assert.Single(model.Notes).Id);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task RefreshWithSearchKeepsEditorWhenCurrentNoteDoesNotMatch(bool dirty)
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        if (dirty) model.Content = "draft";
        model.SearchQuery = "no match";
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([]);

        await model.RefreshCommand.ExecuteAsync(null);

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal(dirty ? "draft" : "# One", model.Content);
        Assert.Equal(dirty, model.IsDirty);
    }

    [Fact]
    public async Task FullRefreshCannotDropDraftOfMissingNote()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        model.Content = "draft";
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([]);

        await model.RefreshCommand.ExecuteAsync(null);

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("draft", model.Content);
        Assert.True(model.IsDirty);
    }

    [Fact]
    public async Task CancelSearchMakesDraftSavableAndRejectsLateResult()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        var response = Completion();
        service.Search = (_, _) => response.Task;
        model.Content = "draft";
        model.SearchQuery = "slow";
        var searching = model.SearchCommand.ExecuteAsync(null);

        model.CancelSearchCommand.Execute(null);

        Assert.False(model.IsBusy);
        Assert.True(model.CanSave);
        await model.SaveCommand.ExecuteAsync(null);
        response.SetResult([]);
        await searching;
        Assert.False(model.IsDirty);
        Assert.Equal("draft", model.Content);
        Assert.Equal("one", Assert.Single(model.Notes).Id);
    }

    [Fact]
    public async Task SavingExcludedNoteDoesNotInsertItIntoSearchResults()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        model.SearchQuery = "match";
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([service.AllNotes[1]]);
        model.Content = "draft";
        await model.SearchCommand.ExecuteAsync(null);

        await model.SaveCommand.ExecuteAsync(null);

        Assert.Equal(1, service.Saves);
        Assert.False(model.IsDirty);
        Assert.Equal("draft", model.Content);
        Assert.Equal("two", Assert.Single(model.Notes).Id);
    }

    [Fact]
    public async Task UnfilteredRefreshStillClearsMissingCleanNote()
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        service.Search = (_, _) => Task.FromResult<IReadOnlyList<NotepadNote>>([]);
        await model.RefreshCommand.ExecuteAsync(null);
        Assert.Null(model.SelectedNote);
        Assert.Empty(model.Content);
        Assert.False(model.IsDirty);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldSearchCannotEndNewSearchWhileItIsPending(bool fail)
    {
        var service = new SearchService();
        using var model = await OpenAsync(service);
        var oldResponse = Completion();
        service.Search = (_, _) => oldResponse.Task;
        model.SearchQuery = "old";
        var oldSearch = model.SearchCommand.ExecuteAsync(null);
        var nextResponse = Completion();
        service.Search = (_, _) => nextResponse.Task;
        model.SearchQuery = "new";
        var nextSearch = model.SearchCommand.ExecuteAsync(null);
        if (fail) oldResponse.SetException(new IOException("old failure"));
        else oldResponse.SetResult([]);
        await oldSearch;
        Assert.True(model.IsSearching);
        Assert.True(model.IsBusy);
        Assert.Null(model.ErrorMessage);
        nextResponse.SetResult([service.AllNotes[1]]);
        await nextSearch;
        Assert.False(model.IsSearching);
        Assert.Equal("two", Assert.Single(model.Notes).Id);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task QueuedSearchCallbackChecksCancellationWhenActuallyApplied(bool fail)
    {
        var service = new SearchService();
        var dispatcher = new DeferredDispatcher();
        using var model = new NotepadViewModel(service, dispatcher);
        await model.OpenAsync();
        dispatcher.DeferNext = true;
        service.Search = (_, _) => fail
            ? Task.FromException<IReadOnlyList<NotepadNote>>(new IOException("queued failure"))
            : Task.FromResult<IReadOnlyList<NotepadNote>>([]);
        model.SearchQuery = "queued";
        var searching = model.SearchCommand.ExecuteAsync(null);
        await dispatcher.Queued.Task.WaitAsync(TimeSpan.FromSeconds(5));
        model.CancelSearchCommand.Execute(null);
        model.Content = "new draft";
        model.ErrorMessage = "new status";

        dispatcher.Release();
        await searching;

        Assert.Equal("new draft", model.Content);
        Assert.Equal("new status", model.ErrorMessage);
        Assert.Equal("one", Assert.Single(model.Notes).Id);
        Assert.False(model.IsBusy);
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

    private static TaskCompletionSource<IReadOnlyList<NotepadNote>> Completion() => new(TaskCreationOptions.RunContinuationsAsynchronously);
    private static async Task<NotepadViewModel> OpenAsync(SearchService service)
    {
        var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync();
        Assert.Equal("one", model.SelectedNote?.Id);
        return model;
    }

    private sealed class SearchService : INotepadService
    {
        public NotepadNote[] AllNotes { get; } =
        [
            new("one", "One", "", [], null, null, "one.md"),
            new("two", "Two", "work", [], null, null, "two.md"),
        ];
        public Func<string?, CancellationToken, Task<IReadOnlyList<NotepadNote>>>? Search { get; set; }
        public int NoteReads { get; private set; }
        public int FolderReads { get; private set; }
        public int DetailReads { get; private set; }
        public int Saves { get; private set; }
        public string? LastQuery { get; private set; }
        public Task InitializeAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task<IReadOnlyList<string>> ListFoldersAsync(CancellationToken cancellationToken = default) { FolderReads++; return Task.FromResult<IReadOnlyList<string>>(["work"]); }
        public Task<IReadOnlyList<NotepadNote>> ListNotesAsync(string? query, int limit = 500, CancellationToken cancellationToken = default)
        {
            NoteReads++;
            LastQuery = query;
            return Search?.Invoke(query, cancellationToken) ?? Task.FromResult<IReadOnlyList<NotepadNote>>(AllNotes);
        }
        public Task<NotepadNoteDetail> FetchNoteAsync(string id, CancellationToken cancellationToken = default) { DetailReads++; return Task.FromResult(new NotepadNoteDetail(AllNotes.Single(note => note.Id == id), "# One")); }
        public Task<NotepadNoteDetail> UpdateNoteAsync(string id, NotepadNoteUpdate update, CancellationToken cancellationToken = default) { Saves++; return Task.FromResult(new NotepadNoteDetail(AllNotes.Single(note => note.Id == id), update.Content!)); }
        public Task CreateFolderAsync(string folder, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task RenameFolderAsync(string from, string to, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DeleteFolderAsync(string folder, bool recursive, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<NotepadNoteDetail> CreateNoteAsync(NotepadNoteDraft draft, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DeleteNoteAsync(string id, CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }
}
