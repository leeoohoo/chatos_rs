using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Notepad;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class NotepadViewModelTests
{
    [Fact]
    public async Task OpensInPreviewAndKeepsSelectionAcrossRefresh()
    {
        var service = new NotepadServiceDouble();
        using var viewModel = new NotepadViewModel(service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync();
        var selected = viewModel.SelectedNote?.Id;
        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Equal(NotepadEditorMode.Preview, viewModel.EditorMode);
        Assert.Equal(selected, viewModel.SelectedNote?.Id);
        Assert.Equal("# First", viewModel.Content);
        Assert.False(viewModel.IsDirty);
    }

    [Fact]
    public async Task SavesDirtyNoteBeforeSwitchingSelection()
    {
        var service = new NotepadServiceDouble();
        using var viewModel = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync();
        viewModel.Content = "changed";
        var second = service.Notes[1];

        await viewModel.SelectNoteCommand.ExecuteAsync(second);

        Assert.Equal("changed", service.LastUpdate?.Content);
        Assert.Equal("note-2", viewModel.SelectedNote?.Id);
        Assert.Equal("Second", viewModel.Title);
    }

    [Fact]
    public async Task FolderSelectionFiltersNotesWithoutLosingGlobalResources()
    {
        var service = new NotepadServiceDouble();
        using var viewModel = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync();

        viewModel.SelectFolderCommand.Execute(new NotepadFolderItem("work"));

        Assert.Single(viewModel.Notes);
        Assert.Equal("note-2", viewModel.Notes[0].Id);
        Assert.Contains(viewModel.Folders, folder => folder.Path == "work");
    }

    [Theory]
    [InlineData("work", "work", "work")]
    [InlineData("work/child", " /work\\child/ ", "work/child")]
    [InlineData("work", "child", "work/child")]
    [InlineData("work", "work/child", "work/child")]
    [InlineData("work", "workshop", "work/workshop")]
    [InlineData("", "work", "work")]
    public async Task CreateFolderResolvesRelativeAndCurrentPathsWithoutDuplicatingParent(
        string current, string input, string expected)
    {
        var service = new NotepadServiceDouble();
        using var viewModel = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync();
        viewModel.SelectFolderCommand.Execute(new NotepadFolderItem(current));
        viewModel.Content = "keep this draft";

        await viewModel.CreateFolderCommand.ExecuteAsync(input);

        Assert.Equal(expected, Assert.Single(service.CreatedFolders));
        Assert.Equal(expected, viewModel.SelectedFolder);
        Assert.Equal("note-1", viewModel.SelectedNote?.Id);
        Assert.Equal("keep this draft", viewModel.Content);
        Assert.True(viewModel.IsDirty);
        Assert.Null(service.LastUpdate);
        Assert.False(viewModel.IsBusy);
    }

    [Fact]
    public async Task InvalidFolderCreationClearsPreviousSuccessAndCanBeRetried()
    {
        var service = new NotepadServiceDouble();
        using var viewModel = new NotepadViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync();
        await viewModel.CreateFolderCommand.ExecuteAsync("work");
        Assert.NotNull(viewModel.ActionMessage);
        await viewModel.CreateFolderCommand.ExecuteAsync(" / ");
        Assert.Null(viewModel.ActionMessage);
        Assert.NotNull(viewModel.ErrorMessage);
        Assert.Single(service.CreatedFolders);
        await viewModel.CreateFolderCommand.ExecuteAsync("child");
        Assert.Null(viewModel.ErrorMessage);
        Assert.NotNull(viewModel.ActionMessage);
        Assert.Equal("work/child", viewModel.SelectedFolder);
    }

    private sealed class NotepadServiceDouble : INotepadService
    {
        public List<NotepadNote> Notes { get; } =
        [
            Note("note-1", "First", ""),
            Note("note-2", "Second", "work"),
        ];

        public NotepadNoteUpdate? LastUpdate { get; private set; }

        public List<string> CreatedFolders { get; } = [];

        public Task InitializeAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<IReadOnlyList<string>> ListFoldersAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<string>>(["", "work"]);

        public Task CreateFolderAsync(string folder, CancellationToken cancellationToken = default)
        {
            CreatedFolders.Add(folder);
            return Task.CompletedTask;
        }

        public Task RenameFolderAsync(string from, string to, CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task DeleteFolderAsync(string folder, bool recursive, CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<IReadOnlyList<NotepadNote>> ListNotesAsync(string? query, int limit = 500, CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<NotepadNote>>(Notes);

        public Task<NotepadNoteDetail> CreateNoteAsync(NotepadNoteDraft draft, CancellationToken cancellationToken = default) =>
            Task.FromResult(new NotepadNoteDetail(Note("created", draft.Title, draft.Folder), draft.Content));

        public Task<NotepadNoteDetail> FetchNoteAsync(string id, CancellationToken cancellationToken = default)
        {
            var note = Notes.Single(value => value.Id == id);
            return Task.FromResult(new NotepadNoteDetail(note, id == "note-1" ? "# First" : "# Second"));
        }

        public Task<NotepadNoteDetail> UpdateNoteAsync(string id, NotepadNoteUpdate update, CancellationToken cancellationToken = default)
        {
            LastUpdate = update;
            var note = Notes.Single(value => value.Id == id) with { Title = update.Title ?? string.Empty, Tags = update.Tags ?? [] };
            return Task.FromResult(new NotepadNoteDetail(note, update.Content ?? string.Empty));
        }

        public Task DeleteNoteAsync(string id, CancellationToken cancellationToken = default) => Task.CompletedTask;

        private static NotepadNote Note(string id, string title, string folder) => new(
            id,
            title,
            folder,
            [],
            DateTimeOffset.UtcNow,
            DateTimeOffset.UtcNow,
            $"{id}.md");
    }
}
