using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class ProjectGitViewModelTests
{
    [Fact]
    public async Task OpenLoadsRepositoryStateAndDiff()
    {
        var service = new GitServiceDouble();
        using var viewModel = new ProjectGitViewModel(service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(Project("root-1"));
        var change = Assert.Single(viewModel.Changes);
        await viewModel.OpenDiffCommand.ExecuteAsync(new ProjectGitDiffRequest(change, false));

        Assert.True(viewModel.IsRepository);
        Assert.Equal("main", viewModel.CurrentBranch);
        Assert.Equal("origin/main", viewModel.Upstream);
        Assert.Equal("diff for README.md", viewModel.SelectedDiff?.Content);
        Assert.Equal("已与 origin/main 同步", viewModel.SyncLabel);
    }

    [Fact]
    public async Task StageAndCommitRefreshAuthoritativeSnapshot()
    {
        var service = new GitServiceDouble();
        using var viewModel = new ProjectGitViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Project("root-1"));

        await viewModel.StageChangeCommand.ExecuteAsync(viewModel.Changes[0]);
        Assert.True(viewModel.HasStagedChanges);
        viewModel.CommitMessage = "docs: update readme";
        await viewModel.CommitCommand.ExecuteAsync(null);

        Assert.Equal("docs: update readme", service.LastCommitMessage);
        Assert.Empty(viewModel.Changes);
        Assert.Equal(string.Empty, viewModel.CommitMessage);
        Assert.Equal("提交已创建。", viewModel.ActionMessage);
    }

    [Fact]
    public async Task CancelStopsCurrentOperationAndLeavesSessionUsable()
    {
        var service = new DelayedGitServiceDouble();
        using var viewModel = new ProjectGitViewModel(service, new ImmediateUiDispatcher());

        var opening = viewModel.OpenAsync(Project("root-1"));
        await service.Started.Task.WaitAsync(TimeSpan.FromSeconds(2));
        viewModel.CancelOperationCommand.Execute(null);
        await opening;

        Assert.Equal("Git 操作已取消。", viewModel.ActionMessage);
        Assert.False(viewModel.IsLoading);
        Assert.False(viewModel.CanCancel);
    }

    [Fact]
    public async Task OldProjectResultCannotOverwriteNewProject()
    {
        var service = new SwitchingGitServiceDouble();
        using var viewModel = new ProjectGitViewModel(service, new ImmediateUiDispatcher());

        var first = viewModel.OpenAsync(Project("root-1"));
        await service.FirstStarted.Task.WaitAsync(TimeSpan.FromSeconds(2));
        await viewModel.OpenAsync(Project("root-2"));
        service.ReleaseFirst();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => first);

        Assert.Equal("root-2", viewModel.ProjectRoot);
        Assert.Equal("second", viewModel.CurrentBranch);
    }

    private static WorkspaceProject Project(string root) => new(
        root,
        root,
        root,
        root,
        null);

    private class GitServiceDouble : IProjectGitService
    {
        private bool _staged;
        private bool _committed;

        public string? LastCommitMessage { get; private set; }

        public virtual Task<ProjectGitSnapshot> SnapshotAsync(
            string projectRoot,
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var changes = _committed
                ? Array.Empty<ProjectGitChange>()
                : new[]
                {
                    new ProjectGitChange(
                        "README.md",
                        null,
                        Path.Combine(Path.GetTempPath(), "README.md"),
                        _staged ? "M" : " ",
                        _staged ? " " : "M",
                        ProjectGitChangeKind.Modified),
                };
            return Task.FromResult(Snapshot("main", changes));
        }

        public Task InitializeRepositoryAsync(string projectRoot, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task<ProjectGitDiff> DiffAsync(
            string projectRoot,
            ProjectGitChange change,
            bool staged,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new ProjectGitDiff(change.Path, staged, $"diff for {change.Path}"));

        public Task StageAsync(
            string projectRoot,
            IReadOnlyList<string> paths,
            CancellationToken cancellationToken = default)
        {
            _staged = true;
            return Task.CompletedTask;
        }

        public Task UnstageAsync(
            string projectRoot,
            IReadOnlyList<string> paths,
            CancellationToken cancellationToken = default)
        {
            _staged = false;
            return Task.CompletedTask;
        }

        public Task CommitAsync(
            string projectRoot,
            string message,
            CancellationToken cancellationToken = default)
        {
            LastCommitMessage = message;
            _committed = true;
            return Task.CompletedTask;
        }

        public Task SwitchBranchAsync(string projectRoot, string branch, CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task CreateBranchAsync(string projectRoot, string name, bool switchToBranch, CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task MergeBranchAsync(string projectRoot, string branch, CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task SaveRemoteAsync(string projectRoot, string? originalName, string name, string url, CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RemoveRemoteAsync(string projectRoot, string name, CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task PullAsync(string projectRoot, CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task PushAsync(string projectRoot, CancellationToken cancellationToken = default) => Task.CompletedTask;

        protected static ProjectGitSnapshot Snapshot(
            string branch,
            IReadOnlyList<ProjectGitChange>? changes = null) => new(
            true,
            Path.GetTempPath(),
            branch,
            null,
            "origin/main",
            0,
            0,
            true,
            changes ?? Array.Empty<ProjectGitChange>(),
            [new ProjectGitBranch(branch, true, "origin/main")],
            [new ProjectGitCommit("a", "a", [], "Test", DateTimeOffset.UtcNow, [], "commit")],
            [new ProjectGitRemote("origin", "https://example.com/repo.git")]);
    }

    private sealed class DelayedGitServiceDouble : GitServiceDouble
    {
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public override async Task<ProjectGitSnapshot> SnapshotAsync(
            string projectRoot,
            CancellationToken cancellationToken = default)
        {
            Started.TrySetResult();
            await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
            throw new InvalidOperationException("unreachable");
        }
    }

    private sealed class SwitchingGitServiceDouble : GitServiceDouble
    {
        private readonly TaskCompletionSource _release = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource FirstStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public void ReleaseFirst() => _release.TrySetResult();

        public override async Task<ProjectGitSnapshot> SnapshotAsync(
            string projectRoot,
            CancellationToken cancellationToken = default)
        {
            if (projectRoot == "root-2")
            {
                return Snapshot("second");
            }

            FirstStarted.TrySetResult();
            await _release.Task.WaitAsync(cancellationToken);
            return Snapshot("first");
        }
    }
}
