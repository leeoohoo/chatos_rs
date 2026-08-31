using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class ProjectFilesViewModelTests
{
    [Fact]
    public async Task ProjectFilesOpenDirectoriesBeforeFilesAndOpenTextInPreviewMode()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(new WorkspaceProject(
            "project-1",
            "ChatOS",
            "local://device/workspace/project",
            "project",
            null));
        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries[1]);

        Assert.True(viewModel.Entries[0].IsDirectory);
        Assert.Equal("App.cs", viewModel.SelectedFile?.Name);
        Assert.True(viewModel.IsPreviewMode);
        Assert.False(viewModel.IsEditing);
        Assert.True(viewModel.CanEdit);
    }

    [Fact]
    public async Task EditingRequiresExplicitToggleAndSaveUpdatesAuthoritativePreview()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject(
            "project-1",
            "ChatOS",
            "root",
            "root",
            null));
        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries[1]);

        viewModel.ToggleEditingCommand.Execute(null);
        viewModel.EditorContent = "class Updated {}";
        await viewModel.SaveCommand.ExecuteAsync(null);

        Assert.Equal(("root/App.cs", "class Updated {}"), filesystem.LastWrite);
        Assert.Equal("class Updated {}", viewModel.SelectedFile?.Content);
        Assert.True(viewModel.IsPreviewMode);
    }

    [Fact]
    public async Task CreateRenameAndDeleteRefreshCurrentDirectoryWithStablePaths()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject(
            "project-1",
            "ChatOS",
            "local://device/workspace/project",
            "project",
            null));
        var file = viewModel.Entries.Single(value => !value.IsDirectory);
        var directory = viewModel.Entries.Single(value => value.IsDirectory);

        await viewModel.CreateEntryCommand.ExecuteAsync(new ProjectFileCreationRequest("Notes.md", false));
        await viewModel.CreateEntryCommand.ExecuteAsync(new ProjectFileCreationRequest("docs", true));
        await viewModel.RenameEntryCommand.ExecuteAsync(new ProjectFileRenameRequest(file, "Program.cs"));
        await viewModel.DeleteEntryCommand.ExecuteAsync(directory);

        Assert.Equal(("local://device/workspace/project", "Notes.md"), filesystem.LastCreatedFile);
        Assert.Equal(("local://device/workspace/project", "docs"), filesystem.LastCreatedDirectory);
        Assert.Equal(
            (file.Path, "local://device/workspace/project", "Program.cs", false),
            filesystem.LastMove);
        Assert.Equal((directory.Path, true), filesystem.LastDelete);
        Assert.True(filesystem.ForceRefreshCount >= 4);
        Assert.False(viewModel.IsMutatingEntry);
    }

    [Fact]
    public async Task InvalidEntryNameIsRejectedBeforeCallingFilesystem()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));

        await viewModel.CreateEntryCommand.ExecuteAsync(new ProjectFileCreationRequest("../secret", false));

        Assert.Null(filesystem.LastCreatedFile);
        Assert.NotNull(viewModel.ErrorMessage);
    }

    private sealed class FilesystemDouble : IProjectFilesystemService
    {
        public (string Path, string Content)? LastWrite { get; private set; }

        public (string Parent, string Name)? LastCreatedFile { get; private set; }

        public (string Parent, string Name)? LastCreatedDirectory { get; private set; }

        public (string Source, string Parent, string? Name, bool Replace)? LastMove { get; private set; }

        public (string Path, bool Recursive)? LastDelete { get; private set; }

        public int ForceRefreshCount { get; private set; }

        public Task<ProjectDirectoryListing> ListEntriesAsync(
            string path,
            bool forceRefresh = false,
            CancellationToken cancellationToken = default)
        {
            if (forceRefresh)
            {
                ForceRefreshCount++;
            }

            return Task.FromResult(new ProjectDirectoryListing(
                path,
                null,
                true,
                new[]
                {
                    new ProjectFileEntry("App.cs", $"{path}/App.cs", "App.cs", false, true, 12, null),
                    new ProjectFileEntry("Sources", $"{path}/Sources", "Sources", true, true, null, null),
                },
                false));
        }

        public Task<IReadOnlyList<ProjectFileEntry>> SearchEntriesAsync(
            string path,
            string query,
            int limit = 100,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ProjectFileEntry>>(Array.Empty<ProjectFileEntry>());

        public Task<IReadOnlyList<ProjectFileContentMatch>> SearchContentAsync(
            string path,
            string query,
            int limit = 100,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ProjectFileContentMatch>>(Array.Empty<ProjectFileContentMatch>());

        public Task<ProjectFileContent> ReadFileAsync(
            string path,
            CancellationToken cancellationToken = default) => Task.FromResult(new ProjectFileContent(
            path,
            "App.cs",
            "App.cs",
            "text/x-csharp",
            false,
            true,
            12,
            null,
            "class App {}"));

        public Task WriteFileAsync(
            string path,
            string content,
            CancellationToken cancellationToken = default)
        {
            LastWrite = (path, content);
            return Task.CompletedTask;
        }

        public Task CreateFileAsync(string parentPath, string name, CancellationToken cancellationToken = default)
        {
            LastCreatedFile = (parentPath, name);
            return Task.CompletedTask;
        }

        public Task CreateDirectoryAsync(string parentPath, string name, CancellationToken cancellationToken = default)
        {
            LastCreatedDirectory = (parentPath, name);
            return Task.CompletedTask;
        }

        public Task DeleteEntryAsync(string path, bool recursive, CancellationToken cancellationToken = default)
        {
            LastDelete = (path, recursive);
            return Task.CompletedTask;
        }

        public Task<ProjectFileMoveResult> MoveEntryAsync(
            string sourcePath,
            string targetParentPath,
            string? targetName = null,
            bool replaceExisting = false,
            CancellationToken cancellationToken = default)
        {
            LastMove = (sourcePath, targetParentPath, targetName, replaceExisting);
            return Task.FromResult(new ProjectFileMoveResult(
                sourcePath,
                $"{targetParentPath}/{targetName}",
                targetName,
                targetName,
                false,
                true));
        }

        public Task OpenExternallyAsync(
            string path,
            ProjectFileExternalOpenMode mode,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
    }
}
