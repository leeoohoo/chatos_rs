using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Notepad;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class NotepadDiscardTests
{
    [Theory]
    [InlineData("title")]
    [InlineData("tags")]
    [InlineData("body")]
    public async Task DiscardRestoresLoadedSnapshotWithoutWriting(string field)
    {
        var service = new DiscardService();
        using var model = await OpenAsync(service);
        switch (field)
        {
            case "title": model.Title = "draft"; break;
            case "tags": model.TagsText = "draft"; break;
            default: model.Content = "draft"; break;
        }
        Assert.True(model.CanDiscardChanges);
        Assert.True(model.DiscardChangesCommand.CanExecute(null));

        model.DiscardChangesCommand.Execute(null);

        Assert.Equal("Original", model.Title);
        Assert.Equal("one, two", model.TagsText);
        Assert.Equal("# Original\r\nbody  \r\n", model.Content);
        Assert.False(model.IsDirty);
        Assert.False(model.CanSave);
        Assert.False(model.DiscardChangesCommand.CanExecute(null));
        Assert.Equal("note", model.SelectedNote?.Id);
        Assert.Equal(0, service.Saves);
        Assert.Equal(1, service.DetailReads);
    }

    [Fact]
    public async Task ExplicitDiscardDoesNotReturnAfterClosingAndReopening()
    {
        var service = new DiscardService();
        using var model = await OpenAsync(service);
        model.Title = "discard title";
        model.TagsText = "discard tags";
        model.Content = "discard body";
        model.ErrorMessage = "old error";

        model.DiscardChangesCommand.Execute(null);
        await model.CloseAsync();
        await model.OpenAsync();

        Assert.Equal("Original", model.Title);
        Assert.Equal("one, two", model.TagsText);
        Assert.Equal("# Original\r\nbody  \r\n", model.Content);
        Assert.False(model.IsDirty);
        Assert.Null(model.ErrorMessage);
        Assert.Equal(0, service.Saves);
    }

    [Fact]
    public async Task DiscardUsesMostRecentSuccessfulSaveIncludingMetadataOnlyResponse()
    {
        var service = new DiscardService();
        using var model = await OpenAsync(service);
        model.Title = "  Saved  ";
        model.TagsText = "saved, tag";
        model.Content = "saved body";
        await model.SaveCommand.ExecuteAsync(null);
        model.Title = "later title";
        model.TagsText = "later tags";
        model.Content = "later body";

        model.DiscardChangesCommand.Execute(null);

        Assert.Equal("Saved", model.Title);
        Assert.Equal("saved, tag", model.TagsText);
        Assert.Equal("saved body", model.Content);
        Assert.False(model.IsDirty);
        Assert.Equal(1, service.Saves);
    }

    [Fact]
    public async Task FailedSaveDoesNotBecomeDiscardBaseline()
    {
        var service = new DiscardService { SaveError = new IOException("save failed") };
        using var model = await OpenAsync(service);
        model.Content = "failed body";
        await model.SaveCommand.ExecuteAsync(null);
        Assert.True(model.IsDirty);

        model.DiscardChangesCommand.Execute(null);

        Assert.Equal("# Original\r\nbody  \r\n", model.Content);
        Assert.False(model.IsDirty);
        Assert.Null(model.ErrorMessage);
        Assert.Equal(1, service.Saves);
    }

    [Fact]
    public async Task DiscardCannotUndoOrInterfereWithAnActiveSave()
    {
        var service = new DiscardService { SaveCompletion = new(TaskCreationOptions.RunContinuationsAsynchronously) };
        using var model = await OpenAsync(service);
        model.Content = "submitted";
        var saving = model.SaveCommand.ExecuteAsync(null);
        model.Content = "newer unsaved";

        Assert.False(model.CanDiscardChanges);
        model.DiscardChangesCommand.Execute(null);
        Assert.Equal("newer unsaved", model.Content);
        service.SaveCompletion.SetResult(new NotepadNoteDetail(service.Note, string.Empty));
        await saving;
        Assert.True(model.CanDiscardChanges);
        model.DiscardChangesCommand.Execute(null);
        Assert.Equal("submitted", model.Content);
        Assert.False(model.IsDirty);
        Assert.Equal(1, service.Saves);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task DiscardCancelsSearchAndIgnoresItsLateResult(bool fail)
    {
        var service = new DiscardService();
        using var model = await OpenAsync(service);
        var response = new TaskCompletionSource<IReadOnlyList<NotepadNote>>(TaskCreationOptions.RunContinuationsAsynchronously);
        service.SearchCompletion = response;
        model.SearchQuery = "slow";
        var searching = model.SearchCommand.ExecuteAsync(null);
        model.Content = "discard";

        model.DiscardChangesCommand.Execute(null);

        Assert.False(model.IsSearching);
        Assert.True(service.SearchToken.IsCancellationRequested);
        if (fail) response.SetException(new IOException("late failure"));
        else response.SetResult([]);
        await searching;
        Assert.False(model.IsDirty);
        Assert.Equal("# Original\r\nbody  \r\n", model.Content);
        Assert.Null(model.ErrorMessage);
        Assert.Single(model.Notes);
        Assert.Equal(0, service.Saves);
    }

    [Fact]
    public async Task LifecycleCloseWithoutExplicitDiscardStillKeepsDraft()
    {
        var service = new DiscardService();
        using var model = await OpenAsync(service);
        model.Content = "keep draft";
        await model.CloseAsync();
        model.DiscardChangesCommand.Execute(null);
        await model.OpenAsync();
        Assert.Equal("keep draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.Equal(0, service.Saves);
    }

    [Fact]
    public async Task RepeatedDiscardOnCleanNoteDoesNotChangeStatusOrWrite()
    {
        var service = new DiscardService();
        using var model = await OpenAsync(service);
        model.ErrorMessage = "current status";
        model.DiscardChangesCommand.Execute(null);
        Assert.Equal("current status", model.ErrorMessage);
        Assert.Null(model.ActionMessage);
        Assert.Equal(0, service.Saves);
    }

    private static async Task<NotepadViewModel> OpenAsync(DiscardService service)
    {
        var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync();
        return model;
    }

    private sealed class DiscardService : INotepadService
    {
        public NotepadNote Note { get; private set; } = new("note", "Original", "", ["one", "two"], null, null, "note.md");
        public int Saves { get; private set; }
        public int DetailReads { get; private set; }
        public Exception? SaveError { get; set; }
        public TaskCompletionSource<NotepadNoteDetail>? SaveCompletion { get; set; }
        public TaskCompletionSource<IReadOnlyList<NotepadNote>>? SearchCompletion { get; set; }
        public CancellationToken SearchToken { get; private set; }
        public Task InitializeAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task<IReadOnlyList<string>> ListFoldersAsync(CancellationToken cancellationToken = default) => Task.FromResult<IReadOnlyList<string>>([]);
        public Task<IReadOnlyList<NotepadNote>> ListNotesAsync(string? query, int limit = 500, CancellationToken cancellationToken = default)
        {
            SearchToken = cancellationToken;
            return SearchCompletion?.Task ?? Task.FromResult<IReadOnlyList<NotepadNote>>([Note]);
        }
        public Task<NotepadNoteDetail> FetchNoteAsync(string id, CancellationToken cancellationToken = default) { DetailReads++; return Task.FromResult(new NotepadNoteDetail(Note, "# Original\r\nbody  \r\n")); }
        public Task<NotepadNoteDetail> UpdateNoteAsync(string id, NotepadNoteUpdate update, CancellationToken cancellationToken = default)
        {
            Saves++;
            if (SaveError is not null) return Task.FromException<NotepadNoteDetail>(SaveError);
            Note = Note with { Title = update.Title!, Tags = update.Tags! };
            return SaveCompletion?.Task ?? Task.FromResult(new NotepadNoteDetail(Note, string.Empty));
        }
        public Task CreateFolderAsync(string folder, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task RenameFolderAsync(string from, string to, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DeleteFolderAsync(string folder, bool recursive, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<NotepadNoteDetail> CreateNoteAsync(NotepadNoteDraft draft, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DeleteNoteAsync(string id, CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }
}
