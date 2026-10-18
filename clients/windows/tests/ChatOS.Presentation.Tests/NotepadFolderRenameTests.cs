using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Notepad;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class NotepadFolderRenameTests
{
    [Fact]
    public async Task FolderNavigationNotifiesSelectionAndRootWithoutChangingEditorDraft()
    {
        using var model = await OpenAsync(new RenameService());
        model.Content = "keep editor draft";
        var changes = new List<string?>();
        model.PropertyChanged += (_, args) => changes.Add(args.PropertyName);
        model.SelectFolderCommand.Execute(new NotepadFolderItem("work"));
        Assert.Same(model.Folders.Single(folder => folder.Path == "work"), model.SelectedFolderItem);
        Assert.Contains(nameof(model.SelectedFolderItem), changes);
        Assert.False(model.IsRootFolderSelected);
        changes.Clear();
        model.SelectFolderCommand.Execute(null);
        Assert.Null(model.SelectedFolderItem);
        Assert.True(model.IsRootFolderSelected);
        Assert.Contains(nameof(model.IsRootFolderSelected), changes);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("keep editor draft", model.Content);
        Assert.True(model.IsDirty);
    }

    [Fact]
    public async Task FolderRefreshPublishesCurrentCollectionInstanceEvenWhenPathIsUnchanged()
    {
        using var model = await OpenAsync(new RenameService());
        var previous = model.SelectedFolderItem;
        var changes = new List<NotepadFolderItem?>();
        model.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName == nameof(model.SelectedFolderItem)) changes.Add(model.SelectedFolderItem);
        };
        await model.RefreshCommand.ExecuteAsync(null);
        Assert.NotSame(previous, model.SelectedFolderItem);
        Assert.Same(model.Folders.Single(folder => folder.Path == "work/child"), model.SelectedFolderItem);
        Assert.Contains(changes, folder => ReferenceEquals(folder, model.SelectedFolderItem));
    }

    [Fact]
    public async Task FolderRefreshPublishesSelectionAfterCollectionConsumersHaveProcessedReplacement()
    {
        using var model = await OpenAsync(new RenameService());
        var visibleItems = model.Folders.ToList();
        NotepadFolderItem? highlighted = null;
        model.Folders.CollectionChanged += (_, _) => visibleItems = model.Folders.ToList();
        model.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName == nameof(model.SelectedFolderItem))
                highlighted = visibleItems.FirstOrDefault(folder => ReferenceEquals(folder, model.SelectedFolderItem));
        };
        // Put the selected item last so no later Add can accidentally repair an
        // early notification before the native list has received that item.
        model.SelectFolderCommand.Execute(new NotepadFolderItem("workshop"));
        await model.RefreshCommand.ExecuteAsync(null);
        Assert.NotNull(highlighted);
        Assert.Same(model.SelectedFolderItem, highlighted);
    }

    [Fact]
    public async Task RenamePublishesMappedSelectionAndSelectingAnotherNoteMovesFolderHighlight()
    {
        var service = new RenameService();
        using var model = await OpenAsync(service);
        var changes = new List<NotepadFolderItem?>();
        model.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName == nameof(model.SelectedFolderItem)) changes.Add(model.SelectedFolderItem);
        };
        await RenameAsync(model);
        Assert.Equal("archive/child", model.SelectedFolderItem?.Path);
        Assert.Contains(changes, folder => ReferenceEquals(folder, model.SelectedFolderItem));
        await model.SelectNoteCommand.ExecuteAsync(service.Notes.Single(note => note.Id == "similar"));
        Assert.Equal("workshop", model.SelectedFolderItem?.Path);
        Assert.False(model.IsRootFolderSelected);
        await model.SelectNoteCommand.ExecuteAsync(service.Notes.Single(note => note.Id == "other"));
        Assert.Null(model.SelectedFolderItem);
        Assert.True(model.IsRootFolderSelected);
    }

    [Fact]
    public async Task InvalidRenameReplacesPreviousSuccessAndValidRetryClearsError()
    {
        var service = new RenameService();
        using var model = await OpenAsync(service);
        await RenameAsync(model);
        Assert.NotNull(model.ActionMessage);
        model.Content = "keep draft through validation";
        await model.RenameFolderCommand.ExecuteAsync(new NotepadFolderRenameRequest("archive", "archive/child"));
        Assert.Null(model.ActionMessage);
        Assert.NotNull(model.ErrorMessage);
        Assert.Single(service.Renames);
        Assert.Equal("archive/child", model.SelectedFolder);
        await model.RenameFolderCommand.ExecuteAsync(new NotepadFolderRenameRequest("archive", "final"));
        Assert.Null(model.ErrorMessage);
        Assert.NotNull(model.ActionMessage);
        Assert.Equal("final/child", model.SelectedFolder);
        Assert.Equal("keep draft through validation", model.Content);
        Assert.True(model.IsDirty);
        Assert.Empty(service.Updates);
    }

    [Theory]
    [InlineData("title")]
    [InlineData("tags")]
    [InlineData("body")]
    public async Task RenamePreservesDraftAndInputDuringRequestAndKeepsChildSelection(string field)
    {
        var service = new RenameService { PendingRename = Completion() };
        using var model = await OpenAsync(service);
        model.Content = "existing draft";
        var renaming = RenameAsync(model);
        Assert.True(model.IsLoading);
        if (field == "title") model.Title = "new title";
        else if (field == "tags") model.TagsText = "new, tags";
        else model.Content = "new body";
        service.PendingRename.SetResult();
        await renaming;

        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("archive/child", model.SelectedNote?.Folder);
        Assert.Equal("archive/child", model.SelectedFolder);
        Assert.Equal(field == "title" ? "new title" : "One", model.Title);
        Assert.Equal(field == "tags" ? "new, tags" : "", model.TagsText);
        Assert.Equal(field == "body" ? "new body" : "existing draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.Empty(service.Updates);
        Assert.False(model.IsBusy);
        await model.SaveCommand.ExecuteAsync(null);
        Assert.Equal(model.Content, Assert.Single(service.Updates).Content);
        Assert.Equal("archive/child", model.SelectedNote?.Folder);
    }

    [Fact]
    public async Task UnrelatedEditorAndNavigationAreNotMovedToRenamedFolder()
    {
        var service = new RenameService();
        using var model = await OpenAsync(service);
        await model.SelectNoteCommand.ExecuteAsync(service.Notes.Single(note => note.Id == "similar"));
        model.Content = "unrelated draft";
        await RenameAsync(model);

        Assert.Equal("similar", model.SelectedNote?.Id);
        Assert.Equal("workshop", model.SelectedNote?.Folder);
        Assert.Equal("workshop", model.SelectedFolder);
        Assert.Equal("unrelated draft", model.Content);
        Assert.Equal("similar", Assert.Single(model.Notes).Id);
    }

    [Theory]
    [InlineData("work/child", "archive/child")]
    [InlineData("workshop", "workshop")]
    [InlineData("", "")]
    public async Task NavigationChangedDuringRenameIsMappedOnlyWhenAffected(string navigation, string expected)
    {
        var service = new RenameService { PendingRename = Completion() };
        using var model = await OpenAsync(service);
        var renaming = RenameAsync(model);
        model.SelectFolderCommand.Execute(new NotepadFolderItem(navigation));
        service.PendingRename.SetResult();
        await renaming;
        Assert.Equal(expected, model.SelectedFolder);
        Assert.Equal("archive/child", model.SelectedNote?.Folder);
    }

    [Fact]
    public async Task SuccessfulRenameUpdatesPathsBeforeRefreshAndRefreshCannotEraseNewInput()
    {
        var service = new RenameService();
        using var model = await OpenAsync(service);
        var folders = Completion<IReadOnlyList<string>>();
        service.Folders = _ => folders.Task;
        var renaming = RenameAsync(model);
        var folderBeforeRefresh = model.SelectedFolder;
        var noteFolderBeforeRefresh = model.SelectedNote?.Folder;
        var localFolders = model.Folders.Select(item => item.Path).ToArray();
        var actionBeforeRefresh = model.ActionMessage;
        model.Title = "typed during refresh";
        folders.SetResult(["archive", "archive/child", "workshop"]);
        await renaming;

        Assert.Equal("archive/child", folderBeforeRefresh);
        Assert.Equal("archive/child", noteFolderBeforeRefresh);
        Assert.Contains("archive/child", localFolders);
        Assert.DoesNotContain("work", localFolders);
        Assert.NotNull(actionBeforeRefresh);
        Assert.Equal("typed during refresh", model.Title);
        Assert.True(model.IsDirty);
        Assert.Equal("one.md", model.SelectedNote?.File);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task RefreshFailureKeepsConfirmedRenameAndPlainRefreshDoesNotRenameAgain(bool failNotes)
    {
        var service = new RenameService();
        using var model = await OpenAsync(service);
        model.Content = "draft";
        if (failNotes) service.ReadNotes = (_, _) => throw new IOException("note list failed");
        else service.Folders = _ => throw new IOException("folder list failed");
        await RenameAsync(model);

        Assert.Equal("archive/child", model.SelectedFolder);
        Assert.Equal("archive/child", model.SelectedNote?.Folder);
        Assert.Contains(model.Folders, item => item.Path == "archive/child");
        Assert.DoesNotContain(model.Folders, item => item.Path == "work");
        Assert.Equal("draft", model.Content);
        Assert.True(model.IsDirty);
        Assert.Contains("重命名已完成", model.ErrorMessage);
        Assert.Contains("无需重复重命名", model.ErrorMessage);
        Assert.NotNull(model.ActionMessage);
        service.Folders = null;
        service.ReadNotes = null;
        await model.RefreshCommand.ExecuteAsync(null);
        Assert.Single(service.Renames);
        Assert.Null(model.ErrorMessage);
        Assert.Equal("draft", model.Content);
    }

    [Fact]
    public async Task MutationFailureKeepsPathsAndDraftAndAllowsRetry()
    {
        var service = new RenameService { RenameError = new IOException("target already exists") };
        using var model = await OpenAsync(service);
        model.Content = "draft";
        await RenameAsync(model);
        Assert.Equal("target already exists", model.ErrorMessage);
        Assert.Null(model.ActionMessage);
        Assert.Equal("work/child", model.SelectedFolder);
        Assert.Equal("work/child", model.SelectedNote?.Folder);
        Assert.Equal(1, service.NoteReads);
        Assert.Equal(1, service.FolderReads);
        Assert.Equal("draft", model.Content);
        service.RenameError = null;
        await RenameAsync(model);
        Assert.Null(model.ErrorMessage);
        Assert.Equal("archive/child", model.SelectedFolder);
    }

    [Fact]
    public async Task SearchRefreshUsesAppliedQueryAndKeepsEditorWhenRenamedNoteStopsMatching()
    {
        var service = new RenameService();
        using var model = await OpenAsync(service);
        model.SearchQuery = "work";
        await model.SearchCommand.ExecuteAsync(null);
        model.SearchQuery = "unsubmitted query";
        model.Content = "draft outside next results";
        await RenameAsync(model);

        Assert.Equal("work", service.Queries.Last());
        Assert.Equal("unsubmitted query", model.SearchQuery);
        Assert.True(model.IsSearchActive);
        Assert.Equal("similar", Assert.Single(model.Notes).Id);
        Assert.Equal("one", model.SelectedNote?.Id);
        Assert.Equal("archive/child", model.SelectedNote?.Folder);
        Assert.Equal("draft outside next results", model.Content);
    }

    [Theory]
    [InlineData("", "archive")]
    [InlineData(" / ", "archive")]
    [InlineData("work", "")]
    [InlineData("work", "work/inside")]
    [InlineData("work", "work")]
    public async Task InvalidRootDescendantAndUnchangedPathsDoNotSendMutation(string from, string to)
    {
        var service = new RenameService();
        using var model = await OpenAsync(service);
        await model.RenameFolderCommand.ExecuteAsync(new NotepadFolderRenameRequest(from, to));
        Assert.Empty(service.Renames);
        Assert.Equal("work/child", model.SelectedFolder);
    }

    [Fact]
    public async Task NormalizedPathsAndNewParentFoldersAreAvailableBeforeRefreshFailure()
    {
        var service = new RenameService();
        using var model = await OpenAsync(service);
        service.Folders = _ => throw new IOException("offline");
        await model.RenameFolderCommand.ExecuteAsync(new NotepadFolderRenameRequest(" /work\\ ", " /archive\\notes/ "));
        Assert.Equal(new NotepadFolderRenameRequest("work", "archive/notes"), Assert.Single(service.Renames));
        Assert.Equal("archive/notes/child", model.SelectedFolder);
        Assert.Contains(model.Folders, item => item.Path == "archive");
        Assert.Contains(model.Folders, item => item.Path == "archive/notes");
        Assert.Contains(model.Folders, item => item.Path == "archive/notes/child");
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ClosedMutationCannotChangeReopenedDraftPathsOrLoading(bool fail)
    {
        var service = new RenameService { PendingRename = Completion() };
        using var model = await OpenAsync(service);
        var renaming = RenameAsync(model);
        await model.CloseAsync();
        var folders = Completion<IReadOnlyList<string>>();
        service.Folders = _ => folders.Task;
        var opening = model.OpenAsync();
        model.Content = "new draft";
        model.ErrorMessage = "new error";
        if (fail) service.PendingRename.SetException(new IOException("old error"));
        else service.PendingRename.SetResult();
        await renaming;
        var stillLoading = model.IsLoading;
        var currentFolder = model.SelectedFolder;
        var currentError = model.ErrorMessage;
        var folderReads = service.FolderReads;
        folders.SetResult(["current"]);
        await opening;

        Assert.True(stillLoading);
        Assert.Equal("work/child", currentFolder);
        Assert.Equal("new error", currentError);
        Assert.Equal("new draft", model.Content);
        Assert.Equal(2, folderReads);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldRefreshCannotReplaceNewSessionsResourcesOrEndLoading(bool fail)
    {
        var service = new RenameService();
        using var model = await OpenAsync(service);
        var oldFolders = Completion<IReadOnlyList<string>>();
        service.Folders = _ => oldFolders.Task;
        var renaming = RenameAsync(model);
        await model.CloseAsync();
        var newFolders = Completion<IReadOnlyList<string>>();
        service.Folders = _ => newFolders.Task;
        var opening = model.OpenAsync();
        model.ErrorMessage = "new error";
        if (fail) oldFolders.SetException(new IOException("old refresh failed"));
        else oldFolders.SetResult(["stale"]);
        await renaming;
        var busy = model.IsLoading;
        var paths = model.Folders.Select(item => item.Path).ToArray();
        var error = model.ErrorMessage;
        newFolders.SetResult(["current"]);
        await opening;
        Assert.True(busy);
        Assert.DoesNotContain("stale", paths);
        Assert.Equal("new error", error);
        Assert.Equal("current", Assert.Single(model.Folders).Path);
    }

    [Fact]
    public async Task CancellationKeepsDraftAndDoesNotReportFailureOrReload()
    {
        using var cancellation = new CancellationTokenSource();
        var service = new RenameService { PendingRename = Completion() };
        using var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync(cancellation.Token);
        model.Content = "draft";
        var renaming = RenameAsync(model);
        cancellation.Cancel();
        service.PendingRename.SetCanceled(cancellation.Token);
        await renaming;
        Assert.False(model.IsBusy);
        Assert.Null(model.ErrorMessage);
        Assert.Equal("draft", model.Content);
        Assert.Equal(1, service.NoteReads);
    }

    private static Task RenameAsync(NotepadViewModel model) => model.RenameFolderCommand.ExecuteAsync(new NotepadFolderRenameRequest("work", "archive"));
    private static TaskCompletionSource Completion() => new(TaskCreationOptions.RunContinuationsAsynchronously);
    private static TaskCompletionSource<T> Completion<T>() => new(TaskCreationOptions.RunContinuationsAsynchronously);
    private static async Task<NotepadViewModel> OpenAsync(RenameService service)
    {
        var model = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await model.OpenAsync();
        await model.SelectNoteCommand.ExecuteAsync(service.Notes.Single(note => note.Id == "one"));
        return model;
    }

    private sealed class RenameService : INotepadService
    {
        public List<NotepadNote> Notes { get; } =
        [
            new("one", "One", "work/child", [], null, null, "one.md"),
            new("other", "Other", "", [], null, null, "other.md"),
            new("similar", "Similar", "workshop", [], null, null, "similar.md"),
        ];
        private List<string> _folders = ["work", "work/child", "workshop"];
        public TaskCompletionSource? PendingRename { get; set; }
        public Exception? RenameError { get; set; }
        public Func<CancellationToken, Task<IReadOnlyList<string>>>? Folders { get; set; }
        public Func<string?, CancellationToken, Task<IReadOnlyList<NotepadNote>>>? ReadNotes { get; set; }
        public List<NotepadFolderRenameRequest> Renames { get; } = [];
        public List<NotepadNoteUpdate> Updates { get; } = [];
        public List<string?> Queries { get; } = [];
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
            Queries.Add(query);
            return ReadNotes?.Invoke(query, cancellationToken) ?? Task.FromResult<IReadOnlyList<NotepadNote>>(
                Notes.Where(note => query is null || note.Title.Contains(query) || note.Folder.Contains(query)).ToArray());
        }
        public Task<NotepadNoteDetail> FetchNoteAsync(string id, CancellationToken cancellationToken = default) =>
            Task.FromResult(new NotepadNoteDetail(Notes.Single(note => note.Id == id), "# " + id));
        public Task<NotepadNoteDetail> UpdateNoteAsync(string id, NotepadNoteUpdate update, CancellationToken cancellationToken = default)
        {
            Updates.Add(update);
            return Task.FromResult(new NotepadNoteDetail(Notes.Single(note => note.Id == id) with { Title = update.Title!, Tags = update.Tags! }, update.Content!));
        }
        public async Task RenameFolderAsync(string from, string to, CancellationToken cancellationToken = default)
        {
            Renames.Add(new(from, to));
            if (RenameError is { } error) throw error;
            if (PendingRename is { } pending) await pending.Task;
            string Map(string path) => path == from || path.StartsWith(from + "/", StringComparison.Ordinal) ? to + path[from.Length..] : path;
            _folders = _folders.Select(Map).ToList();
            for (var i = 0; i < Notes.Count; i++) Notes[i] = Notes[i] with { Folder = Map(Notes[i].Folder) };
        }
        public Task CreateFolderAsync(string folder, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DeleteFolderAsync(string folder, bool recursive, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DeleteNoteAsync(string id, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<NotepadNoteDetail> CreateNoteAsync(NotepadNoteDraft draft, CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }
}
