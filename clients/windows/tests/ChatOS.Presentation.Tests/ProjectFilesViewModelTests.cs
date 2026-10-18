using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class ProjectFilesViewModelTests
{
    [Fact]
    public async Task EmptyDirectoryEvidenceOnlyAppearsForACompletedSuccessfulListing()
    {
        var filesystem = new FilesystemDouble
        {
            ListingEntries = Array.Empty<ProjectFileEntry>(),
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(new WorkspaceProject(
            "project-empty",
            "Empty project",
            "root",
            "root",
            null));

        Assert.True(viewModel.IsOpen);
        Assert.False(viewModel.IsLoading);
        Assert.True(viewModel.IsDirectoryEmpty);
        Assert.True(viewModel.HasEmptyDirectoryState);

        viewModel.SearchQuery = "marker";
        Assert.False(viewModel.HasEmptyDirectoryState);

        viewModel.SearchQuery = string.Empty;
        viewModel.ErrorMessage = "listing failed";
        Assert.False(viewModel.HasEmptyDirectoryState);

        viewModel.ErrorMessage = null;
        await viewModel.CloseAsync();
        Assert.False(viewModel.HasEmptyDirectoryState);
    }

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
        Assert.Equal("App.cs", viewModel.SelectedFileName);
        Assert.Equal("App.cs", viewModel.SelectedFileDisplayPath);
        Assert.True(viewModel.IsPreviewMode);
        Assert.False(viewModel.IsEditing);
        Assert.True(viewModel.CanEdit);
    }

    [Fact]
    public async Task DirectoryRefreshClearsSelectedFileHeaderProperties()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(value => !value.IsDirectory));
        var changed = new List<string?>();
        viewModel.PropertyChanged += (_, args) => changed.Add(args.PropertyName);

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Null(viewModel.SelectedFile);
        Assert.Empty(viewModel.SelectedFileName);
        Assert.Empty(viewModel.SelectedFileDisplayPath);
        Assert.Contains(nameof(ProjectFilesViewModel.SelectedFileName), changed);
        Assert.Contains(nameof(ProjectFilesViewModel.SelectedFileDisplayPath), changed);
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
        Assert.False(viewModel.IsDirty);
        Assert.False(viewModel.CanSave);

        viewModel.EditorContent = "class Updated {}";
        Assert.True(viewModel.IsDirty);
        Assert.True(viewModel.CanSave);

        await viewModel.SaveCommand.ExecuteAsync(null);

        Assert.Equal(("root/App.cs", "class Updated {}"), filesystem.LastWrite);
        Assert.Equal("class Updated {}", viewModel.SelectedFile?.Content);
        Assert.True(viewModel.IsPreviewMode);
        Assert.False(viewModel.IsDirty);
        Assert.False(viewModel.CanSave);
    }

    [Fact]
    public async Task SaveKeepsEditsMadeWhileTheWriteIsInFlight()
    {
        var filesystem = new FilesystemDouble { DelayWrites = true };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await OpenEditedFileAsync(viewModel);
        const string submitted = "已提交的版本";
        viewModel.EditorContent = submitted;

        var pending = viewModel.SaveCommand.ExecuteAsync(null);
        await filesystem.WriteStarted.Task.WaitAsync(TimeSpan.FromSeconds(2));
        Assert.True(viewModel.IsSaving);
        Assert.False(viewModel.CanSave);
        viewModel.EditorContent = "new unsaved draft";
        filesystem.WriteRelease.TrySetResult();
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.Equal(("root/App.cs", submitted), filesystem.LastWrite);
        Assert.Equal(submitted, viewModel.SelectedFile?.Content);
        Assert.Equal(System.Text.Encoding.UTF8.GetByteCount(submitted), viewModel.SelectedFile?.Size);
        Assert.Equal("new unsaved draft", viewModel.EditorContent);
        Assert.True(viewModel.IsEditing);
        Assert.True(viewModel.IsDirty);
        Assert.True(viewModel.CanSave);
        Assert.False(viewModel.IsSaving);

        await viewModel.SaveCommand.ExecuteAsync(null);

        Assert.Equal(("root/App.cs", "new unsaved draft"), filesystem.LastWrite);
        Assert.Equal("new unsaved draft", viewModel.SelectedFile?.Content);
        Assert.False(viewModel.IsEditing);
        Assert.False(viewModel.IsDirty);
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public async Task LateSaveCannotChangeTheNewSelection(bool changeProject, bool failWrite)
    {
        var filesystem = new FilesystemDouble
        {
            DelayWrites = true,
            WriteFailure = failWrite ? new IOException("old write failed") : null,
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await OpenEditedFileAsync(viewModel);
        viewModel.EditorContent = "old draft";
        var pending = viewModel.SaveCommand.ExecuteAsync(null);
        await filesystem.WriteStarted.Task.WaitAsync(TimeSpan.FromSeconds(2));

        if (changeProject)
        {
            // Reusing the same path also requires project-session isolation.
            await OpenEditedFileAsync(viewModel, "project-new");
        }
        else
        {
            await viewModel.OpenEntryCommand.ExecuteAsync(new ProjectFileEntry(
                "Other.cs", "root/Other.cs", "Other.cs", false, true, 12, null));
            viewModel.ToggleEditingCommand.Execute(null);
        }
        viewModel.EditorContent = "new draft";
        var currentFile = viewModel.SelectedFile;
        filesystem.WriteRelease.TrySetResult();
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.Same(currentFile, viewModel.SelectedFile);
        Assert.Equal("new draft", viewModel.EditorContent);
        Assert.True(viewModel.IsEditing);
        Assert.True(viewModel.IsDirty);
        Assert.False(viewModel.IsSaving);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task LateSaveCannotReopenAClosedProject(bool failWrite)
    {
        var filesystem = new FilesystemDouble
        {
            DelayWrites = true,
            WriteFailure = failWrite ? new IOException("old write failed") : null,
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await OpenEditedFileAsync(viewModel);
        viewModel.EditorContent = "old draft";
        var pending = viewModel.SaveCommand.ExecuteAsync(null);
        await filesystem.WriteStarted.Task.WaitAsync(TimeSpan.FromSeconds(2));

        await viewModel.CloseAsync();
        filesystem.WriteRelease.TrySetResult();
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.False(viewModel.IsOpen);
        Assert.Null(viewModel.SelectedFile);
        Assert.Empty(viewModel.EditorContent);
        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.IsSaving);
    }

    [Fact]
    public async Task FailedSaveKeepsTheCurrentDraftAndReportsTheError()
    {
        var filesystem = new FilesystemDouble { WriteFailure = new IOException("write failed") };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await OpenEditedFileAsync(viewModel);
        viewModel.EditorContent = "keep this draft";

        await viewModel.SaveCommand.ExecuteAsync(null);

        Assert.Equal("class App {}", viewModel.SelectedFile?.Content);
        Assert.Equal("keep this draft", viewModel.EditorContent);
        Assert.Equal("write failed", viewModel.ErrorMessage);
        Assert.True(viewModel.IsDirty);
        Assert.True(viewModel.CanSave);
        Assert.False(viewModel.IsSaving);
    }

    private static async Task OpenEditedFileAsync(ProjectFilesViewModel viewModel, string projectId = "project-1")
    {
        await viewModel.OpenAsync(new WorkspaceProject(projectId, "ChatOS", "root", "root", null));
        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(value => !value.IsDirectory));
        viewModel.ToggleEditingCommand.Execute(null);
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

    [Theory]
    [InlineData("create-file", "文件已创建。")]
    [InlineData("create-directory", "文件夹已创建。")]
    [InlineData("rename-file", "名称已更新。")]
    [InlineData("rename-directory", "名称已更新。")]
    [InlineData("delete-file", "文件已删除。")]
    [InlineData("delete-directory", "文件夹已删除。")]
    public async Task CompletedEntryMutationReportsRefreshFailureAndRetryOnlyListsEntries(string operation, string success)
    {
        var filesystem = new FilesystemDouble
        {
            MutationHandler = (_, _) => Task.CompletedTask,
            RefreshHandler = (count, _) => count == 1
                ? Task.FromException(new IOException("directory unavailable"))
                : Task.CompletedTask,
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));
        var entries = viewModel.Entries.ToArray();

        await StartEntryMutationAsync(viewModel, operation);

        Assert.Equal(success, viewModel.EntryActionMessage);
        Assert.Equal("操作已完成，但目录刷新失败。请刷新目录，无需重复操作。 directory unavailable", viewModel.ErrorMessage);
        Assert.Equal(entries, viewModel.Entries);
        Assert.Equal(1, filesystem.MutationCount);
        Assert.Equal(1, filesystem.ForceRefreshCount);
        Assert.False(viewModel.IsMutatingEntry);
        Assert.False(viewModel.IsLoading);

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal(success, viewModel.EntryActionMessage);
        Assert.Equal(1, filesystem.MutationCount);
        Assert.Equal(2, filesystem.ForceRefreshCount);
        Assert.False(viewModel.IsLoading);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task EntryMutationPublishesSuccessWhileItsDirectoryRefreshIsStillPending(bool failRefresh)
    {
        var refresh = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var started = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble
        {
            MutationHandler = (_, _) => Task.CompletedTask,
            RefreshHandler = (_, _) => { started.TrySetResult(); return refresh.Task; },
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));
        var pending = StartEntryMutationAsync(viewModel, "create-file");
        await started.Task.WaitAsync(TimeSpan.FromSeconds(2));
        var messageWhileRefreshing = viewModel.EntryActionMessage;
        var busyWhileRefreshing = viewModel.IsMutatingEntry && viewModel.IsLoading;
        await viewModel.CreateEntryCommand.ExecuteAsync(new ProjectFileCreationRequest("duplicate", false));
        if (failRefresh) refresh.SetException(new IOException("directory unavailable"));
        else refresh.SetResult();
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.Equal("文件已创建。", messageWhileRefreshing);
        Assert.True(busyWhileRefreshing);
        Assert.Equal(1, filesystem.MutationCount);
        Assert.Equal("文件已创建。", viewModel.EntryActionMessage);
        Assert.Equal(failRefresh, viewModel.ErrorMessage is not null);
        Assert.False(viewModel.IsMutatingEntry);
        Assert.False(viewModel.IsLoading);
    }

    [Theory]
    [InlineData("file", false)]
    [InlineData("file", true)]
    [InlineData("directory", false)]
    [InlineData("directory", true)]
    [InlineData("project", false)]
    [InlineData("project", true)]
    [InlineData("close", false)]
    [InlineData("close", true)]
    public async Task LateEntryRefreshCannotOverwriteANewContext(string transition, bool failRefresh)
    {
        var refresh = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var started = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble
        {
            MutationHandler = (_, _) => Task.CompletedTask,
            RefreshHandler = (_, _) => { started.TrySetResult(); return refresh.Task; },
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));
        var pending = StartEntryMutationAsync(viewModel, "create-file");
        await started.Task.WaitAsync(TimeSpan.FromSeconds(2));
        if (transition == "close") await viewModel.CloseAsync();
        else if (transition == "project") await viewModel.OpenAsync(new WorkspaceProject("next", "Next", "root", "root", null));
        else if (transition == "directory") await viewModel.NavigateToPathAsync("root/Sources");
        else
        {
            await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(entry => !entry.IsDirectory));
            viewModel.ToggleEditingCommand.Execute(null);
            viewModel.EditorContent = "new draft";
        }
        var selected = viewModel.SelectedFile;
        var content = viewModel.EditorContent;
        var path = viewModel.CurrentPath;
        viewModel.ErrorMessage = "current error";
        viewModel.EntryActionMessage = "current action";
        if (failRefresh) refresh.SetException(new IOException("old refresh failed"));
        else refresh.SetResult();
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.Same(selected, viewModel.SelectedFile);
        Assert.Equal(content, viewModel.EditorContent);
        Assert.Equal(path, viewModel.CurrentPath);
        Assert.Equal("current error", viewModel.ErrorMessage);
        Assert.Equal("current action", viewModel.EntryActionMessage);
        Assert.False(viewModel.IsLoading);
        Assert.False(viewModel.IsMutatingEntry);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task EntryRefreshFailureRechecksContextAfterWaitingInTheUiQueue(bool switchProject)
    {
        var refresh = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var dispatcher = new PausingUiDispatcher();
        var filesystem = new FilesystemDouble
        {
            MutationHandler = (_, _) => Task.CompletedTask,
            RefreshHandler = (_, _) => refresh.Task,
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, dispatcher);
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));
        var pending = StartEntryMutationAsync(viewModel, "create-file");
        dispatcher.PauseNextUncancellableDispatch = true;
        refresh.SetException(new IOException("old refresh failed"));
        await dispatcher.Paused.Task.WaitAsync(TimeSpan.FromSeconds(2));
        if (switchProject) await viewModel.OpenAsync(new WorkspaceProject("next", "Next", "root", "root", null));
        else await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(entry => !entry.IsDirectory));
        viewModel.ErrorMessage = "current error";
        viewModel.EntryActionMessage = "current action";
        dispatcher.Release.SetResult();
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.Equal("current error", viewModel.ErrorMessage);
        Assert.Equal("current action", viewModel.EntryActionMessage);
        Assert.False(viewModel.IsLoading);
        Assert.False(viewModel.IsMutatingEntry);
    }

    [Fact]
    public async Task OrdinaryDirectoryRefreshFailureDoesNotClaimAnEntryMutationSucceeded()
    {
        var filesystem = new FilesystemDouble { RefreshHandler = (_, _) => Task.FromException(new IOException("read failed")) };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Equal("read failed", viewModel.ErrorMessage);
        Assert.Null(viewModel.EntryActionMessage);
        Assert.False(viewModel.IsLoading);
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public async Task EntrySuccessRechecksContextAfterWaitingInTheUiQueue(bool switchProject, bool nested)
    {
        var mutation = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var dispatcher = new PausingUiDispatcher();
        var filesystem = new FilesystemDouble { MutationHandler = (_, _) => mutation.Task };
        using var viewModel = new ProjectFilesViewModel(filesystem, dispatcher);
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));
        var changes = new List<string>();
        viewModel.DirectoryChanged += (_, path) => changes.Add(path);
        var pending = viewModel.CreateEntryCommand.ExecuteAsync(new ProjectFileCreationRequest("new", false, nested ? "root/Sources" : null));
        dispatcher.PauseNextDispatch = true;
        mutation.SetResult();
        await dispatcher.Paused.Task.WaitAsync(TimeSpan.FromSeconds(2));
        if (switchProject) await viewModel.OpenAsync(new WorkspaceProject("next", "Next", "root", "root", null));
        else await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(entry => !entry.IsDirectory));
        viewModel.ErrorMessage = "current error";
        viewModel.EntryActionMessage = "current action";
        dispatcher.Release.SetResult();
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.Equal("current error", viewModel.ErrorMessage);
        Assert.Equal("current action", viewModel.EntryActionMessage);
        Assert.Empty(changes);
        Assert.Equal(0, filesystem.ForceRefreshCount);
        Assert.False(viewModel.IsLoading);
        Assert.False(viewModel.IsMutatingEntry);
    }

    [Theory]
    [InlineData("create-file")]
    [InlineData("delete-directory")]
    public async Task FailedEntryMutationDoesNotReportCompletionOrAttemptDirectoryRefresh(string operation)
    {
        var filesystem = new FilesystemDouble { MutationHandler = (_, _) => Task.FromException(new IOException("write denied")) };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));

        await StartEntryMutationAsync(viewModel, operation);

        Assert.Equal("write denied", viewModel.ErrorMessage);
        Assert.Null(viewModel.EntryActionMessage);
        Assert.Equal(1, filesystem.MutationCount);
        Assert.Equal(0, filesystem.ForceRefreshCount);
        Assert.False(viewModel.IsMutatingEntry);
    }

    private static Task StartEntryMutationAsync(ProjectFilesViewModel viewModel, string operation)
    {
        if (operation.StartsWith("create", StringComparison.Ordinal))
            return viewModel.CreateEntryCommand.ExecuteAsync(new ProjectFileCreationRequest("new", operation.EndsWith("directory", StringComparison.Ordinal)));
        var entry = viewModel.Entries.Single(value => value.IsDirectory == operation.EndsWith("directory", StringComparison.Ordinal));
        return operation.StartsWith("rename", StringComparison.Ordinal)
            ? viewModel.RenameEntryCommand.ExecuteAsync(new ProjectFileRenameRequest(entry, "renamed"))
            : viewModel.DeleteEntryCommand.ExecuteAsync(entry);
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public async Task NestedRenameAndDeleteUseTheEntriesParentAndPreserveTheOtherEditor(bool delete, bool directory)
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));
        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(entry => !entry.IsDirectory));
        viewModel.ToggleEditingCommand.Execute(null);
        viewModel.EditorContent = "unrelated draft";
        var file = viewModel.SelectedFile;
        var changes = new List<string>();
        viewModel.DirectoryChanged += (_, path) => changes.Add(path);
        var nested = new ProjectFileEntry("nested", "root/Sources/nested", "Sources/nested", directory, true, null, null);

        if (delete) await viewModel.DeleteEntryCommand.ExecuteAsync(nested);
        else await viewModel.RenameEntryCommand.ExecuteAsync(new ProjectFileRenameRequest(nested, "renamed"));

        if (delete) Assert.Equal((nested.Path, directory), filesystem.LastDelete);
        else Assert.Equal((nested.Path, "root/Sources", "renamed", false), filesystem.LastMove);
        Assert.Equal("root/Sources", Assert.Single(changes));
        Assert.Equal("root", viewModel.CurrentPath);
        Assert.Equal(0, filesystem.ForceRefreshCount);
        Assert.Same(file, viewModel.SelectedFile);
        Assert.Equal("unrelated draft", viewModel.EditorContent);
        Assert.True(viewModel.IsDirty);
        Assert.False(viewModel.IsMutatingEntry);
        Assert.Null(viewModel.ErrorMessage);
        Assert.NotNull(viewModel.EntryActionMessage);
    }

    [Theory]
    [InlineData("local://device/workspace/project/Sources/App.cs", "local://device/workspace/project/Sources")]
    [InlineData("local://device/workspace/project/资料%20文件/App.cs", "local://device/workspace/project/资料%20文件")]
    [InlineData("C:\\work\\project\\Sources\\App.cs", "C:\\work\\project\\Sources")]
    [InlineData("C:\\App.cs", "C:\\")]
    [InlineData("C:/App.cs", "C:/")]
    [InlineData("/App.cs", "/")]
    [InlineData("\\\\server\\share\\Sources\\App.cs", "\\\\server\\share\\Sources")]
    [InlineData("root/Sources/App.cs", "root/Sources")]
    public async Task RenamePreservesTheExactParentPathAcrossFilesystemFormats(string source, string parent)
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));
        var entry = new ProjectFileEntry("App.cs", source, "App.cs", false, true, 12, null);

        await viewModel.RenameEntryCommand.ExecuteAsync(new ProjectFileRenameRequest(entry, "Renamed.cs"));

        Assert.Equal((source, parent, "Renamed.cs", false), filesystem.LastMove);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task FailedNestedMutationDoesNotRefreshOrReportSuccess(bool delete)
    {
        var filesystem = new FilesystemDouble
        {
            MutationHandler = (_, _) => Task.FromException(new IOException("entry failed")),
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));
        var changes = new List<string>();
        viewModel.DirectoryChanged += (_, path) => changes.Add(path);
        var entry = new ProjectFileEntry("nested", "root/Sources/nested", "Sources/nested", false, true, 12, null);

        if (delete) await viewModel.DeleteEntryCommand.ExecuteAsync(entry);
        else await viewModel.RenameEntryCommand.ExecuteAsync(new ProjectFileRenameRequest(entry, "renamed"));

        Assert.Equal("entry failed", viewModel.ErrorMessage);
        Assert.Empty(changes);
        Assert.Equal(0, filesystem.ForceRefreshCount);
        Assert.Null(viewModel.EntryActionMessage);
        Assert.False(viewModel.IsMutatingEntry);
    }

    [Theory]
    [InlineData("root/Sources/nested/", "root/Sources")]
    [InlineData("C:\\root\\Sources\\nested\\", "C:\\root\\Sources")]
    public async Task RenameDirectoryWithTrailingSeparatorStaysInItsParent(string source, string parent)
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));
        var entry = new ProjectFileEntry("nested", source, "Sources/nested", true, true, null, null);

        await viewModel.RenameEntryCommand.ExecuteAsync(new ProjectFileRenameRequest(entry, "renamed"));

        Assert.Equal((source, parent, "renamed", false), filesystem.LastMove);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task MutationWithoutAnEntryParentDoesNotGuessTheBrowsingDirectory(bool delete)
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project", "ChatOS", "root", "root", null));
        var entry = new ProjectFileEntry("unresolved", "unresolved", "unresolved", false, true, null, null);

        if (delete) await viewModel.DeleteEntryCommand.ExecuteAsync(entry);
        else await viewModel.RenameEntryCommand.ExecuteAsync(new ProjectFileRenameRequest(entry, "renamed"));

        Assert.Null(filesystem.LastMove);
        Assert.Null(filesystem.LastDelete);
        Assert.NotNull(viewModel.ErrorMessage);
        Assert.False(viewModel.IsMutatingEntry);
    }

    [Theory]
    [InlineData("create-file", false)]
    [InlineData("create-file", true)]
    [InlineData("create-directory", false)]
    [InlineData("create-directory", true)]
    [InlineData("rename", false)]
    [InlineData("rename", true)]
    [InlineData("delete", false)]
    [InlineData("delete", true)]
    public async Task EntryMutationFinishesAfterOpeningAnotherFile(string operation, bool fail)
    {
        await AssertMutationFinishesAfterNavigationAsync(operation, "file", fail);
    }

    [Theory]
    [InlineData("refresh", false)]
    [InlineData("refresh", true)]
    [InlineData("directory", false)]
    [InlineData("directory", true)]
    public async Task EntryMutationFinishesAfterDirectoryNavigation(string transition, bool fail)
    {
        await AssertMutationFinishesAfterNavigationAsync("create-file", transition, fail);
    }

    private static async Task AssertMutationFinishesAfterNavigationAsync(string operation, string transition, bool fail)
    {
        var pending = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble { MutationHandler = (_, _) => pending.Task };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        var file = viewModel.Entries.Single(entry => !entry.IsDirectory);
        var mutation = operation switch
        {
            "rename" => viewModel.RenameEntryCommand.ExecuteAsync(new ProjectFileRenameRequest(file, "Renamed.cs")),
            "delete" => viewModel.DeleteEntryCommand.ExecuteAsync(file),
            _ => viewModel.CreateEntryCommand.ExecuteAsync(new ProjectFileCreationRequest("new", operation == "create-directory")),
        };
        Assert.True(viewModel.IsMutatingEntry);
        Assert.Equal(1, filesystem.MutationCount);

        if (transition == "file") await viewModel.OpenEntryCommand.ExecuteAsync(file);
        else if (transition == "refresh") await viewModel.RefreshCommand.ExecuteAsync(null);
        else await viewModel.NavigateToPathAsync("root/Sources");
        var currentPath = viewModel.CurrentPath;
        var listingCount = filesystem.ForceRefreshCount;
        if (transition == "file")
        {
            viewModel.ToggleEditingCommand.Execute(null);
            viewModel.EditorContent = "new draft";
        }
        Assert.True(viewModel.IsMutatingEntry);
        // Other entry actions must stay serialized while the connector is pending.
        await viewModel.CreateEntryCommand.ExecuteAsync(new ProjectFileCreationRequest("duplicate", false));
        Assert.Equal(1, filesystem.MutationCount);

        if (fail) pending.SetException(new IOException("old mutation failed"));
        else pending.SetResult();
        await mutation;

        Assert.False(viewModel.IsMutatingEntry);
        Assert.Equal(currentPath, viewModel.CurrentPath);
        Assert.Equal(listingCount, filesystem.ForceRefreshCount);
        Assert.Null(viewModel.ErrorMessage);
        Assert.Null(viewModel.EntryActionMessage);
        if (transition == "file")
        {
            Assert.Equal(file.Path, viewModel.SelectedFile?.Path);
            Assert.Equal("new draft", viewModel.EditorContent);
            Assert.True(viewModel.IsDirty);
        }
        // Completion must restore the ability to perform the next entry action.
        await viewModel.CreateEntryCommand.ExecuteAsync(new ProjectFileCreationRequest("next", false));
        Assert.Equal(2, filesystem.MutationCount);
        Assert.False(viewModel.IsMutatingEntry);
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public async Task OldMutationCannotClearNewProjectsPendingMutation(bool closeFirst, bool failOld)
    {
        var oldResult = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var newResult = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble
        {
            MutationHandler = (count, _) => count == 1 ? oldResult.Task : newResult.Task,
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("old", "Old", "root", "root", null));
        var oldMutation = viewModel.CreateEntryCommand.ExecuteAsync(new ProjectFileCreationRequest("old", false));
        if (closeFirst) await viewModel.CloseAsync();
        // The new project deliberately uses the same filesystem path.
        await viewModel.OpenAsync(new WorkspaceProject("new", "New", "root", "root", null));
        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(entry => !entry.IsDirectory));
        viewModel.ToggleEditingCommand.Execute(null);
        viewModel.EditorContent = "new project draft";
        var newMutation = viewModel.CreateEntryCommand.ExecuteAsync(new ProjectFileCreationRequest("new", false));
        Assert.Equal(2, filesystem.MutationCount);

        if (failOld) oldResult.SetException(new IOException("old project failed"));
        else oldResult.SetResult();
        await oldMutation;

        Assert.Equal("new", viewModel.ProjectId);
        Assert.True(viewModel.IsMutatingEntry);
        Assert.False(viewModel.IsLoading);
        Assert.Equal("new project draft", viewModel.EditorContent);
        Assert.True(viewModel.IsDirty);
        Assert.Null(viewModel.ErrorMessage);
        Assert.Null(viewModel.EntryActionMessage);
        newResult.SetResult();
        await newMutation;
        Assert.False(viewModel.IsMutatingEntry);
    }

    [Fact]
    public async Task CancelEditingRestoresAuthoritativeFileContent()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(value => !value.IsDirectory));
        viewModel.ToggleEditingCommand.Execute(null);
        viewModel.EditorContent = "unsaved";

        viewModel.CancelEditingCommand.Execute(null);

        Assert.False(viewModel.IsEditing);
        Assert.Equal("class App {}", viewModel.EditorContent);
        Assert.False(viewModel.IsDirty);
    }

    [Fact]
    public async Task ImageFilesUseImagePreviewAndCanOpenExternally()
    {
        var filesystem = new FilesystemDouble
        {
            ReadResult = new ProjectFileContent(
                "root/image.png", "image.png", "image.png", "image/png",
                true, true, 4, null, "iVBORw=="),
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(value => !value.IsDirectory));

        Assert.True(viewModel.HasImagePreview);
        Assert.False(viewModel.HasBinaryPreview);
        Assert.False(viewModel.HasTextPreview);

        await viewModel.OpenSelectedExternallyAsync(ProjectFileExternalOpenMode.Reveal);

        Assert.Equal(("root/image.png", ProjectFileExternalOpenMode.Reveal), filesystem.LastExternalOpen);
    }

    [Theory]
    [InlineData(8, 3)]
    [InlineData(1, 1)]
    public async Task MarkdownFilesUseReaderPreviewUntilCodeNavigationTargetsALine(int line, int column)
    {
        var filesystem = new FilesystemDouble
        {
            ReadResult = new ProjectFileContent(
                "root/README.md", "README.md", "README.md", "text/markdown",
                false, true, 9, null, "# ChatOS"),
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(value => !value.IsDirectory));

        Assert.True(viewModel.HasMarkdownPreview);
        Assert.False(viewModel.HasCodeTextPreview);

        await viewModel.OpenNavigationLocationCommand.ExecuteAsync(new ProjectCodeNavigationLocation(
            "root/README.md", "README.md", line, column, line, column + 5, "target", 1));

        Assert.False(viewModel.HasMarkdownPreview);
        Assert.True(viewModel.HasCodeTextPreview);
        Assert.Equal(line, viewModel.NavigationLine);
        Assert.Equal(column, viewModel.NavigationColumn);
    }

    [Theory]
    [InlineData("App.cs", 42, 7)]
    [InlineData("README.md", 8, 3)]
    [InlineData("README.md", 1, 1)]
    public async Task ContentSearchOpensTheMatchedFileAtItsLineAndColumn(string name, int line, int column)
    {
        var filesystem = new FilesystemDouble
        {
            ReadResult = new ProjectFileContent(
                $"root/{name}", name, name, "text/plain", false, true, 8, null, "# ChatOS"),
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));

        await viewModel.OpenContentMatchCommand.ExecuteAsync(new ProjectFileContentMatch(
            $"root/{name}", name, line, column, "matched text"));

        Assert.Equal($"root/{name}", filesystem.ReadPaths.Single());
        Assert.Equal($"root/{name}", viewModel.SelectedFile?.Path);
        Assert.Equal(line, viewModel.NavigationLine);
        Assert.Equal(column, viewModel.NavigationColumn);
        Assert.True(viewModel.HasNavigationTarget);
        Assert.True(viewModel.HasCodeTextPreview);
        Assert.False(viewModel.HasMarkdownPreview);
        Assert.False(viewModel.IsEditing);
    }

    [Fact]
    public async Task OpeningMarkdownNormallyAfterSearchRestoresReaderPreview()
    {
        var filesystem = new FilesystemDouble
        {
            ReadResult = new ProjectFileContent(
                "root/README.md", "README.md", "README.md", "text/markdown",
                false, true, 8, null, "# ChatOS"),
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        await viewModel.OpenContentMatchCommand.ExecuteAsync(new ProjectFileContentMatch(
            "root/README.md", "README.md", 8, 3, "matched text"));

        await viewModel.OpenEntryCommand.ExecuteAsync(new ProjectFileEntry(
            "README.md", "root/README.md", "README.md", false, true, 8, null));

        Assert.True(viewModel.HasMarkdownPreview);
        Assert.False(viewModel.HasCodeTextPreview);
        Assert.False(viewModel.HasNavigationTarget);
        Assert.Equal(1, viewModel.NavigationLine);
        Assert.Equal(1, viewModel.NavigationColumn);
    }

    [Fact]
    public async Task CodeNavigationCanReturnToPreviousFileAndLine()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(value => !value.IsDirectory));

        await viewModel.OpenNavigationLocationCommand.ExecuteAsync(new ProjectCodeNavigationLocation(
            "root/Definition.cs", "Definition.cs", 42, 7, 42, 12, "class Definition", 1));

        Assert.True(viewModel.CanNavigateBack);
        Assert.Equal("root/Definition.cs", filesystem.ReadPaths[^1]);
        Assert.Equal(42, viewModel.NavigationLine);

        await viewModel.NavigateBackCommand.ExecuteAsync(null);

        Assert.False(viewModel.CanNavigateBack);
        Assert.Equal("root/App.cs", filesystem.ReadPaths[^1]);
        Assert.Equal(1, viewModel.NavigationLine);
    }

    [Fact]
    public async Task ChangingSearchTextRunsDebouncedFileAndContentSearch()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));

        viewModel.SearchQuery = "App";
        await Task.Delay(450);

        Assert.Equal(1, filesystem.EntrySearchCount);
        Assert.Equal(1, filesystem.ContentSearchCount);
        Assert.False(viewModel.IsSearching);
    }

    [Fact]
    public async Task EmptySearchKeepsTheSearchSurfaceActiveUntilItIsCleared()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));

        viewModel.SearchQuery = "missing";
        await viewModel.SearchCommand.ExecuteAsync(null);

        Assert.True(viewModel.IsSearchActive);
        Assert.False(viewModel.HasSearchResults);
        Assert.True(viewModel.HasNoSearchResults);

        viewModel.ClearSearchCommand.Execute(null);

        Assert.False(viewModel.IsSearchActive);
        Assert.False(viewModel.HasNoSearchResults);
    }

    [Theory]
    [InlineData("file", false)]
    [InlineData("file", true)]
    [InlineData("project", false)]
    [InlineData("project", true)]
    [InlineData("close", false)]
    [InlineData("close", true)]
    [InlineData("position", false)]
    [InlineData("position", true)]
    public async Task LateCodeNavigationCannotChangeANewContext(string transition, bool failOld)
    {
        var response = new TaskCompletionSource<ProjectCodeNavigationResult>(TaskCreationOptions.RunContinuationsAsynchronously);
        var started = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken oldToken = default;
        var navigation = new NavigationDouble((_, _, _, _, _, token) =>
        {
            oldToken = token;
            started.TrySetResult();
            return response.Task;
        });
        using var viewModel = new ProjectFilesViewModel(new FilesystemDouble(), new ImmediateUiDispatcher(), navigation);
        await OpenEditedFileAsync(viewModel);
        var pending = viewModel.FindDefinitionCommand.ExecuteAsync(null);
        await started.Task.WaitAsync(TimeSpan.FromSeconds(2));

        if (transition == "close") await viewModel.CloseAsync();
        else if (transition == "project") await OpenEditedFileAsync(viewModel, "project-new");
        else if (transition == "position") viewModel.NavigationColumn = 2;
        else await viewModel.OpenEntryCommand.ExecuteAsync(SearchEntry("Other.cs"));
        var cancelledBeforeCompletion = oldToken.IsCancellationRequested;
        var currentFile = viewModel.SelectedFile;
        viewModel.ErrorMessage = "current error";
        viewModel.EntryActionMessage = "current action";
        CompleteNavigation(response, failOld, "old");
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.True(cancelledBeforeCompletion);
        Assert.Same(currentFile, viewModel.SelectedFile);
        Assert.Equal("current error", viewModel.ErrorMessage);
        Assert.Equal("current action", viewModel.EntryActionMessage);
        Assert.Empty(viewModel.NavigationLocations);
        Assert.Null(viewModel.NavigationToken);
        Assert.False(viewModel.IsNavigating);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CodeNavigationCallbackRechecksContextAfterWaitingInTheUiQueue(bool failOld)
    {
        var response = new TaskCompletionSource<ProjectCodeNavigationResult>(TaskCreationOptions.RunContinuationsAsynchronously);
        var dispatcher = new PausingUiDispatcher();
        using var viewModel = new ProjectFilesViewModel(new FilesystemDouble(), dispatcher,
            new NavigationDouble((_, _, _, _, _, _) => response.Task));
        await OpenEditedFileAsync(viewModel);
        var pending = viewModel.FindDefinitionCommand.ExecuteAsync(null);
        dispatcher.PauseNextDispatch = true;
        CompleteNavigation(response, failOld, "old");
        await dispatcher.Paused.Task.WaitAsync(TimeSpan.FromSeconds(2));

        await viewModel.OpenEntryCommand.ExecuteAsync(SearchEntry("Other.cs"));
        viewModel.ErrorMessage = "new file error";
        viewModel.EntryActionMessage = "new file action";
        dispatcher.Release.TrySetResult();
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.Empty(viewModel.NavigationLocations);
        Assert.Null(viewModel.NavigationToken);
        Assert.Equal("new file error", viewModel.ErrorMessage);
        Assert.Equal("new file action", viewModel.EntryActionMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task NewReferencesRequestReplacesAPendingDefinitionRequest(bool failOld)
    {
        var response = new TaskCompletionSource<ProjectCodeNavigationResult>(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken oldToken = default;
        var navigation = new NavigationDouble((definition, _, _, _, _, token) =>
        {
            if (!definition) return Task.FromResult(NavigationResult("new"));
            oldToken = token;
            return response.Task;
        });
        using var viewModel = new ProjectFilesViewModel(new FilesystemDouble(), new ImmediateUiDispatcher(), navigation);
        await OpenEditedFileAsync(viewModel);
        var pending = viewModel.FindDefinitionCommand.ExecuteAsync(null);
        await viewModel.FindReferencesCommand.ExecuteAsync(null);
        var cancelledBeforeCompletion = oldToken.IsCancellationRequested;
        CompleteNavigation(response, failOld, "old");
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.True(cancelledBeforeCompletion);
        Assert.Equal("new", viewModel.NavigationToken);
        Assert.Equal("引用", viewModel.NavigationMode);
        Assert.Equal("new", viewModel.NavigationLocations.Single().Preview);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task CompletingCodeNavigationDoesNotStopAFileSearch()
    {
        var response = new TaskCompletionSource<ProjectCodeNavigationResult>(TaskCreationOptions.RunContinuationsAsynchronously);
        var entries = new TaskCompletionSource<IReadOnlyList<ProjectFileEntry>>(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble { EntrySearchHandler = (_, _) => entries.Task };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher(),
            new NavigationDouble((_, _, _, _, _, _) => response.Task));
        await OpenEditedFileAsync(viewModel);
        var pending = viewModel.FindDefinitionCommand.ExecuteAsync(null);
        viewModel.SearchQuery = "App";
        var search = viewModel.SearchCommand.ExecuteAsync(null);
        response.TrySetResult(NavigationResult("target"));
        await pending.WaitAsync(TimeSpan.FromSeconds(2));
        var stillSearching = viewModel.IsSearching;
        entries.TrySetResult([SearchEntry("App")]);
        await search.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.True(stillSearching);
        Assert.False(viewModel.IsSearching);
        Assert.Equal("App", viewModel.FileSearchResults.Single().Name);
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public async Task CurrentCodeNavigationUsesTheRequestedLocationAndReportsItsResult(bool definition, bool fail)
    {
        (bool Definition, string Root, string Path, int Line, int Column)? request = null;
        var navigation = new NavigationDouble((isDefinition, root, path, line, column, _) =>
        {
            request = (isDefinition, root, path, line, column);
            return fail ? Task.FromException<ProjectCodeNavigationResult>(new IOException("navigation failed"))
                : Task.FromResult(NavigationResult("target"));
        });
        using var viewModel = new ProjectFilesViewModel(new FilesystemDouble(), new ImmediateUiDispatcher(), navigation);
        await OpenEditedFileAsync(viewModel);
        viewModel.NavigationLine = 7;
        viewModel.NavigationColumn = 3;

        await (definition ? viewModel.FindDefinitionCommand : viewModel.FindReferencesCommand).ExecuteAsync(null);

        Assert.Equal((definition, "root", "root/App.cs", 7, 3), request);
        Assert.False(viewModel.IsSearching);
        Assert.False(viewModel.IsNavigating);
        if (fail)
        {
            Assert.Equal("navigation failed", viewModel.ErrorMessage);
            Assert.Empty(viewModel.NavigationLocations);
        }
        else
        {
            Assert.Null(viewModel.ErrorMessage);
            Assert.Equal("target", viewModel.NavigationToken);
            Assert.Equal(definition ? "定义" : "引用", viewModel.NavigationMode);
            Assert.Single(viewModel.NavigationLocations);
        }
    }

    private static ProjectCodeNavigationResult NavigationResult(string token) =>
        new("test", "csharp", "test", token,
            [new ProjectCodeNavigationLocation("root/App.cs", "App.cs", 7, 3, 7, 8, token, 1)]);

    [Fact]
    public async Task CompletingFileSearchDoesNotStopCodeNavigation()
    {
        var response = new TaskCompletionSource<ProjectCodeNavigationResult>(TaskCreationOptions.RunContinuationsAsynchronously);
        using var viewModel = new ProjectFilesViewModel(new FilesystemDouble(), new ImmediateUiDispatcher(),
            new NavigationDouble((_, _, _, _, _, _) => response.Task));
        await OpenEditedFileAsync(viewModel);
        var pending = viewModel.FindDefinitionCommand.ExecuteAsync(null);
        Assert.True(viewModel.IsNavigating);
        Assert.False(viewModel.IsSearching);
        viewModel.SearchQuery = "App";
        await viewModel.SearchCommand.ExecuteAsync(null);
        var stillNavigating = viewModel.IsNavigating;
        response.TrySetResult(NavigationResult("target"));
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.True(stillNavigating);
        Assert.False(viewModel.IsNavigating);
        Assert.False(viewModel.IsSearching);
        Assert.Equal("target", viewModel.NavigationToken);
    }

    [Fact]
    public async Task CancellingTheCurrentCodeNavigationEndsItsBusyState()
    {
        var response = new TaskCompletionSource<ProjectCodeNavigationResult>(TaskCreationOptions.RunContinuationsAsynchronously);
        using var cancellation = new CancellationTokenSource();
        using var viewModel = new ProjectFilesViewModel(new FilesystemDouble(), new ImmediateUiDispatcher(),
            new NavigationDouble((_, _, _, _, _, _) => response.Task));
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null), cancellation.Token);
        await viewModel.OpenEntryCommand.ExecuteAsync(SearchEntry("App.cs"));
        var pending = viewModel.FindReferencesCommand.ExecuteAsync(null);
        Assert.True(viewModel.IsNavigating);
        cancellation.Cancel();
        response.TrySetResult(NavigationResult("cancelled"));
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.False(viewModel.IsNavigating);
        Assert.False(viewModel.IsSearching);
        Assert.Null(viewModel.ErrorMessage);
        Assert.Empty(viewModel.NavigationLocations);
        Assert.True(viewModel.IsOpen);
    }

    private static void CompleteNavigation(TaskCompletionSource<ProjectCodeNavigationResult> response, bool fail, string token)
    {
        if (fail) response.TrySetException(new IOException("old navigation failed"));
        else response.TrySetResult(NavigationResult(token));
    }

    private sealed class NavigationDouble(
        Func<bool, string, string, int, int, CancellationToken, Task<ProjectCodeNavigationResult>> handler)
        : IProjectCodeNavigationService
    {
        public Task<ProjectCodeNavigationResult> DefinitionAsync(string projectRoot, string filePath, int line, int column,
            CancellationToken cancellationToken = default) => handler(true, projectRoot, filePath, line, column, cancellationToken);

        public Task<ProjectCodeNavigationResult> ReferencesAsync(string projectRoot, string filePath, int line, int column,
            CancellationToken cancellationToken = default) => handler(false, projectRoot, filePath, line, column, cancellationToken);
    }

    [Fact]
    public async Task SearchAlwaysUsesProjectRootAfterEnteringChildDirectory()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));

        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(value => value.IsDirectory));
        viewModel.SearchQuery = "App";
        await Task.Delay(450);

        Assert.Equal("root", filesystem.LastEntrySearchPath);
        Assert.Equal("root", filesystem.LastContentSearchPath);
        Assert.Equal(200, filesystem.LastContentSearchLimit);
    }

    [Fact]
    public async Task BreadcrumbsExposeRootAndCurrentDirectoryForDirectNavigation()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));

        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(value => value.IsDirectory));

        Assert.Equal(["root", "Sources"], viewModel.Breadcrumbs.Select(value => value.Name));
        Assert.Equal(["root", "root/Sources"], viewModel.Breadcrumbs.Select(value => value.Path));

        await viewModel.NavigateToPathAsync("root");

        Assert.Single(viewModel.Breadcrumbs);
        Assert.Equal("root", viewModel.CurrentPath);
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public async Task SearchPreservesTheOtherResultWhenOneProviderFails(bool failEntries, bool synchronous)
    {
        Task<IReadOnlyList<T>> Failure<T>() => synchronous
            ? throw new IOException("search unavailable")
            : Task.FromException<IReadOnlyList<T>>(new IOException("search unavailable"));
        var filesystem = new FilesystemDouble
        {
            EntrySearchHandler = (_, _) => failEntries
                ? Failure<ProjectFileEntry>()
                : Task.FromResult<IReadOnlyList<ProjectFileEntry>>([SearchEntry("App")]),
            ContentSearchHandler = (_, _) => failEntries
                ? Task.FromResult<IReadOnlyList<ProjectFileContentMatch>>([SearchMatch("App")])
                : Failure<ProjectFileContentMatch>(),
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        viewModel.SearchQuery = "App";

        await viewModel.SearchCommand.ExecuteAsync(null);

        Assert.Equal(failEntries ? 0 : 1, viewModel.FileSearchResults.Count);
        Assert.Equal(failEntries ? 1 : 0, viewModel.ContentSearchResults.Count);
        Assert.Contains(failEntries ? "文件名搜索失败" : "内容搜索失败", viewModel.ErrorMessage);
        Assert.Contains("search unavailable", viewModel.ErrorMessage);
        Assert.False(viewModel.IsSearching);
    }

    [Theory]
    [InlineData("query", false)]
    [InlineData("query", true)]
    [InlineData("clear", false)]
    [InlineData("clear", true)]
    [InlineData("project", false)]
    [InlineData("project", true)]
    [InlineData("close", false)]
    [InlineData("close", true)]
    public async Task ReplacingAManualSearchCancelsItAndIgnoresItsLateResponse(string transition, bool failOld)
    {
        var entries = new TaskCompletionSource<IReadOnlyList<ProjectFileEntry>>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var content = new TaskCompletionSource<IReadOnlyList<ProjectFileContentMatch>>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var started = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken oldToken = default;
        var filesystem = new FilesystemDouble
        {
            EntrySearchHandler = (query, token) =>
            {
                if (query != "old") return Task.FromResult<IReadOnlyList<ProjectFileEntry>>([SearchEntry(query)]);
                oldToken = token;
                return entries.Task;
            },
            ContentSearchHandler = (query, _) =>
            {
                if (query != "old") return Task.FromResult<IReadOnlyList<ProjectFileContentMatch>>([SearchMatch(query)]);
                started.TrySetResult();
                return content.Task;
            },
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        viewModel.SearchQuery = "old";
        var pending = viewModel.SearchCommand.ExecuteAsync(null);
        await started.Task.WaitAsync(TimeSpan.FromSeconds(2));

        if (transition == "close") await viewModel.CloseAsync();
        else if (transition == "clear") viewModel.ClearSearchCommand.Execute(null);
        else
        {
            if (transition == "project")
                await viewModel.OpenAsync(new WorkspaceProject("project-new", "New", "root", "root", null));
            viewModel.SearchQuery = "new";
            await viewModel.SearchCommand.ExecuteAsync(null);
        }
        var cancelledBeforeCompletion = oldToken.IsCancellationRequested;
        viewModel.ErrorMessage = "current page marker";
        if (failOld)
        {
            entries.TrySetException(new IOException("old entries failed"));
            content.TrySetException(new IOException("old content failed"));
        }
        else
        {
            entries.TrySetResult([SearchEntry("old")]);
            content.TrySetResult([SearchMatch("old")]);
        }
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.True(cancelledBeforeCompletion);
        Assert.Equal("current page marker", viewModel.ErrorMessage);
        Assert.False(viewModel.IsSearching);
        if (transition is "query" or "project")
        {
            Assert.Equal("new", viewModel.FileSearchResults.Single().Name);
            Assert.Equal("new", viewModel.ContentSearchResults.Single().Text);
        }
        else
        {
            Assert.Empty(viewModel.FileSearchResults);
            Assert.Empty(viewModel.ContentSearchResults);
        }
    }

    [Fact]
    public async Task DebouncedSearchUpdatesObservableStateThroughTheUiDispatcher()
    {
        var dispatcher = new SearchUiDispatcher();
        using var viewModel = new ProjectFilesViewModel(new FilesystemDouble(), dispatcher);
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        var outsideDispatch = new System.Collections.Concurrent.ConcurrentQueue<string>();
        var finished = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        viewModel.PropertyChanged += (_, change) =>
        {
            if (change.PropertyName is not (nameof(ProjectFilesViewModel.IsSearching) or
                nameof(ProjectFilesViewModel.ErrorMessage))) return;
            if (!dispatcher.IsDispatching) outsideDispatch.Enqueue(change.PropertyName);
            if (change.PropertyName == nameof(ProjectFilesViewModel.IsSearching) && !viewModel.IsSearching)
                finished.TrySetResult();
        };

        await dispatcher.InvokeAsync(() => viewModel.SearchQuery = "App");
        await finished.Task.WaitAsync(TimeSpan.FromSeconds(3));

        Assert.Empty(outsideDispatch);
    }

    private static ProjectFileEntry SearchEntry(string name) =>
        new(name, $"root/{name}", name, false, true, null, null);

    private static ProjectFileContentMatch SearchMatch(string text) =>
        new($"root/{text}", text, 1, 1, text);

    private sealed class SearchUiDispatcher : IUiDispatcher
    {
        [ThreadStatic]
        private static int _depth;
        public bool IsDispatching => _depth > 0;

        public Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            _depth++;
            try { action(); }
            finally { _depth--; }
            return Task.CompletedTask;
        }
    }

    [Fact]
    public async Task TreeFolderCreationUsesTheFolderThatOpenedTheContextMenu()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        string? changedPath = null;
        viewModel.DirectoryChanged += (_, path) => changedPath = path;

        await viewModel.CreateEntryCommand.ExecuteAsync(
            new ProjectFileCreationRequest("notes", true, "root/Sources"));

        Assert.Equal(("root/Sources", "notes"), filesystem.LastCreatedDirectory);
        Assert.Equal("root", viewModel.CurrentPath);
        Assert.Equal("root/Sources", changedPath);
    }

    [Fact]
    public async Task LateTreeListingFromPreviousProjectIsIgnored()
    {
        var filesystem = new FilesystemDouble
        {
            DelayTreePath = "root/Sources",
        };
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(new WorkspaceProject("project-old", "Old", "root", "root", null));
        var pending = viewModel.LoadTreeDirectoryAsync("root/Sources");
        await filesystem.TreeListingStarted.Task.WaitAsync(TimeSpan.FromSeconds(2));

        await viewModel.OpenAsync(new WorkspaceProject("project-new", "New", "new-root", "new-root", null));
        filesystem.TreeListingRelease.TrySetResult();

        Assert.Null(await pending);
        Assert.Equal("new-root", viewModel.ProjectRoot);
        Assert.Equal("new-root", viewModel.CurrentPath);
    }

    [Fact]
    public async Task ClosingProjectClearsFileSessionAndUnsavedState()
    {
        var filesystem = new FilesystemDouble();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(value => !value.IsDirectory));
        viewModel.ToggleEditingCommand.Execute(null);
        viewModel.EditorContent = "unsaved";

        await viewModel.CloseAsync();

        Assert.False(viewModel.IsOpen);
        Assert.Null(viewModel.ProjectId);
        Assert.Null(viewModel.SelectedFile);
        Assert.False(viewModel.IsDirty);
        Assert.Empty(viewModel.Entries);
        Assert.Empty(viewModel.Breadcrumbs);
    }

    [Theory]
    [InlineData(false, "file", false)]
    [InlineData(false, "file", true)]
    [InlineData(false, "project", false)]
    [InlineData(false, "project", true)]
    [InlineData(false, "close", false)]
    [InlineData(false, "close", true)]
    [InlineData(true, "file", false)]
    [InlineData(true, "file", true)]
    [InlineData(true, "project", false)]
    [InlineData(true, "project", true)]
    [InlineData(true, "close", false)]
    [InlineData(true, "close", true)]
    public async Task LateFileOrDirectoryReadCannotChangeTheCurrentPage(
        bool directory, string transition, bool failRead)
    {
        var filesystem = DelayedReadFilesystem(directory, failRead);
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        var pending = StartDelayedRead(viewModel, directory);
        await ReadStarted(filesystem, directory).WaitAsync(TimeSpan.FromSeconds(2));
        Assert.True(viewModel.IsLoading);

        if (transition == "close")
        {
            await viewModel.CloseAsync();
        }
        else
        {
            if (transition == "project")
            {
                // A new project may point to exactly the same workspace path.
                await viewModel.OpenAsync(new WorkspaceProject("project-new", "New", "root", "root", null));
            }
            await viewModel.OpenEntryCommand.ExecuteAsync(viewModel.Entries.Single(value => !value.IsDirectory));
            viewModel.ToggleEditingCommand.Execute(null);
            viewModel.EditorContent = "current unsaved draft";
            viewModel.ErrorMessage = "current page error";
        }
        var currentFile = viewModel.SelectedFile;
        var currentError = viewModel.ErrorMessage;

        ReleaseRead(filesystem, directory);
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.Equal(currentError, viewModel.ErrorMessage);
        Assert.Same(currentFile, viewModel.SelectedFile);
        Assert.False(viewModel.IsLoading);
        Assert.Equal(transition == "close" ? string.Empty : "root", viewModel.CurrentPath);
        if (transition == "close")
        {
            Assert.False(viewModel.IsOpen);
            Assert.Null(viewModel.ProjectId);
            Assert.Empty(viewModel.EditorContent);
            Assert.Empty(viewModel.Entries);
        }
        else
        {
            Assert.Equal(transition == "project" ? "project-new" : "project-1", viewModel.ProjectId);
            Assert.True(viewModel.IsEditing);
            Assert.True(viewModel.IsDirty);
            Assert.Equal("current unsaved draft", viewModel.EditorContent);
        }
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ReadFailureQueuedBeforeProjectChangeCannotOverwriteTheNewError(bool directory)
    {
        var filesystem = DelayedReadFilesystem(directory, failRead: true);
        var dispatcher = new PausingUiDispatcher();
        using var viewModel = new ProjectFilesViewModel(filesystem, dispatcher);
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        var pending = StartDelayedRead(viewModel, directory);
        await ReadStarted(filesystem, directory).WaitAsync(TimeSpan.FromSeconds(2));
        dispatcher.PauseNextUncancellableDispatch = true;
        ReleaseRead(filesystem, directory);
        await dispatcher.Paused.Task.WaitAsync(TimeSpan.FromSeconds(2));

        await viewModel.OpenAsync(new WorkspaceProject("project-new", "New", "root", "root", null));
        viewModel.ErrorMessage = "new project error";
        dispatcher.Release.TrySetResult();
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.Equal("new project error", viewModel.ErrorMessage);
        Assert.Equal("project-new", viewModel.ProjectId);
        Assert.False(viewModel.IsLoading);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ClosingBeforeAQueuedReadStartsCompletesWithoutAnError(bool directory)
    {
        var dispatcher = new PausingUiDispatcher();
        using var viewModel = new ProjectFilesViewModel(new FilesystemDouble(), dispatcher);
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        dispatcher.PauseNextDispatch = true;
        var pending = StartDelayedRead(viewModel, directory);
        await dispatcher.Paused.Task.WaitAsync(TimeSpan.FromSeconds(2));

        await viewModel.CloseAsync();
        dispatcher.Release.TrySetResult();
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.False(viewModel.IsOpen);
        Assert.False(viewModel.IsLoading);
        Assert.Null(viewModel.ErrorMessage);
        Assert.Null(viewModel.SelectedFile);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CancellingTheCurrentReadStillEndsItsLoadingState(bool directory)
    {
        var filesystem = DelayedReadFilesystem(directory, failRead: false);
        using var sessionCancellation = new CancellationTokenSource();
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(
            new WorkspaceProject("project-1", "ChatOS", "root", "root", null), sessionCancellation.Token);
        var pending = StartDelayedRead(viewModel, directory);
        await ReadStarted(filesystem, directory).WaitAsync(TimeSpan.FromSeconds(2));
        Assert.True(viewModel.IsLoading);

        sessionCancellation.Cancel();
        ReleaseRead(filesystem, directory);
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.False(viewModel.IsLoading);
        Assert.Null(viewModel.ErrorMessage);
        Assert.Null(viewModel.SelectedFile);
        Assert.True(viewModel.IsOpen);
        Assert.Equal("project-1", viewModel.ProjectId);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CurrentFileOrDirectoryReadFailureIsStillReported(bool directory)
    {
        var filesystem = DelayedReadFilesystem(directory, failRead: true);
        using var viewModel = new ProjectFilesViewModel(filesystem, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        var pending = StartDelayedRead(viewModel, directory);
        await ReadStarted(filesystem, directory).WaitAsync(TimeSpan.FromSeconds(2));

        ReleaseRead(filesystem, directory);
        await pending.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.Equal("old read failed", viewModel.ErrorMessage);
        Assert.False(viewModel.IsLoading);
        Assert.True(viewModel.IsOpen);
        Assert.Equal("project-1", viewModel.ProjectId);
    }

    private static FilesystemDouble DelayedReadFilesystem(bool directory, bool failRead) => new()
    {
        DelayTreePath = directory ? "root/Slow" : null,
        TreeListingFailure = failRead ? new IOException("old read failed") : null,
        DelayReadPath = directory ? null : "root/Slow",
        ReadFailure = failRead ? new IOException("old read failed") : null,
    };

    private static Task StartDelayedRead(ProjectFilesViewModel viewModel, bool directory) =>
        viewModel.OpenEntryCommand.ExecuteAsync(new ProjectFileEntry(
            "Slow", "root/Slow", "Slow", directory, true, null, null));

    private static Task ReadStarted(FilesystemDouble filesystem, bool directory) =>
        directory ? filesystem.TreeListingStarted.Task : filesystem.ReadStarted.Task;

    private static void ReleaseRead(FilesystemDouble filesystem, bool directory) =>
        (directory ? filesystem.TreeListingRelease : filesystem.ReadRelease).TrySetResult();

    private sealed class PausingUiDispatcher : IUiDispatcher
    {
        public bool PauseNextDispatch { get; set; }
        public bool PauseNextUncancellableDispatch { get; set; }
        public TaskCompletionSource Paused { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public async Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
        {
            if (PauseNextDispatch || PauseNextUncancellableDispatch && !cancellationToken.CanBeCanceled)
            {
                PauseNextDispatch = false;
                PauseNextUncancellableDispatch = false;
                Paused.TrySetResult();
                await Release.Task;
            }
            cancellationToken.ThrowIfCancellationRequested();
            action();
        }
    }

    private sealed class FilesystemDouble : IProjectFilesystemService
    {
        public IReadOnlyList<ProjectFileEntry>? ListingEntries { get; init; }

        public (string Path, string Content)? LastWrite { get; private set; }

        public bool DelayWrites { get; init; }

        public Exception? WriteFailure { get; init; }

        public TaskCompletionSource WriteStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource WriteRelease { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public (string Parent, string Name)? LastCreatedFile { get; private set; }

        public (string Parent, string Name)? LastCreatedDirectory { get; private set; }

        public (string Source, string Parent, string? Name, bool Replace)? LastMove { get; private set; }

        public (string Path, bool Recursive)? LastDelete { get; private set; }

        public int ForceRefreshCount { get; private set; }

        public Func<int, CancellationToken, Task>? RefreshHandler { get; init; }

        public Func<int, CancellationToken, Task>? MutationHandler { get; init; }

        public int MutationCount { get; private set; }

        private Task FinishMutationAsync(CancellationToken cancellationToken) =>
            MutationHandler?.Invoke(++MutationCount, cancellationToken) ?? Task.CompletedTask;

        public ProjectFileContent? ReadResult { get; init; }

        public (string Path, ProjectFileExternalOpenMode Mode)? LastExternalOpen { get; private set; }

        public List<string> ReadPaths { get; } = [];

        public int EntrySearchCount { get; private set; }

        public int ContentSearchCount { get; private set; }

        public string? LastEntrySearchPath { get; private set; }

        public string? LastContentSearchPath { get; private set; }

        public int LastContentSearchLimit { get; private set; }

        public Func<string, CancellationToken, Task<IReadOnlyList<ProjectFileEntry>>>? EntrySearchHandler { get; init; }

        public Func<string, CancellationToken, Task<IReadOnlyList<ProjectFileContentMatch>>>? ContentSearchHandler { get; init; }

        public string? DelayTreePath { get; init; }

        public Exception? TreeListingFailure { get; init; }

        public string? DelayReadPath { get; init; }

        public Exception? ReadFailure { get; init; }

        public TaskCompletionSource ReadStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource ReadRelease { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource TreeListingStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource TreeListingRelease { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public async Task<ProjectDirectoryListing> ListEntriesAsync(
            string path,
            bool forceRefresh = false,
            CancellationToken cancellationToken = default)
        {
            if (!forceRefresh && string.Equals(path, DelayTreePath, StringComparison.Ordinal))
            {
                TreeListingStarted.TrySetResult();
                // Deliberately ignore cancellation to model a connector response
                // that was already in flight when the project changed.
                await TreeListingRelease.Task;
                if (TreeListingFailure is not null) throw TreeListingFailure;
            }

            if (forceRefresh)
            {
                ForceRefreshCount++;
                if (RefreshHandler is not null) await RefreshHandler(ForceRefreshCount, cancellationToken);
            }

            return new ProjectDirectoryListing(
                path,
                null,
                true,
                ListingEntries ??
                [
                    new ProjectFileEntry("App.cs", $"{path}/App.cs", "App.cs", false, true, 12, null),
                    new ProjectFileEntry("Sources", $"{path}/Sources", "Sources", true, true, null, null),
                ],
                false);
        }

        public Task<IReadOnlyList<ProjectFileEntry>> SearchEntriesAsync(
            string path,
            string query,
            int limit = 100,
            CancellationToken cancellationToken = default)
        {
            EntrySearchCount++;
            LastEntrySearchPath = path;
            return EntrySearchHandler?.Invoke(query, cancellationToken)
                ?? Task.FromResult<IReadOnlyList<ProjectFileEntry>>(Array.Empty<ProjectFileEntry>());
        }

        public Task<IReadOnlyList<ProjectFileContentMatch>> SearchContentAsync(
            string path,
            string query,
            int limit = 100,
            CancellationToken cancellationToken = default)
        {
            ContentSearchCount++;
            LastContentSearchPath = path;
            LastContentSearchLimit = limit;
            return ContentSearchHandler?.Invoke(query, cancellationToken)
                ?? Task.FromResult<IReadOnlyList<ProjectFileContentMatch>>(Array.Empty<ProjectFileContentMatch>());
        }

        public async Task<ProjectFileContent> ReadFileAsync(
            string path,
            CancellationToken cancellationToken = default)
        {
            ReadPaths.Add(path);
            if (string.Equals(path, DelayReadPath, StringComparison.Ordinal))
            {
                ReadStarted.TrySetResult();
                // Keep the in-flight connector response alive after cancellation.
                await ReadRelease.Task;
                if (ReadFailure is not null) throw ReadFailure;
            }
            return ReadResult ?? new ProjectFileContent(
                path,
                Path.GetFileName(path),
                Path.GetFileName(path),
                "text/x-csharp",
                false,
                true,
                12,
                null,
                "class App {}");
        }

        public async Task WriteFileAsync(
            string path,
            string content,
            CancellationToken cancellationToken = default)
        {
            LastWrite = (path, content);
            WriteStarted.TrySetResult();
            // Model a response already in flight when the project is changed.
            if (DelayWrites) await WriteRelease.Task;
            if (WriteFailure is not null) throw WriteFailure;
        }

        public Task CreateFileAsync(string parentPath, string name, CancellationToken cancellationToken = default)
        {
            LastCreatedFile = (parentPath, name);
            return FinishMutationAsync(cancellationToken);
        }

        public Task CreateDirectoryAsync(string parentPath, string name, CancellationToken cancellationToken = default)
        {
            LastCreatedDirectory = (parentPath, name);
            return FinishMutationAsync(cancellationToken);
        }

        public Task DeleteEntryAsync(string path, bool recursive, CancellationToken cancellationToken = default)
        {
            LastDelete = (path, recursive);
            return FinishMutationAsync(cancellationToken);
        }

        public async Task<ProjectFileMoveResult> MoveEntryAsync(
            string sourcePath,
            string targetParentPath,
            string? targetName = null,
            bool replaceExisting = false,
            CancellationToken cancellationToken = default)
        {
            LastMove = (sourcePath, targetParentPath, targetName, replaceExisting);
            await FinishMutationAsync(cancellationToken);
            return new ProjectFileMoveResult(
                sourcePath,
                $"{targetParentPath}/{targetName}",
                targetName,
                targetName,
                false,
                true);
        }

        public Task OpenExternallyAsync(
            string path,
            ProjectFileExternalOpenMode mode,
            CancellationToken cancellationToken = default)
        {
            LastExternalOpen = (path, mode);
            return Task.CompletedTask;
        }
    }
}
