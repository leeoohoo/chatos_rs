using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Notepad;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class NotepadSavingTests
{
    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    [InlineData("\r\n\t")]
    [InlineData("\u3000")]
    public async Task BlankTitleKeepsAllDraftFieldsAndCanRetryWithoutSendingInvalidUpdate(string title)
    {
        var service = new SavingNotepadService();
        using var model = await OpenAsync(service);
        model.Title = title;
        model.TagsText = "draft, 中文";
        model.Content = "unsaved body\n  ";
        model.ActionMessage = "previous success";

        await model.SaveCommand.ExecuteAsync(null);

        Assert.Empty(service.Updates);
        Assert.Equal("笔记标题不能为空。", model.ErrorMessage);
        Assert.Null(model.ActionMessage);
        Assert.Equal(title, model.Title);
        Assert.Equal("draft, 中文", model.TagsText);
        Assert.Equal("unsaved body\n  ", model.Content);
        Assert.True(model.IsDirty);
        Assert.True(model.CanSave);
        Assert.True(model.CanClose);
        Assert.False(model.IsSaving);

        model.Title = "  Valid title  ";
        await model.SaveCommand.ExecuteAsync(null);
        Assert.Equal("Valid title", Assert.Single(service.Updates).Title);
        Assert.Null(model.ErrorMessage);
        Assert.False(model.IsDirty);
        Assert.Equal("unsaved body\n  ", model.Content);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task BlankTitleBlocksAutomaticSaveBeforeNavigationOrCreation(bool create)
    {
        var service = new SavingNotepadService();
        using var model = await OpenAsync(service);
        model.Title = " ";
        model.Content = "draft to retain";

        if (create) await model.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest("new"));
        else await model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);

        Assert.Empty(service.Updates);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("draft to retain", model.Content);
        Assert.Equal(" ", model.Title);
        Assert.True(model.IsDirty);
        Assert.Equal("笔记标题不能为空。", model.ErrorMessage);
    }

    [Theory]
    [InlineData("title")]
    [InlineData("tags")]
    [InlineData("body")]
    public async Task DirtyStateNotifiesTheUiOnEditFailedSaveAndSuccessfulRetry(string field)
    {
        var service = new SavingNotepadService { SaveError = new IOException("save unavailable") };
        using var model = await OpenAsync(service);
        var dirtyStates = new List<bool>();
        model.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName == nameof(model.IsDirty)) dirtyStates.Add(model.IsDirty);
        };

        switch (field)
        {
            case "title": model.Title = "Edited title"; break;
            case "tags": model.TagsText = "验收, windows"; break;
            default: model.Content = "中文正文\r\n\r\nTail spaces.  \r\n"; break;
        }
        Assert.True(dirtyStates.Last());
        await model.SaveCommand.ExecuteAsync(null);
        Assert.True(model.IsDirty);
        Assert.True(dirtyStates.Last());
        Assert.True(model.CanSave);

        service.SaveError = null;
        await model.SaveCommand.ExecuteAsync(null);
        Assert.False(model.IsDirty);
        Assert.False(dirtyStates.Last());
        Assert.False(model.CanSave);
        Assert.Equal(2, service.Updates.Count);
    }

    [Fact]
    public async Task MetadataOnlySaveKeepsBodyAndAppliesNormalizedMetadataWithoutRebuildingEditor()
    {
        var service = new SavingNotepadService();
        using var model = await OpenAsync(service);
        model.Title = "  Renamed  ";
        model.TagsText = "one，two, ONE";
        model.Content = "# Draft\r\n\r\nKeep exact whitespace.  \r\n";
        var contentChanges = 0;
        model.PropertyChanged += (_, args) => contentChanges += args.PropertyName == nameof(model.Content) ? 1 : 0;

        await model.SaveCommand.ExecuteAsync(null);

        Assert.Equal("# Draft\r\n\r\nKeep exact whitespace.  \r\n", model.Content);
        Assert.Equal(0, contentChanges);
        Assert.Equal("Renamed", model.Title);
        Assert.Equal("one, two", model.TagsText);
        Assert.Equal("Renamed", model.Notes.Single(note => note.Id == "one").Title);
        Assert.False(model.IsDirty);
        Assert.False(model.IsSaving);
    }

    [Theory]
    [InlineData("title")]
    [InlineData("tags")]
    [InlineData("body")]
    public async Task EditsMadeWhileSavingRemainDirtyAndCanBeSavedAgain(string field)
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var model = await OpenAsync(service);
        model.Title = "Submitted";
        model.TagsText = "submitted";
        model.Content = "submitted body";
        var saving = model.SaveCommand.ExecuteAsync(null);
        Assert.True(model.IsSaving);
        switch (field)
        {
            case "title": model.Title = "New title"; break;
            case "tags": model.TagsText = "new, tags"; break;
            default: model.Content = "new body"; break;
        }

        service.CompleteSave();
        await saving;

        Assert.Equal(field == "title" ? "New title" : "Submitted", model.Title);
        Assert.Equal(field == "tags" ? "new, tags" : "submitted", model.TagsText);
        Assert.Equal(field == "body" ? "new body" : "submitted body", model.Content);
        Assert.True(model.IsDirty);
        Assert.Equal("submitted body", service.Updates[0].Content);
        Assert.Equal("Submitted", service.Updates[0].Title);
        Assert.Equal(["submitted"], service.Updates[0].Tags);

        service.PendingSave = null;
        await model.SaveCommand.ExecuteAsync(null);
        Assert.Equal(2, service.Updates.Count);
        Assert.Equal(model.Content, service.Updates[1].Content);
        Assert.Equal(model.Title, service.Updates[1].Title);
        Assert.False(model.IsDirty);
    }

    [Theory]
    [InlineData("title")]
    [InlineData("tags")]
    [InlineData("body")]
    public async Task EditingAnyFieldDuringSaveDefersAllMetadataNormalization(string field)
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var model = await OpenAsync(service);
        model.Title = "  Submitted  ";
        model.TagsText = "one，two, ONE";
        model.Content = "submitted body";
        var saving = model.SaveCommand.ExecuteAsync(null);
        switch (field)
        {
            case "title": model.Title = "  New title  "; break;
            case "tags": model.TagsText = "new，tags"; break;
            default: model.Content = "new body"; break;
        }

        service.CompleteSave();
        await saving;

        Assert.Equal(field == "title" ? "  New title  " : "  Submitted  ", model.Title);
        Assert.Equal(field == "tags" ? "new，tags" : "one，two, ONE", model.TagsText);
        Assert.Equal(field == "body" ? "new body" : "submitted body", model.Content);
        Assert.True(model.IsDirty);
        Assert.True(model.CanSave);
        Assert.Equal("先前的修改已保存；当前仍有未保存的修改。", model.ActionMessage);
        Assert.Equal("Submitted", Assert.Single(service.Updates).Title);
        Assert.Equal(["one", "two"], service.Updates[0].Tags);

        service.PendingSave = null;
        await model.SaveCommand.ExecuteAsync(null);
        Assert.Equal(field == "title" ? "New title" : "Submitted", model.Title);
        Assert.Equal(field == "tags" ? "new, tags" : "one, two", model.TagsText);
        Assert.Equal(field == "body" ? "new body" : "submitted body", model.Content);
        Assert.False(model.IsDirty);
        Assert.False(model.CanSave);
    }

    [Fact]
    public async Task SaveFailurePreservesBufferAndAllowsRetry()
    {
        var service = new SavingNotepadService { SaveError = new IOException("disk unavailable") };
        using var model = await OpenAsync(service);
        model.Content = "unsaved";

        await model.SaveCommand.ExecuteAsync(null);

        Assert.True(model.IsDirty);
        Assert.Equal("unsaved", model.Content);
        Assert.Equal("disk unavailable", model.ErrorMessage);
        Assert.Null(model.ActionMessage);
        Assert.False(model.IsSaving);
        service.SaveError = null;
        await model.SaveCommand.ExecuteAsync(null);
        Assert.Null(model.ErrorMessage);
        Assert.False(model.IsDirty);
        Assert.Equal("unsaved", model.Content);
    }

    [Fact]
    public async Task SelectionDoesNotStartAnotherSaveWhileManualSaveIsInFlight()
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var model = await OpenAsync(service);
        model.Content = "draft";
        var saving = model.SaveCommand.ExecuteAsync(null);

        var selecting = model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        var duplicate = model.SaveCommand.ExecuteAsync(null);
        var updatesBeforeCompletion = service.Updates.Count;
        service.CompleteSave();
        await Task.WhenAll(saving, selecting, duplicate);

        Assert.Equal(1, updatesBeforeCompletion);
        Assert.Single(service.Updates);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("draft", model.Content);
    }

    [Fact]
    public async Task AutomaticSaveDoesNotNavigateAwayFromNewlyTypedEdits()
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var model = await OpenAsync(service);
        model.Content = "submitted";
        var selecting = model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);
        model.Content = "typed while saving";

        service.CompleteSave();
        await selecting;

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("typed while saving", model.Content);
        Assert.True(model.IsDirty);
    }

    [Fact]
    public async Task SuccessfulAutomaticSaveStillOpensRequestedNote()
    {
        var service = new SavingNotepadService();
        using var model = await OpenAsync(service);
        model.Content = "submitted";

        await model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);

        Assert.Equal("submitted", Assert.Single(service.Updates).Content);
        Assert.Equal("two", model.SelectedNote?.Id);
        Assert.Equal("# Two", model.Content);
        Assert.False(model.IsDirty);
    }

    [Fact]
    public async Task FailedAutomaticSaveKeepsCurrentNoteAndDraft()
    {
        var service = new SavingNotepadService { SaveError = new IOException("conflict") };
        using var model = await OpenAsync(service);
        model.Content = "draft";

        await model.SelectNoteCommand.ExecuteAsync(service.Notes[1]);

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.Equal("conflict", model.ErrorMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task LateSaveResultAfterCloseCannotChangeReopenedSession(bool fail)
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var model = await OpenAsync(service);
        model.Content = "old submitted";
        var saving = model.SaveCommand.ExecuteAsync(null);
        await model.CloseAsync();
        await model.OpenAsync();
        model.Content = "new session draft";
        model.ErrorMessage = "new session error";
        model.ActionMessage = "new session action";

        if (fail) service.PendingSave.SetException(new IOException("old error"));
        else service.CompleteSave();
        try { await saving; }
        catch (OperationCanceledException) { }

        Assert.Equal("new session draft", model.Content);
        Assert.Equal("new session error", model.ErrorMessage);
        Assert.Equal("new session action", model.ActionMessage);
        Assert.True(model.IsDirty);
        Assert.False(model.IsSaving);
    }

    [Fact]
    public async Task RefreshAndSearchCannotResetEditorDuringSave()
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var model = await OpenAsync(service);
        model.Content = "draft";
        var saving = model.SaveCommand.ExecuteAsync(null);
        service.ListedNotes = [];

        await model.RefreshCommand.ExecuteAsync(null);
        await model.SearchCommand.ExecuteAsync(null);
        var countWhileSaving = service.ListCount;
        service.CompleteSave();
        await saving;

        Assert.Equal(1, countWhileSaving);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("draft", model.Content);
    }

    [Fact]
    public async Task SaveDoesNotMoveBackFromFolderChosenWhileSaving()
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var model = await OpenAsync(service);
        model.Content = "draft";
        var saving = model.SaveCommand.ExecuteAsync(null);
        model.SelectFolderCommand.Execute(new NotepadFolderItem("work"));

        service.CompleteSave();
        await saving;

        Assert.Equal("work", model.SelectedFolder);
        Assert.All(model.Notes, note => Assert.Equal("work", note.Folder));
        Assert.Equal("draft", model.Content);
    }

    [Fact]
    public async Task SaveActionsShowBusyStateAndAllowRetryForNewEdits()
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var model = await OpenAsync(service);
        Assert.False(model.CanSave);
        Assert.False(model.SaveCommand.CanExecute(null));
        model.Content = "submitted";
        Assert.True(model.CanSave);
        Assert.True(model.SaveCommand.CanExecute(null));
        var saving = model.SaveCommand.ExecuteAsync(null);

        Assert.True(model.IsBusy);
        Assert.False(model.CanSave);
        Assert.False(model.CanClose);
        Assert.False(model.RefreshCommand.CanExecute(null));
        Assert.False(model.SearchCommand.CanExecute(null));
        model.Content = "newer edits";
        service.CompleteSave();
        await saving;

        Assert.False(model.IsBusy);
        Assert.True(model.CanSave);
        Assert.True(model.CanClose);
        Assert.True(model.SaveCommand.CanExecute(null));
        Assert.True(model.RefreshCommand.CanExecute(null));
        Assert.True(model.SearchCommand.CanExecute(null));
        Assert.Contains("未保存", model.ActionMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldSaveCompletionCannotClearNewSessionsSavingState(bool fail)
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var model = await OpenAsync(service);
        model.Content = "old draft";
        var oldSave = model.SaveCommand.ExecuteAsync(null);
        var oldCompletion = service.PendingSave;
        await model.CloseAsync();
        await model.OpenAsync();
        model.Content = "new draft";
        service.PendingSave = NewCompletion();
        var newSave = model.SaveCommand.ExecuteAsync(null);

        if (fail) oldCompletion.SetException(new IOException("old error"));
        else oldCompletion.SetResult(new NotepadNoteDetail(service.Notes[0], "old response"));
        await oldSave;

        Assert.True(model.IsSaving);
        Assert.True(model.IsBusy);
        Assert.Equal("new draft", model.Content);
        Assert.Null(model.ErrorMessage);
        service.CompleteSave();
        await newSave;
        Assert.False(model.IsSaving);
        Assert.False(model.IsDirty);
        Assert.Equal("new draft", model.Content);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task DisposalRejectsLateSuccessAndFailure(bool fail)
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var model = await OpenAsync(service);
        model.Content = "draft";
        var saving = model.SaveCommand.ExecuteAsync(null);
        model.Dispose();
        if (fail) service.PendingSave.SetException(new IOException("late error"));
        else service.CompleteSave();
        await saving;

        Assert.False(model.IsOpen);
        Assert.False(model.IsSaving);
        Assert.True(model.IsDirty);
        Assert.Equal("draft", model.Content);
        Assert.Null(model.ErrorMessage);
        Assert.Null(model.ActionMessage);
    }

    [Fact]
    public async Task RepeatedOpenCannotInterruptActiveSave()
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var model = await OpenAsync(service);
        model.Content = "draft";
        var saving = model.SaveCommand.ExecuteAsync(null);
        await model.OpenAsync();
        Assert.True(model.IsSaving);
        Assert.Equal(1, service.ListCount);

        service.CompleteSave();
        await saving;

        Assert.False(model.IsSaving);
        Assert.False(model.IsDirty);
        Assert.Equal("draft", model.Content);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CanceledSessionEndsSavingWithoutApplyingLateResponseAndCanReopen(bool fail)
    {
        var service = new SavingNotepadService { PendingSave = NewCompletion() };
        using var cancellation = new CancellationTokenSource();
        using var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync(cancellation.Token);
        model.Content = "draft";
        var saving = model.SaveCommand.ExecuteAsync(null);
        cancellation.Cancel();
        if (fail) service.PendingSave.SetException(new IOException("canceled error"));
        else service.CompleteSave();
        await saving;

        Assert.False(model.IsSaving);
        Assert.False(model.IsBusy);
        Assert.False(model.CanSave);
        Assert.True(model.CanClose);
        Assert.True(model.IsDirty);
        Assert.Equal("draft", model.Content);
        Assert.Null(model.ErrorMessage);
        Assert.Null(model.ActionMessage);
        await model.OpenAsync();
        service.PendingSave = null;
        await model.SaveCommand.ExecuteAsync(null);
        Assert.False(model.IsDirty);
        Assert.Equal("draft", model.Content);
    }

    private static TaskCompletionSource<NotepadNoteDetail> NewCompletion() => new(TaskCreationOptions.RunContinuationsAsynchronously);

    private static async Task<NotepadViewModel> OpenAsync(SavingNotepadService service)
    {
        var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync();
        Assert.Equal("one", model.SelectedNote?.Id);
        return model;
    }

    private sealed class SavingNotepadService : INotepadService
    {
        public NotepadNote[] Notes { get; } =
        [
            new("one", "One", "", [], null, null, "one.md"),
            new("two", "Two", "work", [], null, null, "two.md"),
        ];
        public IReadOnlyList<NotepadNote>? ListedNotes { get; set; }
        public List<NotepadNoteUpdate> Updates { get; } = [];
        public TaskCompletionSource<NotepadNoteDetail>? PendingSave { get; set; }
        public Exception? SaveError { get; set; }
        public int ListCount { get; private set; }

        public void CompleteSave() => PendingSave!.SetResult(SavedDetail());
        private NotepadNoteDetail SavedDetail() => new(Notes[0] with { Title = Updates[^1].Title!, Tags = Updates[^1].Tags! }, string.Empty);
        public Task InitializeAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task<IReadOnlyList<string>> ListFoldersAsync(CancellationToken cancellationToken = default) => Task.FromResult<IReadOnlyList<string>>(["work"]);
        public Task<IReadOnlyList<NotepadNote>> ListNotesAsync(string? query, int limit = 500, CancellationToken cancellationToken = default)
        {
            ListCount++;
            return Task.FromResult(ListedNotes ?? Notes);
        }
        public Task<NotepadNoteDetail> FetchNoteAsync(string id, CancellationToken cancellationToken = default) => Task.FromResult(new NotepadNoteDetail(Notes.Single(note => note.Id == id), id == "one" ? "# One" : "# Two"));
        public Task<NotepadNoteDetail> UpdateNoteAsync(string id, NotepadNoteUpdate update, CancellationToken cancellationToken = default)
        {
            Updates.Add(update);
            if (SaveError is not null) return Task.FromException<NotepadNoteDetail>(SaveError);
            return PendingSave?.Task ?? Task.FromResult(SavedDetail());
        }
        public Task CreateFolderAsync(string folder, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task RenameFolderAsync(string from, string to, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DeleteFolderAsync(string folder, bool recursive, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<NotepadNoteDetail> CreateNoteAsync(NotepadNoteDraft draft, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DeleteNoteAsync(string id, CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }
}
