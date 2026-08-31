using ChatOS.Connector.Workspaces;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Git;

public sealed class WindowsProjectGitService : IProjectGitService
{
    private readonly ILocalProjectPathResolver _paths;
    private readonly IGitProcess _git;

    public WindowsProjectGitService(ILocalProjectPathResolver paths)
        : this(paths, new GitProcess())
    {
    }

    internal WindowsProjectGitService(ILocalProjectPathResolver paths, IGitProcess git)
    {
        _paths = paths;
        _git = git;
    }

    public async Task<ProjectGitSnapshot> SnapshotAsync(
        string projectRoot,
        CancellationToken cancellationToken = default)
    {
        var project = _paths.Resolve(projectRoot);
        var repository = await RepositoryContextAsync(project, cancellationToken).ConfigureAwait(false);
        if (repository is null)
        {
            return ProjectGitSnapshot.Unavailable;
        }

        var head = await RunAsync(
            ["rev-parse", "--verify", "HEAD"],
            repository.Root,
            GitExitCodes.SuccessOrNotFound,
            cancellationToken).ConfigureAwait(false);
        var hasHead = head.ExitCode == 0;
        var branch = await RunAsync(
            ["symbolic-ref", "--quiet", "--short", "HEAD"],
            repository.Root,
            GitExitCodes.SuccessOrOne,
            cancellationToken).ConfigureAwait(false);
        var currentBranch = branch.ExitCode == 0 ? TrimmedOrNull(branch.StandardOutput) : null;
        string? detachedCommit = null;
        if (currentBranch is null && hasHead)
        {
            detachedCommit = TrimmedOrNull((await RunAsync(
                ["rev-parse", "--short", "HEAD"],
                repository.Root,
                cancellationToken: cancellationToken).ConfigureAwait(false)).StandardOutput);
        }

        var statusTask = RunAsync(
            ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            repository.Root,
            cancellationToken: cancellationToken);
        var branchesTask = RunAsync(
            [
                "for-each-ref",
                "--sort=-committerdate",
                "--format=%(refname:short)%00%(HEAD)%00%(upstream:short)",
                "refs/heads",
            ],
            repository.Root,
            cancellationToken: cancellationToken);
        var remoteNamesTask = RunAsync(
            ["remote"],
            repository.Root,
            cancellationToken: cancellationToken);
        await Task.WhenAll(statusTask, branchesTask, remoteNamesTask).ConfigureAwait(false);

        var remotes = new List<ProjectGitRemote>();
        foreach (var name in remoteNamesTask.Result.StandardOutput.Split(
                     ['\r', '\n'],
                     StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            var remoteUrl = await RunAsync(
                ["remote", "get-url", name],
                repository.Root,
                GitExitCodes.SuccessOrNotFound,
                cancellationToken).ConfigureAwait(false);
            remotes.Add(new ProjectGitRemote(name, TrimmedOrNull(remoteUrl.StandardOutput) ?? string.Empty));
        }

        string? upstream = null;
        var ahead = 0;
        var behind = 0;
        if (currentBranch is not null)
        {
            var upstreamOutput = await RunAsync(
                ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
                repository.Root,
                GitExitCodes.SuccessOrNotFound,
                cancellationToken).ConfigureAwait(false);
            upstream = upstreamOutput.ExitCode == 0
                ? TrimmedOrNull(upstreamOutput.StandardOutput)
                : null;
            if (upstream is not null)
            {
                var counts = (await RunAsync(
                    ["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
                    repository.Root,
                    cancellationToken: cancellationToken).ConfigureAwait(false)).StandardOutput
                    .Split(['\t', ' ', '\r', '\n'], StringSplitOptions.RemoveEmptyEntries)
                    .Select(static value => int.TryParse(value, out var count) ? count : -1)
                    .ToArray();
                if (counts.Length == 2 && counts.All(static count => count >= 0))
                {
                    ahead = counts[0];
                    behind = counts[1];
                }
            }
        }

        IReadOnlyList<ProjectGitCommit> commits = Array.Empty<ProjectGitCommit>();
        if (hasHead)
        {
            var log = await RunAsync(
                [
                    "log",
                    "--all",
                    "-n",
                    "80",
                    "--date=iso-strict",
                    "--pretty=format:%H%x00%h%x00%P%x00%an%x00%aI%x00%D%x00%s%x1e",
                ],
                repository.Root,
                cancellationToken: cancellationToken).ConfigureAwait(false);
            commits = ProjectGitParser.ParseCommits(log.StandardOutput);
        }

        return new ProjectGitSnapshot(
            true,
            repository.Root,
            currentBranch,
            detachedCommit,
            upstream,
            ahead,
            behind,
            hasHead,
            ProjectGitParser.ParseChanges(statusTask.Result.StandardOutput, repository.Root),
            ProjectGitParser.ParseBranches(branchesTask.Result.StandardOutput),
            commits,
            remotes);
    }

    public async Task InitializeRepositoryAsync(
        string projectRoot,
        CancellationToken cancellationToken = default)
    {
        var project = _paths.Resolve(projectRoot);
        _ = await RunAsync(
            ["init", "-b", "main"],
            project.AbsolutePath,
            cancellationToken: cancellationToken).ConfigureAwait(false);
    }

    public async Task<ProjectGitDiff> DiffAsync(
        string projectRoot,
        ProjectGitChange change,
        bool staged,
        CancellationToken cancellationToken = default)
    {
        var repository = await RequiredRepositoryAsync(projectRoot, cancellationToken).ConfigureAwait(false);
        ValidateChange(change, repository.Root);
        GitProcessOutput output;
        if (change.Kind == ProjectGitChangeKind.Untracked && !staged)
        {
            output = await RunAsync(
                ["diff", "--no-index", "--no-color", "--no-ext-diff", "--", "/dev/null", change.AbsolutePath],
                repository.Root,
                GitExitCodes.SuccessOrOne,
                cancellationToken).ConfigureAwait(false);
        }
        else
        {
            var arguments = new List<string> { "diff", "--no-color", "--no-ext-diff" };
            if (staged)
            {
                arguments.Add("--cached");
            }

            arguments.Add("--");
            arguments.Add(change.Path);
            output = await RunAsync(arguments, repository.Root, cancellationToken: cancellationToken)
                .ConfigureAwait(false);
        }

        return new ProjectGitDiff(change.Path, staged, output.StandardOutput);
    }

    public Task StageAsync(
        string projectRoot,
        IReadOnlyList<string> paths,
        CancellationToken cancellationToken = default) =>
        MutatePathsAsync(projectRoot, paths, ["add", "--"], cancellationToken);

    public async Task UnstageAsync(
        string projectRoot,
        IReadOnlyList<string> paths,
        CancellationToken cancellationToken = default)
    {
        if (paths.Count == 0)
        {
            return;
        }

        var repository = await RequiredRepositoryAsync(projectRoot, cancellationToken).ConfigureAwait(false);
        ValidatePaths(paths, repository.Root);
        var head = await RunAsync(
            ["rev-parse", "--verify", "HEAD"],
            repository.Root,
            GitExitCodes.SuccessOrNotFound,
            cancellationToken).ConfigureAwait(false);
        var arguments = head.ExitCode == 0
            ? new List<string> { "restore", "--staged", "--" }
            : new List<string> { "rm", "--cached", "--" };
        arguments.AddRange(paths);
        _ = await RunAsync(arguments, repository.Root, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
    }

    public async Task CommitAsync(
        string projectRoot,
        string message,
        CancellationToken cancellationToken = default)
    {
        var clean = message.Trim();
        if (clean.Length == 0)
        {
            throw new ProjectGitOperationException(
                ProjectGitErrorCode.EmptyCommitMessage,
                "请输入提交说明。");
        }

        var repository = await RequiredRepositoryAsync(projectRoot, cancellationToken).ConfigureAwait(false);
        _ = await RunAsync(
            ["commit", "-m", clean],
            repository.Root,
            cancellationToken: cancellationToken).ConfigureAwait(false);
    }

    public async Task SwitchBranchAsync(
        string projectRoot,
        string branch,
        CancellationToken cancellationToken = default)
    {
        ValidateBranchSyntax(branch);
        var repository = await RequiredRepositoryAsync(projectRoot, cancellationToken).ConfigureAwait(false);
        await ValidateBranchAsync(branch, repository.Root, cancellationToken).ConfigureAwait(false);
        _ = await RunAsync(
            ["switch", branch],
            repository.Root,
            cancellationToken: cancellationToken).ConfigureAwait(false);
    }

    public async Task CreateBranchAsync(
        string projectRoot,
        string name,
        bool switchToBranch,
        CancellationToken cancellationToken = default)
    {
        ValidateBranchSyntax(name);
        var repository = await RequiredRepositoryAsync(projectRoot, cancellationToken).ConfigureAwait(false);
        await ValidateBranchAsync(name, repository.Root, cancellationToken).ConfigureAwait(false);
        var arguments = switchToBranch
            ? new[] { "switch", "-c", name }
            : new[] { "branch", name };
        _ = await RunAsync(arguments, repository.Root, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
    }

    public async Task MergeBranchAsync(
        string projectRoot,
        string branch,
        CancellationToken cancellationToken = default)
    {
        ValidateBranchSyntax(branch);
        var repository = await RequiredRepositoryAsync(projectRoot, cancellationToken).ConfigureAwait(false);
        await ValidateBranchAsync(branch, repository.Root, cancellationToken).ConfigureAwait(false);
        _ = await RunAsync(
            ["merge", "--no-edit", branch],
            repository.Root,
            cancellationToken: cancellationToken).ConfigureAwait(false);
    }

    public async Task SaveRemoteAsync(
        string projectRoot,
        string? originalName,
        string name,
        string url,
        CancellationToken cancellationToken = default)
    {
        var cleanName = NormalizeRemoteName(name);
        var cleanUrl = url.Trim();
        if (cleanUrl.Length == 0 || cleanUrl.Any(char.IsControl))
        {
            throw new ProjectGitOperationException(
                ProjectGitErrorCode.InvalidRemote,
                "远程仓库名称和地址不能为空。");
        }

        var repository = await RequiredRepositoryAsync(projectRoot, cancellationToken).ConfigureAwait(false);
        if (originalName is not null)
        {
            var cleanOriginalName = NormalizeRemoteName(originalName);
            if (!string.Equals(cleanOriginalName, cleanName, StringComparison.Ordinal))
            {
                _ = await RunAsync(
                    ["remote", "rename", cleanOriginalName, cleanName],
                    repository.Root,
                    cancellationToken: cancellationToken).ConfigureAwait(false);
            }

            _ = await RunAsync(
                ["remote", "set-url", cleanName, cleanUrl],
                repository.Root,
                cancellationToken: cancellationToken).ConfigureAwait(false);
            return;
        }

        _ = await RunAsync(
            ["remote", "add", cleanName, cleanUrl],
            repository.Root,
            cancellationToken: cancellationToken).ConfigureAwait(false);
    }

    public async Task RemoveRemoteAsync(
        string projectRoot,
        string name,
        CancellationToken cancellationToken = default)
    {
        var repository = await RequiredRepositoryAsync(projectRoot, cancellationToken).ConfigureAwait(false);
        _ = await RunAsync(
            ["remote", "remove", NormalizeRemoteName(name)],
            repository.Root,
            cancellationToken: cancellationToken).ConfigureAwait(false);
    }

    public async Task PullAsync(
        string projectRoot,
        CancellationToken cancellationToken = default)
    {
        var repository = await RequiredRepositoryAsync(projectRoot, cancellationToken).ConfigureAwait(false);
        _ = await RunAsync(
            ["pull", "--ff-only"],
            repository.Root,
            cancellationToken: cancellationToken).ConfigureAwait(false);
    }

    public async Task PushAsync(
        string projectRoot,
        CancellationToken cancellationToken = default)
    {
        var snapshot = await SnapshotAsync(projectRoot, cancellationToken).ConfigureAwait(false);
        var repository = snapshot.RepositoryRoot
            ?? throw new ProjectGitOperationException(
                ProjectGitErrorCode.NotRepository,
                "这个项目目录还不是 Git 仓库。");
        if (snapshot.Upstream is not null)
        {
            _ = await RunAsync(
                ["push"],
                repository,
                cancellationToken: cancellationToken).ConfigureAwait(false);
            return;
        }

        var branch = snapshot.CurrentBranch
            ?? throw new ProjectGitOperationException(
                ProjectGitErrorCode.NoCurrentBranch,
                "当前处于分离 HEAD 状态，不能直接发布分支。");
        var remote = snapshot.Remotes.FirstOrDefault()
            ?? throw new ProjectGitOperationException(
                ProjectGitErrorCode.NoRemote,
                "这个仓库还没有配置远程仓库。");
        _ = await RunAsync(
            ["push", "--set-upstream", remote.Name, branch],
            repository,
            cancellationToken: cancellationToken).ConfigureAwait(false);
    }

    private async Task MutatePathsAsync(
        string projectRoot,
        IReadOnlyList<string> paths,
        IReadOnlyList<string> prefix,
        CancellationToken cancellationToken)
    {
        if (paths.Count == 0)
        {
            return;
        }

        var repository = await RequiredRepositoryAsync(projectRoot, cancellationToken).ConfigureAwait(false);
        ValidatePaths(paths, repository.Root);
        var arguments = prefix.ToList();
        arguments.AddRange(paths);
        _ = await RunAsync(arguments, repository.Root, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
    }

    private async Task<RepositoryContext> RequiredRepositoryAsync(
        string projectRoot,
        CancellationToken cancellationToken)
    {
        var project = _paths.Resolve(projectRoot);
        return await RepositoryContextAsync(project, cancellationToken).ConfigureAwait(false)
            ?? throw new ProjectGitOperationException(
                ProjectGitErrorCode.NotRepository,
                "这个项目目录还不是 Git 仓库。");
    }

    private async Task<RepositoryContext?> RepositoryContextAsync(
        ResolvedLocalProjectPath project,
        CancellationToken cancellationToken)
    {
        var output = await RunAsync(
            ["rev-parse", "--show-toplevel"],
            project.AbsolutePath,
            GitExitCodes.SuccessOrNotFound,
            cancellationToken).ConfigureAwait(false);
        var rootValue = TrimmedOrNull(output.StandardOutput);
        if (output.ExitCode != 0 || rootValue is null)
        {
            return null;
        }

        var root = Path.GetFullPath(rootValue);
        var workspaceRoot = new WorkspacePathGuard(project.Workspace.AbsoluteRoot).Root;
        if (!LocalProjectPathResolver.IsInside(root, workspaceRoot))
        {
            throw new ProjectGitOperationException(
                ProjectGitErrorCode.RepositoryOutsideWorkspace,
                "Git 仓库根目录超出了当前项目允许访问的本机工作区。");
        }

        return new RepositoryContext(root);
    }

    private async Task ValidateBranchAsync(
        string branch,
        string repositoryRoot,
        CancellationToken cancellationToken)
    {
        var result = await RunAsync(
            ["check-ref-format", "--branch", branch],
            repositoryRoot,
            GitExitCodes.SuccessOrNotFound,
            cancellationToken).ConfigureAwait(false);
        if (result.ExitCode != 0)
        {
            throw InvalidBranch();
        }
    }

    private static void ValidateBranchSyntax(string branch)
    {
        var value = branch.Trim();
        if (value.Length == 0 || value != branch || value.StartsWith('-') || value.Any(char.IsControl))
        {
            throw InvalidBranch();
        }
    }

    private static ProjectGitOperationException InvalidBranch() => new(
        ProjectGitErrorCode.InvalidBranchName,
        "分支名称不符合 Git 规则，请换一个名称。");

    private static string NormalizeRemoteName(string name)
    {
        var value = name.Trim();
        if (value.Length == 0 ||
            value != name ||
            value.StartsWith('-') ||
            value.Any(char.IsWhiteSpace) ||
            value.Any(char.IsControl) ||
            value.IndexOfAny(['/', '\\', ':']) >= 0)
        {
            throw new ProjectGitOperationException(
                ProjectGitErrorCode.InvalidRemote,
                "远程仓库名称不符合规则。");
        }

        return value;
    }

    private static void ValidateChange(ProjectGitChange change, string repositoryRoot)
    {
        ValidatePaths([change.Path], repositoryRoot);
        var expected = Path.GetFullPath(Path.Combine(repositoryRoot, change.Path));
        var comparison = OperatingSystem.IsWindows()
            ? StringComparison.OrdinalIgnoreCase
            : StringComparison.Ordinal;
        if (!string.Equals(expected, Path.GetFullPath(change.AbsolutePath), comparison))
        {
            throw InvalidPath();
        }
    }

    private static void ValidatePaths(IReadOnlyList<string> paths, string repositoryRoot)
    {
        foreach (var path in paths)
        {
            if (string.IsNullOrWhiteSpace(path) || Path.IsPathFullyQualified(path) || path.IndexOf('\0') >= 0)
            {
                throw InvalidPath();
            }

            var absolute = Path.GetFullPath(Path.Combine(repositoryRoot, path));
            if (!LocalProjectPathResolver.IsInside(absolute, repositoryRoot))
            {
                throw InvalidPath();
            }
        }
    }

    private static ProjectGitOperationException InvalidPath() => new(
        ProjectGitErrorCode.InvalidPath,
        "Git 文件路径超出了当前仓库允许访问的范围。");

    private Task<GitProcessOutput> RunAsync(
        IReadOnlyList<string> arguments,
        string workingDirectory,
        IReadOnlySet<int>? allowedExitCodes = null,
        CancellationToken cancellationToken = default) =>
        _git.RunAsync(arguments, workingDirectory, allowedExitCodes, cancellationToken);

    private static string? TrimmedOrNull(string value)
    {
        value = value.Trim();
        return value.Length == 0 ? null : value;
    }

    private sealed record RepositoryContext(string Root);
}
