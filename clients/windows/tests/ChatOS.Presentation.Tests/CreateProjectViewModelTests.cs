using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Projects;

namespace ChatOS.Presentation.Tests;

public sealed class CreateProjectViewModelTests
{
    private static readonly LocalConnectorStatus Status = new(
        true,
        "Connected",
        "owner",
        "device",
        "PC",
        "https://gateway.example",
        null,
        null,
        null,
        [new LocalConnectorWorkspaceStatus("ws", "工作区", @"C:\workspace")]);

    [Fact]
    public void ProjectTypeCatalogMatchesMacOsStableContract()
    {
        Assert.Equal(27, LocalProjectTypeCatalog.All.Count);
        Assert.Equal(
            LocalProjectTypeCatalog.All.Count,
            LocalProjectTypeCatalog.All.Select(value => value.Key).Distinct(StringComparer.Ordinal).Count());
        Assert.Contains(
            LocalProjectTypeCatalog.All,
            value => value.Key == LocalProjectTypeCatalog.LegacyProjectTypeKey);
    }

    [Fact]
    public async Task InitialDirectoryUsesWorkspaceAndSuggestsItsAlias()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);

        await viewModel.LoadInitialDirectoryAsync();

        Assert.Equal(@"C:\workspace", viewModel.CurrentPath);
        Assert.Null(viewModel.CurrentRelativePath);
        Assert.Equal("工作区", viewModel.ProjectName);
        Assert.True(viewModel.CanCreate);
        Assert.Equal(["alpha", "zeta"], viewModel.Entries.Select(value => value.Name));
    }

    [Fact]
    public async Task OpeningDirectoryUpdatesRelativeRootAndSuggestedName()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();

        await viewModel.OpenDirectoryAsync(viewModel.Entries[0]);

        Assert.Equal("alpha", viewModel.CurrentRelativePath);
        Assert.Equal("alpha", viewModel.ProjectName);
        Assert.Equal("工作区/alpha", viewModel.DisplayedLocation);
    }

    [Fact]
    public async Task WindowsDriveRootNavigationKeepsTheSelectedSubdirectory()
    {
        var status = Status with
        {
            Workspaces = [new LocalConnectorWorkspaceStatus("drive", "本机文件系统", @"C:\")],
        };
        var filesystem = new FilesystemDouble { RootPath = @"C:\" };
        var viewModel = new CreateProjectViewModel(status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        await viewModel.OpenWorkspaceRootAsync();

        await viewModel.OpenDirectoryAsync(viewModel.Entries[0]);

        Assert.Equal("alpha", viewModel.CurrentRelativePath);
        Assert.Equal("alpha", viewModel.ProjectName);
        Assert.Equal("本机文件系统/alpha", viewModel.DisplayedLocation);
        LocalProjectDraft? savedDraft = null;
        var saved = await viewModel.SaveAsync((draft, _, _) =>
        {
            savedDraft = draft;
            return Task.CompletedTask;
        });
        Assert.True(saved);
        Assert.Equal("alpha", savedDraft?.RelativeRoot);
    }

    [Fact]
    public async Task AbsolutePathNavigationSelectsAnExistingDirectoryInsideWorkspace()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();

        await viewModel.OpenAbsolutePathAsync(@"C:\workspace\alpha");

        Assert.Equal(@"C:\workspace\alpha", viewModel.CurrentPath);
        Assert.Equal("alpha", viewModel.CurrentRelativePath);
        Assert.Equal("工作区/alpha", viewModel.DisplayedLocation);
        Assert.True(viewModel.CanGoToParent);
    }

    [Theory]
    [InlineData("alpha")]
    [InlineData(@"D:\outside")]
    public async Task AbsolutePathNavigationRejectsRelativeOrOutsideWorkspace(string path)
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        var before = viewModel.CurrentPath;
        var calls = filesystem.ListCallCount;

        await viewModel.OpenAbsolutePathAsync(path);

        Assert.Equal(before, viewModel.CurrentPath);
        Assert.Equal(calls, filesystem.ListCallCount);
        Assert.Contains("绝对目录路径", viewModel.ErrorMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task ParentNavigationUsesPathBoundaryInsteadOfNullableRelativeHint()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        await viewModel.OpenAbsolutePathAsync(@"C:\workspace\alpha");

        await viewModel.GoToParentDirectoryAsync();

        Assert.Equal(@"C:\workspace", viewModel.CurrentPath);
        Assert.Null(viewModel.CurrentRelativePath);
        Assert.False(viewModel.CanGoToParent);
    }

    [Fact]
    public async Task UserEditedNameIsNotReplacedWhileBrowsing()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        viewModel.UpdateProjectName(" My Project ");

        await viewModel.OpenDirectoryAsync(viewModel.Entries[0]);

        Assert.Equal(" My Project ", viewModel.ProjectName);
    }

    [Fact]
    public async Task HiddenDirectoriesCanBeRevealed()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);

        await viewModel.LoadInitialDirectoryAsync();
        Assert.DoesNotContain(viewModel.Entries, value => value.Name == ".cache");

        viewModel.ToggleHiddenDirectories();

        Assert.Contains(viewModel.Entries, value => value.Name == ".cache");
    }

    [Fact]
    public async Task NewDirectoryIsCreatedInCurrentDirectoryAndListIsRefreshed()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();

        await viewModel.CreateDirectoryAsync("docs");

        Assert.Equal((@"C:\workspace", "docs"), filesystem.CreatedDirectory);
        Assert.True(filesystem.ForceRefreshCount > 0);
    }

    [Theory]
    [InlineData("")]
    [InlineData("../outside")]
    [InlineData("..\\outside")]
    public async Task InvalidDirectoryNamePublishesValidationErrorWithoutTouchingFilesystem(string name)
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();

        await viewModel.CreateDirectoryAsync(name);

        Assert.Contains("文件夹名称", viewModel.ErrorMessage, StringComparison.Ordinal);
        Assert.Null(filesystem.CreatedDirectory);
        Assert.Equal(0, filesystem.CreateDirectoryCallCount);
    }

    [Fact]
    public async Task SaveBuildsValidatedDraftWithDescriptionAndSelectedDirectory()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        await viewModel.OpenDirectoryAsync(viewModel.Entries[0]);
        viewModel.UpdateProjectName(" Local ");
        viewModel.ProjectDescription = " Test project ";
        LocalProjectDraft? savedDraft = null;
        string? savedRoot = null;

        var result = await viewModel.SaveAsync((draft, root, _) =>
        {
            savedDraft = draft;
            savedRoot = root;
            return Task.CompletedTask;
        });

        Assert.True(result);
        Assert.Equal(new LocalProjectDraft("Local", "ws", "alpha", "Test project"), savedDraft);
        Assert.Equal(@"C:\workspace", savedRoot);
    }

    [Fact]
    public async Task SavePersistsSelectedProjectType()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        viewModel.SelectedProjectType = viewModel.ProjectTypes.Single(value => value.Key == "desktop_application");
        LocalProjectDraft? savedDraft = null;

        var result = await viewModel.SaveAsync((draft, _, _) =>
        {
            savedDraft = draft;
            return Task.CompletedTask;
        });

        Assert.True(result);
        Assert.Equal("desktop_application", savedDraft?.ProjectTypeKey);
    }

    [Fact]
    public async Task FailedInitialDirectoryLoadCannotCreateProject()
    {
        var filesystem = new FilesystemDouble { FailList = true };
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        var createCalls = 0;

        await viewModel.LoadInitialDirectoryAsync();
        viewModel.UpdateProjectName("Local");
        var result = await viewModel.SaveAsync((_, _, _) =>
        {
            createCalls++;
            return Task.CompletedTask;
        });

        Assert.False(viewModel.CanCreate);
        Assert.False(result);
        Assert.Equal(0, createCalls);
        Assert.NotNull(viewModel.ErrorMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OlderDirectoryResponseCannotChangeNewerSelectionOrError(bool fail)
    {
        var release = new TaskCompletionSource<ProjectDirectoryListing>(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        var alpha = viewModel.Entries[0];
        var zeta = viewModel.Entries[1];
        filesystem.ListHandler = (path, _) => path == alpha.Path ? release.Task
            : Task.FromResult(Listing(path));
        var older = viewModel.OpenDirectoryAsync(alpha);
        await viewModel.OpenDirectoryAsync(zeta);

        if (fail) release.SetException(new IOException("old directory failed"));
        else release.SetResult(Listing(alpha.Path));
        await older;

        Assert.Equal(zeta.Path, viewModel.CurrentPath);
        Assert.Equal("zeta", viewModel.CurrentRelativePath);
        Assert.Equal("zeta", viewModel.ProjectName);
        Assert.Null(viewModel.ErrorMessage);
        Assert.True(viewModel.CanCreate);
    }

    [Fact]
    public async Task OlderDirectoryFinishingDoesNotEndTheNewerLoadingState()
    {
        var first = new TaskCompletionSource<ProjectDirectoryListing>(TaskCreationOptions.RunContinuationsAsynchronously);
        var second = new TaskCompletionSource<ProjectDirectoryListing>(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        var alpha = viewModel.Entries[0];
        var zeta = viewModel.Entries[1];
        filesystem.ListHandler = (path, _) => path == alpha.Path ? first.Task : second.Task;
        var older = viewModel.OpenDirectoryAsync(alpha);
        var newer = viewModel.OpenDirectoryAsync(zeta);
        first.SetResult(Listing(alpha.Path));
        await older;

        Assert.True(viewModel.IsLoadingDirectory);
        Assert.False(viewModel.CanCreate);
        second.SetResult(Listing(zeta.Path));
        await newer;
    }

    [Fact]
    public async Task SavingWhileDirectoryIsLoadingDoesNotCreateAtThePreviousPath()
    {
        var release = new TaskCompletionSource<ProjectDirectoryListing>(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        var alpha = viewModel.Entries[0];
        filesystem.ListHandler = (_, _) => release.Task;
        var loading = viewModel.OpenDirectoryAsync(alpha);
        var calls = 0;

        var saved = await viewModel.SaveAsync((_, _, _) => { calls++; return Task.CompletedTask; });

        Assert.False(saved);
        Assert.Equal(0, calls);
        release.SetResult(Listing(alpha.Path));
        await loading;
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task SwitchingWorkspacesCancelsAndDiscardsTheOldDirectory(bool fail)
    {
        var release = new TaskCompletionSource<ProjectDirectoryListing>(TaskCreationOptions.RunContinuationsAsynchronously);
        var firstWorkspace = Status.Workspaces[0];
        var secondWorkspace = new LocalConnectorWorkspaceStatus("second", "Second", @"D:\second");
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status with { Workspaces = [firstWorkspace, secondWorkspace] }, filesystem);
        CancellationToken oldToken = default;
        filesystem.ListHandler = (path, token) =>
        {
            if (path == firstWorkspace.AbsoluteRoot) { oldToken = token; return release.Task; }
            return Task.FromResult(Listing(path));
        };
        var older = viewModel.LoadInitialDirectoryAsync();

        await viewModel.SelectWorkspaceAsync(secondWorkspace);
        Assert.True(oldToken.IsCancellationRequested);
        if (fail) release.SetException(new IOException("old workspace denied"));
        else release.SetResult(Listing(firstWorkspace.AbsoluteRoot));
        await older;

        Assert.Same(secondWorkspace, viewModel.SelectedWorkspace);
        Assert.Equal(secondWorkspace.AbsoluteRoot, viewModel.CurrentPath);
        Assert.Equal("Second", viewModel.ProjectName);
        Assert.Null(viewModel.CurrentRelativePath);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task ClearingWorkspaceSelectionDiscardsTheInFlightListing()
    {
        var release = new TaskCompletionSource<ProjectDirectoryListing>(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble { ListHandler = (_, _) => release.Task };
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        var loading = viewModel.LoadInitialDirectoryAsync();
        await viewModel.SelectWorkspaceAsync(null);
        release.SetResult(Listing(@"C:\workspace"));
        await loading;

        Assert.Null(viewModel.SelectedWorkspace);
        Assert.Equal(string.Empty, viewModel.CurrentPath);
        Assert.Empty(viewModel.Entries);
        Assert.False(viewModel.IsLoadingDirectory);
        Assert.False(viewModel.CanCreate);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CurrentDirectoryCancellationKeepsPreviousPathWithoutAnError(bool fail)
    {
        var release = new TaskCompletionSource<ProjectDirectoryListing>(TaskCreationOptions.RunContinuationsAsynchronously);
        using var cancellation = new CancellationTokenSource();
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        var alpha = viewModel.Entries[0];
        filesystem.ListHandler = (_, _) => release.Task;
        var loading = viewModel.OpenDirectoryAsync(alpha, cancellation.Token);
        cancellation.Cancel();

        if (fail) release.SetException(new OperationCanceledException(cancellation.Token));
        else release.SetResult(Listing(alpha.Path));
        await loading;

        Assert.Equal(@"C:\workspace", viewModel.CurrentPath);
        Assert.Null(viewModel.CurrentRelativePath);
        Assert.Equal(2, viewModel.Entries.Count);
        Assert.False(viewModel.IsLoadingDirectory);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task CurrentDirectoryFailureIsVisibleAndCanBeRetried()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        filesystem.ListHandler = (_, _) => Task.FromException<ProjectDirectoryListing>(new IOException("current directory failed"));

        await viewModel.RefreshDirectoryAsync();
        Assert.Equal("current directory failed", viewModel.ErrorMessage);
        Assert.False(viewModel.IsLoadingDirectory);
        Assert.False(viewModel.HasUsableDirectoryListing);
        Assert.False(viewModel.CanCreate);
        Assert.Contains("尚未验证", viewModel.DirectoryListingSummary, StringComparison.Ordinal);
        var createCalls = 0;
        Assert.False(await viewModel.SaveAsync((_, _, _) => { createCalls++; return Task.CompletedTask; }));
        Assert.Equal(0, createCalls);
        Assert.Contains("尚未验证", viewModel.ErrorMessage, StringComparison.Ordinal);
        filesystem.ListHandler = null;
        await viewModel.RefreshDirectoryAsync();

        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal(2, viewModel.Entries.Count);
        Assert.True(viewModel.HasUsableDirectoryListing);
        Assert.True(viewModel.IsDirectoryWritable);
        Assert.True(viewModel.CanCreate);
    }

    [Fact]
    public async Task SuccessfulDirectoryListingPublishesExactEntryCounts()
    {
        var filesystem = new FilesystemDouble
        {
            ListHandler = (path, _) => Task.FromResult(new ProjectDirectoryListing(
                path,
                null,
                true,
                [
                    new ProjectFileEntry("folder", Path.Combine(path, "folder"), "/folder", true, true, null, null),
                    new ProjectFileEntry("file.txt", Path.Combine(path, "file.txt"), "/file.txt", false, true, 4, null),
                ],
                false)),
        };
        var viewModel = new CreateProjectViewModel(Status, filesystem);

        await viewModel.LoadInitialDirectoryAsync();

        Assert.True(viewModel.HasUsableDirectoryListing);
        Assert.Equal(2, viewModel.DirectoryEntryCount);
        Assert.Equal(1, viewModel.DirectoryFolderCount);
        Assert.Equal(1, viewModel.DirectoryFileCount);
        Assert.Contains("目录项 2", viewModel.DirectoryListingSummary, StringComparison.Ordinal);
    }

    [Fact]
    public async Task SaveForceRefreshesAndPublishesTheLatestDirectoryProofBeforeCreate()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        filesystem.ListHandler = (path, _) => Task.FromResult(new ProjectDirectoryListing(
            path,
            null,
            true,
            [new ProjectFileEntry("late.txt", Path.Combine(path, "late.txt"), "/late.txt", false, true, 4, null)],
            false));
        var createCalls = 0;

        var saved = await viewModel.SaveAsync((_, _, _) =>
        {
            createCalls++;
            Assert.Equal(1, viewModel.DirectoryEntryCount);
            Assert.Equal(0, viewModel.DirectoryFolderCount);
            Assert.Equal(1, viewModel.DirectoryFileCount);
            return Task.CompletedTask;
        });

        Assert.True(saved);
        Assert.Equal(1, createCalls);
        Assert.Equal(1, filesystem.ForceRefreshCount);
        Assert.True(viewModel.HasUsableDirectoryListing);
    }

    [Fact]
    public async Task SaveRefreshIdentityMismatchFailsClosedBeforeCreate()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        filesystem.ListHandler = (_, _) => Task.FromResult(new ProjectDirectoryListing(
            @"C:\workspace\other",
            @"C:\workspace",
            true,
            [],
            false));
        var createCalls = 0;

        var saved = await viewModel.SaveAsync((_, _, _) =>
        {
            createCalls++;
            return Task.CompletedTask;
        });

        Assert.False(saved);
        Assert.Equal(0, createCalls);
        Assert.False(viewModel.HasUsableDirectoryListing);
        Assert.False(viewModel.CanCreate);
        Assert.Contains("不一致", viewModel.ErrorMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task SaveRefreshFailureFailsClosedBeforeCreateAndCanBeRetried()
    {
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        filesystem.ListHandler = (_, _) => Task.FromException<ProjectDirectoryListing>(new IOException("submit refresh failed"));
        var createCalls = 0;

        Assert.False(await viewModel.SaveAsync((_, _, _) =>
        {
            createCalls++;
            return Task.CompletedTask;
        }));
        Assert.Equal(0, createCalls);
        Assert.False(viewModel.HasUsableDirectoryListing);
        Assert.Contains("submit refresh failed", viewModel.ErrorMessage, StringComparison.Ordinal);

        filesystem.ListHandler = null;
        await viewModel.RefreshDirectoryAsync();
        Assert.True(await viewModel.SaveAsync((_, _, _) => Task.CompletedTask));
    }

    [Fact]
    public async Task ClosingDialogCancelsSubmitRefreshBeforeCreate()
    {
        var release = new TaskCompletionSource<ProjectDirectoryListing>(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        CancellationToken refreshToken = default;
        filesystem.ListHandler = (_, token) =>
        {
            refreshToken = token;
            return release.Task;
        };
        var createCalls = 0;
        var saving = viewModel.SaveAsync((_, _, _) =>
        {
            createCalls++;
            return Task.CompletedTask;
        });

        viewModel.Dispose();
        Assert.True(refreshToken.IsCancellationRequested);
        release.SetException(new OperationCanceledException(refreshToken));

        Assert.False(await saving);
        Assert.Equal(0, createCalls);
        Assert.False(viewModel.CanCreate);
    }

    [Theory]
    [InlineData(@"C:\workspace\other", @"C:\workspace")]
    [InlineData(@"D:\outside", @"D:\")]
    public async Task MismatchedDirectoryResponseCannotAuthorizeProjectCreation(
        string returnedPath,
        string? returnedParent)
    {
        var filesystem = new FilesystemDouble
        {
            ListHandler = (_, _) => Task.FromResult(new ProjectDirectoryListing(
                returnedPath,
                returnedParent,
                true,
                [],
                false)),
        };
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        var createCalls = 0;

        await viewModel.LoadInitialDirectoryAsync();
        Assert.Contains("不一致", viewModel.ErrorMessage, StringComparison.Ordinal);
        viewModel.UpdateProjectName("Local");
        var saved = await viewModel.SaveAsync((_, _, _) =>
        {
            createCalls++;
            return Task.CompletedTask;
        });

        Assert.False(viewModel.HasUsableDirectoryListing);
        Assert.False(viewModel.CanCreate);
        Assert.False(saved);
        Assert.Equal(0, createCalls);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CreatingFolderInAnOldDirectoryCannotReloadTheNewSelection(bool fail)
    {
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble { CreateDirectoryHandler = _ => release.Task };
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        var alpha = viewModel.Entries[0];
        var creating = viewModel.CreateDirectoryAsync("docs");
        await viewModel.OpenDirectoryAsync(alpha);
        var refreshCount = filesystem.ForceRefreshCount;

        if (fail) release.SetException(new IOException("old folder failed"));
        else release.SetResult();
        await creating;

        Assert.Equal(alpha.Path, viewModel.CurrentPath);
        Assert.Equal("alpha", viewModel.CurrentRelativePath);
        Assert.Equal(refreshCount, filesystem.ForceRefreshCount);
        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.IsLoadingDirectory);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldFolderRefreshCannotReplaceNewerDirectoryOrError(bool fail)
    {
        var release = new TaskCompletionSource<ProjectDirectoryListing>(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        var alpha = viewModel.Entries[0];
        filesystem.ListHandler = (path, _) => path == @"C:\workspace" ? release.Task : Task.FromResult(Listing(path));
        var creating = viewModel.CreateDirectoryAsync("docs");
        await viewModel.OpenDirectoryAsync(alpha);

        if (fail) release.SetException(new IOException("old folder refresh failed"));
        else release.SetResult(Listing(@"C:\workspace"));
        await creating;

        Assert.Equal(alpha.Path, viewModel.CurrentPath);
        Assert.Equal("alpha", viewModel.CurrentRelativePath);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task FolderCreationBlocksDuplicateFolderAndProjectCreation()
    {
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var filesystem = new FilesystemDouble { CreateDirectoryHandler = _ => release.Task };
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        var first = viewModel.CreateDirectoryAsync("docs");

        await viewModel.CreateDirectoryAsync("duplicate");
        var saved = await viewModel.SaveAsync((_, _, _) => throw new InvalidOperationException("must not create"));

        Assert.False(saved);
        Assert.Equal(1, filesystem.CreateDirectoryCallCount);
        Assert.True(viewModel.IsLoadingDirectory);
        release.SetResult();
        await first;
    }

    [Fact]
    public async Task SavingFreezesDirectoryAndWorkspaceUntilTheOriginalDraftCompletes()
    {
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var secondWorkspace = new LocalConnectorWorkspaceStatus("second", "Second", @"D:\second");
        var filesystem = new FilesystemDouble();
        var viewModel = new CreateProjectViewModel(Status with { Workspaces = [Status.Workspaces[0], secondWorkspace] }, filesystem);
        await viewModel.LoadInitialDirectoryAsync();
        var alpha = viewModel.Entries[0];
        LocalProjectDraft? submitted = null;
        var saving = viewModel.SaveAsync((draft, _, _) => { submitted = draft; return release.Task; });

        await viewModel.SelectWorkspaceAsync(secondWorkspace);
        await viewModel.OpenDirectoryAsync(alpha);
        await viewModel.RefreshDirectoryAsync();
        await viewModel.CreateDirectoryAsync("docs");

        Assert.Equal(@"C:\workspace", viewModel.CurrentPath);
        Assert.Same(Status.Workspaces[0], viewModel.SelectedWorkspace);
        Assert.Equal(string.Empty, submitted?.RelativeRoot);
        Assert.Null(filesystem.CreatedDirectory);
        Assert.True(viewModel.IsSaving);
        release.SetResult();
        Assert.True(await saving);
        Assert.False(viewModel.IsSaving);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ClosingDialogCancelsDirectoryAndIgnoresLateResults(bool fail)
    {
        var release = new TaskCompletionSource<ProjectDirectoryListing>(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken token = default;
        var filesystem = new FilesystemDouble { ListHandler = (_, ct) => { token = ct; return release.Task; } };
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        var loading = viewModel.LoadInitialDirectoryAsync();

        viewModel.Dispose();
        Assert.True(token.IsCancellationRequested);
        if (fail) release.SetException(new IOException("closed dialog failed"));
        else release.SetResult(Listing(@"C:\workspace"));
        await loading;

        Assert.Equal(string.Empty, viewModel.CurrentPath);
        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.IsLoadingDirectory);
        Assert.False(viewModel.CanCreate);
        Assert.False(await viewModel.SaveAsync((_, _, _) => throw new InvalidOperationException("closed")));
    }

    [Fact]
    public async Task ReadOnlyDirectoryStillAllowsRegisteringAnExistingProject()
    {
        var filesystem = new FilesystemDouble
        {
            ListHandler = (path, _) => Task.FromResult(Listing(path) with { IsWritable = false }),
        };
        var viewModel = new CreateProjectViewModel(Status, filesystem);
        await viewModel.LoadInitialDirectoryAsync();

        Assert.True(viewModel.CanCreate);
        Assert.True(await viewModel.SaveAsync((_, _, _) => Task.CompletedTask));
        await viewModel.CreateDirectoryAsync("not writable");
        Assert.Null(filesystem.CreatedDirectory);
    }

    [Fact]
    public async Task FailedSavePreservesDraftAndAllowsRetry()
    {
        var viewModel = new CreateProjectViewModel(Status, new FilesystemDouble());
        await viewModel.LoadInitialDirectoryAsync();
        viewModel.UpdateProjectName("My project");
        viewModel.ProjectDescription = "My description";

        Assert.False(await viewModel.SaveAsync((_, _, _) => Task.FromException(new IOException("save failed"))));
        Assert.Equal("save failed", viewModel.ErrorMessage);
        Assert.Equal("My project", viewModel.ProjectName);
        Assert.Equal("My description", viewModel.ProjectDescription);
        Assert.True(viewModel.CanCreate);
        Assert.True(await viewModel.SaveAsync((_, _, _) => Task.CompletedTask));
        Assert.Null(viewModel.ErrorMessage);
    }

    private static ProjectDirectoryListing Listing(string path) => new(path, @"C:\workspace", true, [], false);

    private sealed class FilesystemDouble : IProjectFilesystemService
    {
        public string RootPath { get; init; } = @"C:\workspace";

        public bool FailList { get; init; }

        public (string Parent, string Name)? CreatedDirectory { get; private set; }

        public int ForceRefreshCount { get; private set; }
        public int ListCallCount { get; private set; }
        public int CreateDirectoryCallCount { get; private set; }

        public Func<string, CancellationToken, Task<ProjectDirectoryListing>>? ListHandler { get; set; }
        public Func<CancellationToken, Task>? CreateDirectoryHandler { get; set; }

        public Task<ProjectDirectoryListing> ListEntriesAsync(
            string path,
            bool forceRefresh = false,
            CancellationToken cancellationToken = default)
        {
            ListCallCount++;
            if (forceRefresh) ForceRefreshCount++;
            if (ListHandler is not null) return ListHandler(path, cancellationToken);
            if (FailList) throw new UnauthorizedAccessException("Directory denied.");
            var normalized = Path.TrimEndingDirectorySeparator(Path.GetFullPath(path));
            var normalizedRoot = Path.TrimEndingDirectorySeparator(Path.GetFullPath(RootPath));
            var atRoot = string.Equals(normalized, normalizedRoot, StringComparison.OrdinalIgnoreCase);
            IReadOnlyList<ProjectFileEntry> entries = atRoot
                ?
                [
                    new("zeta", Path.Combine(normalizedRoot, "zeta"), "/zeta", true, true, null, null),
                    new("notes.txt", Path.Combine(normalizedRoot, "notes.txt"), "/notes.txt", false, true, 10, null),
                    new(".cache", Path.Combine(normalizedRoot, ".cache"), "/.cache", true, true, null, null),
                    new("alpha", Path.Combine(normalizedRoot, "alpha"), "/alpha", true, true, null, null),
                ]
                : [];
            return Task.FromResult(new ProjectDirectoryListing(
                normalized,
                atRoot ? null : normalizedRoot,
                true,
                entries,
                false));
        }

        public Task<IReadOnlyList<ProjectFileEntry>> SearchEntriesAsync(
            string path,
            string query,
            int limit = 100,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ProjectFileEntry>>([]);

        public Task<IReadOnlyList<ProjectFileContentMatch>> SearchContentAsync(
            string path,
            string query,
            int limit = 100,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ProjectFileContentMatch>>([]);

        public Task<ProjectFileContent> ReadFileAsync(
            string path,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task WriteFileAsync(
            string path,
            string content,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task CreateFileAsync(
            string parentPath,
            string name,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task CreateDirectoryAsync(
            string parentPath,
            string name,
            CancellationToken cancellationToken = default)
        {
            CreatedDirectory = (parentPath, name);
            CreateDirectoryCallCount++;
            return CreateDirectoryHandler?.Invoke(cancellationToken) ?? Task.CompletedTask;
        }

        public Task DeleteEntryAsync(
            string path,
            bool recursive,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task<ProjectFileMoveResult> MoveEntryAsync(
            string sourcePath,
            string targetParentPath,
            string? targetName = null,
            bool replaceExisting = false,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task OpenExternallyAsync(
            string path,
            ProjectFileExternalOpenMode mode,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
    }
}
