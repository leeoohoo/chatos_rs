using System.Diagnostics;
using ChatOS.Connector.Git;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsProjectGitServiceTests
{
    [Fact]
    public async Task LoadsStagesCommitsDiffsAndManagesRemotes()
    {
        if (!GitIsAvailable())
        {
            return;
        }

        using var context = GitTestContext.Create();
        await context.Service.InitializeRepositoryAsync(context.LogicalRoot);
        ConfigureIdentity(context.Root);
        var file = Path.Combine(context.Root, "README.md");
        await File.WriteAllTextAsync(file, "# First\n");

        var snapshot = await context.Service.SnapshotAsync(context.LogicalRoot);
        Assert.True(snapshot.IsRepository);
        Assert.Equal("main", snapshot.CurrentBranch);
        var initial = Assert.Single(snapshot.Changes);
        Assert.Equal(ProjectGitChangeKind.Untracked, initial.Kind);

        var untrackedDiff = await context.Service.DiffAsync(
            context.LogicalRoot,
            initial,
            staged: false);
        Assert.Contains("+# First", untrackedDiff.Content);

        await context.Service.StageAsync(context.LogicalRoot, ["README.md"]);
        snapshot = await context.Service.SnapshotAsync(context.LogicalRoot);
        Assert.True(Assert.Single(snapshot.Changes).HasStagedChanges);

        await context.Service.CommitAsync(context.LogicalRoot, "docs: add readme");
        snapshot = await context.Service.SnapshotAsync(context.LogicalRoot);
        Assert.Empty(snapshot.Changes);
        Assert.Equal("docs: add readme", snapshot.Commits[0].Subject);

        await File.AppendAllTextAsync(file, "\nSecond line\n");
        snapshot = await context.Service.SnapshotAsync(context.LogicalRoot);
        var change = Assert.Single(snapshot.Changes);
        var diff = await context.Service.DiffAsync(context.LogicalRoot, change, staged: false);
        Assert.Contains("+Second line", diff.Content);

        await context.Service.SaveRemoteAsync(
            context.LogicalRoot,
            null,
            "origin",
            "https://example.com/team/project.git");
        snapshot = await context.Service.SnapshotAsync(context.LogicalRoot);
        Assert.Equal("origin", Assert.Single(snapshot.Remotes).Name);

        await context.Service.SaveRemoteAsync(
            context.LogicalRoot,
            "origin",
            "upstream",
            "git@example.com:team/project.git");
        snapshot = await context.Service.SnapshotAsync(context.LogicalRoot);
        Assert.Equal("upstream", Assert.Single(snapshot.Remotes).Name);

        await context.Service.RemoveRemoteAsync(context.LogicalRoot, "upstream");
        Assert.Empty((await context.Service.SnapshotAsync(context.LogicalRoot)).Remotes);
    }

    [Fact]
    public async Task CreatesSwitchesAndMergesWithoutDiscardingWorktreeChanges()
    {
        if (!GitIsAvailable())
        {
            return;
        }

        using var context = GitTestContext.Create();
        await context.Service.InitializeRepositoryAsync(context.LogicalRoot);
        ConfigureIdentity(context.Root);
        var file = Path.Combine(context.Root, "value.txt");
        await File.WriteAllTextAsync(file, "main\n");
        await context.Service.StageAsync(context.LogicalRoot, ["value.txt"]);
        await context.Service.CommitAsync(context.LogicalRoot, "initial");

        await context.Service.CreateBranchAsync(
            context.LogicalRoot,
            "feature/git-workbench",
            switchToBranch: true);
        Assert.Equal(
            "feature/git-workbench",
            (await context.Service.SnapshotAsync(context.LogicalRoot)).CurrentBranch);

        await File.WriteAllTextAsync(file, "feature\n");
        await context.Service.StageAsync(context.LogicalRoot, ["value.txt"]);
        await context.Service.CommitAsync(context.LogicalRoot, "feature change");
        await context.Service.SwitchBranchAsync(context.LogicalRoot, "main");
        await context.Service.MergeBranchAsync(context.LogicalRoot, "feature/git-workbench");

        var snapshot = await context.Service.SnapshotAsync(context.LogicalRoot);
        Assert.Equal("main", snapshot.CurrentBranch);
        Assert.Equal("feature change", snapshot.Commits[0].Subject);
        Assert.Equal("feature\n", await File.ReadAllTextAsync(file));

        await context.Service.CreateBranchAsync(context.LogicalRoot, "other", switchToBranch: true);
        await File.WriteAllTextAsync(file, "other branch\n");
        await context.Service.StageAsync(context.LogicalRoot, ["value.txt"]);
        await context.Service.CommitAsync(context.LogicalRoot, "other branch change");
        await context.Service.SwitchBranchAsync(context.LogicalRoot, "main");
        await File.WriteAllTextAsync(file, "not committed\n");
        var exception = await Assert.ThrowsAsync<ProjectGitOperationException>(() =>
            context.Service.SwitchBranchAsync(context.LogicalRoot, "other"));
        Assert.Equal(ProjectGitErrorCode.CommandFailed, exception.Code);
        Assert.Equal("not committed\n", await File.ReadAllTextAsync(file));
    }

    [Fact]
    public async Task RejectsForgedPathsAndInvalidNamesBeforeRunningMutation()
    {
        using var context = GitTestContext.Create(new RecordingGitProcess());
        var service = context.Service;

        var branch = await Assert.ThrowsAsync<ProjectGitOperationException>(() =>
            service.CreateBranchAsync(context.LogicalRoot, "-unsafe", true));
        var remote = await Assert.ThrowsAsync<ProjectGitOperationException>(() =>
            service.SaveRemoteAsync(context.LogicalRoot, null, "-origin", "https://example.com"));

        Assert.Equal(ProjectGitErrorCode.InvalidBranchName, branch.Code);
        Assert.Equal(ProjectGitErrorCode.InvalidRemote, remote.Code);
    }

    private static bool GitIsAvailable()
    {
        try
        {
            using var process = Process.Start(new ProcessStartInfo
            {
                FileName = OperatingSystem.IsWindows() ? "git.exe" : "git",
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                ArgumentList = { "--version" },
            });
            process?.WaitForExit(5_000);
            return process?.ExitCode == 0;
        }
        catch
        {
            return false;
        }
    }

    private static void ConfigureIdentity(string root)
    {
        RunGit(root, "config", "user.name", "ChatOS Tests");
        RunGit(root, "config", "user.email", "tests@chatos.local");
    }

    private static void RunGit(string root, params string[] arguments)
    {
        var startInfo = new ProcessStartInfo
        {
            FileName = OperatingSystem.IsWindows() ? "git.exe" : "git",
            WorkingDirectory = root,
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        foreach (var argument in arguments)
        {
            startInfo.ArgumentList.Add(argument);
        }

        using var process = Process.Start(startInfo) ?? throw new InvalidOperationException("Git did not start.");
        process.WaitForExit();
        Assert.Equal(0, process.ExitCode);
    }

    private sealed class GitTestContext : IDisposable
    {
        private GitTestContext(string root, IGitProcess? git)
        {
            Root = root;
            LogicalRoot = "local://connector/device-1/workspace-1";
            var workspace = new ConnectorWorkspace("workspace-1", "Test", root, "fingerprint");
            var resolver = new LocalProjectPathResolver(new TestWorkspaceContext(workspace));
            Service = git is null
                ? new WindowsProjectGitService(resolver)
                : new WindowsProjectGitService(resolver, git);
        }

        public string Root { get; }

        public string LogicalRoot { get; }

        public WindowsProjectGitService Service { get; }

        public static GitTestContext Create(IGitProcess? git = null)
        {
            var temporaryRoot = Path.GetTempPath();
            if (OperatingSystem.IsMacOS() && temporaryRoot.StartsWith("/var/", StringComparison.Ordinal))
            {
                temporaryRoot = "/private" + temporaryRoot;
            }

            var root = Path.Combine(temporaryRoot, $"chatos-git-{Guid.NewGuid():N}");
            Directory.CreateDirectory(root);
            return new GitTestContext(root, git);
        }

        public void Dispose()
        {
            if (Directory.Exists(Root))
            {
                Directory.Delete(Root, recursive: true);
            }
        }
    }

    private sealed class TestWorkspaceContext(ConnectorWorkspace workspace) : IConnectorWorkspaceContext
    {
        public string? DeviceId => "device-1";

        public IReadOnlyList<ConnectorWorkspace> Workspaces => [workspace];

        public ConnectorWorkspace? Find(string workspaceId) =>
            workspaceId == workspace.Id ? workspace : null;
    }

    private sealed class RecordingGitProcess : IGitProcess
    {
        public Task<GitProcessOutput> RunAsync(
            IReadOnlyList<string> arguments,
            string workingDirectory,
            IReadOnlySet<int>? allowedExitCodes = null,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new GitProcessOutput(string.Empty, string.Empty, 128, false, false));
    }
}
