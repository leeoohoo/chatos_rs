using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class ProjectRunViewModelTests
{
    [Fact]
    public async Task OpenLoadsCatalogStateAndEnvironmentWithStableSelections()
    {
        var service = new RunServiceDouble();
        using var viewModel = new ProjectRunViewModel(service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(Project("project-1"));

        Assert.Equal("target-2", viewModel.SelectedTarget?.Id);
        Assert.Equal("terminal-1", Assert.Single(viewModel.Instances).Id);
        Assert.Equal("node-22", Assert.Single(viewModel.Toolchains).SelectedOptionId);
        Assert.Equal("3000", Assert.Single(viewModel.EnvironmentVariables).Value);
        Assert.True(viewModel.HasValidationIssues);
        Assert.False(viewModel.CanStart);
    }

    [Fact]
    public async Task FailedEnvironmentSavePreservesUnsavedDrafts()
    {
        var service = new RunServiceDouble { FailEnvironmentUpdate = true };
        using var viewModel = new ProjectRunViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Project("project-1"));
        var toolchain = Assert.Single(viewModel.Toolchains);
        var variable = Assert.Single(viewModel.EnvironmentVariables);
        toolchain.SelectedOptionId = "node-custom";
        variable.Value = "4100";
        viewModel.AddEnvironmentVariableCommand.Execute(null);
        viewModel.EnvironmentVariables[^1].Key = "NODE_ENV";
        viewModel.EnvironmentVariables[^1].Value = "development";

        await viewModel.SaveEnvironmentCommand.ExecuteAsync(null);

        Assert.Equal("node-custom", toolchain.SelectedOptionId);
        Assert.Equal("4100", variable.Value);
        Assert.Equal("development", viewModel.EnvironmentVariables[^1].Value);
        Assert.Equal("save failed", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task StartStopAndDeleteUseCapturedTargetAndTerminalIds()
    {
        var service = new RunServiceDouble { EnvironmentHasIssue = false };
        using var viewModel = new ProjectRunViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Project("project-1"));
        var instance = Assert.Single(viewModel.Instances);

        await viewModel.StartCommand.ExecuteAsync(null);
        await viewModel.StopCommand.ExecuteAsync(instance);
        await viewModel.DeleteInstanceCommand.ExecuteAsync(instance);

        Assert.Equal(("project-1", "target-2"), service.LastStart);
        Assert.Equal("terminal-1", service.LastStopId);
        Assert.Equal("terminal-1", service.LastDeleteId);
        Assert.True(service.StateFetchCount >= 4);
        Assert.False(viewModel.IsMutating);
    }

    [Fact]
    public async Task OlderProjectLoadCannotOverwriteNewProject()
    {
        var service = new DelayedProjectRunService();
        using var viewModel = new ProjectRunViewModel(service, new ImmediateUiDispatcher());
        var oldOpen = viewModel.OpenAsync(Project("project-old"));
        await service.OldRequestsStarted.Task.WaitAsync(TimeSpan.FromSeconds(2));

        await viewModel.OpenAsync(Project("project-new"));
        service.ReleaseOldRequests();
        await oldOpen;

        Assert.Equal("project-new", viewModel.ProjectId);
        Assert.Equal("project-new-target", viewModel.SelectedTarget?.Id);
        Assert.Equal("project-new-terminal", Assert.Single(viewModel.Instances).Id);
        Assert.False(viewModel.IsLoading);
        Assert.False(viewModel.IsMutating);
    }

    private static WorkspaceProject Project(string id) => new(id, id, id, id, null);

    private class RunServiceDouble : IProjectRunService
    {
        public bool FailEnvironmentUpdate { get; init; }

        public bool EnvironmentHasIssue { get; init; } = true;

        public (string ProjectId, string TargetId)? LastStart { get; private set; }

        public string? LastStopId { get; private set; }

        public string? LastDeleteId { get; private set; }

        public int StateFetchCount { get; private set; }

        public virtual Task<ProjectRunCatalog> FetchCatalogAsync(
            string projectId,
            CancellationToken cancellationToken = default) => Task.FromResult(Catalog(projectId));

        public virtual Task<ProjectRunCatalog> AnalyzeAsync(
            string projectId,
            CancellationToken cancellationToken = default) => Task.FromResult(Catalog(projectId));

        public virtual Task<ProjectRunState> FetchStateAsync(
            string projectId,
            CancellationToken cancellationToken = default)
        {
            StateFetchCount++;
            return Task.FromResult(State(projectId));
        }

        public virtual Task<ProjectRunEnvironment> FetchEnvironmentAsync(
            string projectId,
            CancellationToken cancellationToken = default) => Task.FromResult(Environment(EnvironmentHasIssue));

        public virtual Task<ProjectRunEnvironment> UpdateEnvironmentAsync(
            string projectId,
            IReadOnlyDictionary<string, string> selectedToolchains,
            IReadOnlyDictionary<string, ProjectRunCustomToolchain> customToolchains,
            IReadOnlyDictionary<string, string> environmentVariables,
            CancellationToken cancellationToken = default)
        {
            if (FailEnvironmentUpdate)
            {
                throw new InvalidOperationException("save failed");
            }

            return Task.FromResult(new ProjectRunEnvironment(
                Environment(false).ToolchainOptions,
                Array.Empty<ProjectRunConfigurationFile>(),
                Array.Empty<ProjectRunValidationIssue>(),
                selectedToolchains,
                customToolchains,
                environmentVariables,
                true));
        }

        public virtual Task<ProjectRunCatalog> SetDefaultTargetAsync(
            string projectId,
            string targetId,
            CancellationToken cancellationToken = default) => Task.FromResult(Catalog(projectId));

        public virtual Task StartAsync(
            string projectId,
            string targetId,
            CancellationToken cancellationToken = default)
        {
            LastStart = (projectId, targetId);
            return Task.CompletedTask;
        }

        public virtual Task StopAsync(
            string instanceId,
            CancellationToken cancellationToken = default)
        {
            LastStopId = instanceId;
            return Task.CompletedTask;
        }

        public virtual Task DeleteAsync(
            string instanceId,
            CancellationToken cancellationToken = default)
        {
            LastDeleteId = instanceId;
            return Task.CompletedTask;
        }

        protected static ProjectRunCatalog Catalog(string projectId) => new(
            projectId,
            "ready",
            "target-2",
            new[]
            {
                Target("target-1"),
                Target("target-2") with { IsDefault = true },
            },
            null);

        protected static ProjectRunState State(string projectId) => new(
            projectId,
            "running",
            false,
            true,
            new[]
            {
                new ProjectRunInstance(
                    "terminal-1",
                    "Web",
                    "apps/web",
                    "running",
                    false,
                    true,
                    "ready",
                    DateTimeOffset.UtcNow,
                    null),
            });

        protected static ProjectRunTarget Target(string id) => new(
            id,
            id,
            "npm",
            "typescript",
            "apps/web",
            "npm run dev",
            "package.json",
            false,
            "src/main.ts",
            "package.json",
            new[] { "node" });

        protected static ProjectRunEnvironment Environment(bool hasIssue) => new(
            new Dictionary<string, IReadOnlyList<ProjectRunToolchainOption>>
            {
                ["node"] = new[]
                {
                    new ProjectRunToolchainOption(
                        "node-22",
                        "node",
                        "Node.js",
                        "22",
                        "C:\\node.exe",
                        "path",
                        true),
                    new ProjectRunToolchainOption(
                        "node-custom",
                        "node",
                        "Custom Node",
                        null,
                        "D:\\node.exe",
                        "custom",
                        false),
                },
            },
            new[]
            {
                new ProjectRunConfigurationFile("package", "package.json", "package.json", "{}", "project"),
            },
            hasIssue
                ? new[] { new ProjectRunValidationIssue("warning", "检查 PORT", "target-2", "target-2", ".env", "设置 PORT") }
                : Array.Empty<ProjectRunValidationIssue>(),
            new Dictionary<string, string> { ["node"] = "node-22" },
            new Dictionary<string, ProjectRunCustomToolchain>(),
            new Dictionary<string, string> { ["PORT"] = "3000" },
            true);
    }

    private sealed class DelayedProjectRunService : RunServiceDouble
    {
        private readonly TaskCompletionSource _release = new(TaskCreationOptions.RunContinuationsAsynchronously);
        private int _oldRequestCount;

        public TaskCompletionSource OldRequestsStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public void ReleaseOldRequests() => _release.TrySetResult();

        public override async Task<ProjectRunCatalog> FetchCatalogAsync(
            string projectId,
            CancellationToken cancellationToken = default)
        {
            if (projectId == "project-old")
            {
                MarkOldRequest();
                await _release.Task;
            }

            return new ProjectRunCatalog(
                projectId,
                "ready",
                $"{projectId}-target",
                new[] { Target($"{projectId}-target") },
                null);
        }

        public override async Task<ProjectRunState> FetchStateAsync(
            string projectId,
            CancellationToken cancellationToken = default)
        {
            if (projectId == "project-old")
            {
                MarkOldRequest();
                await _release.Task;
            }

            return new ProjectRunState(
                projectId,
                "idle",
                false,
                false,
                new[]
                {
                    new ProjectRunInstance(
                        $"{projectId}-terminal",
                        projectId,
                        null,
                        "stopped",
                        false,
                        false,
                        null,
                        null,
                        0),
                });
        }

        public override async Task<ProjectRunEnvironment> FetchEnvironmentAsync(
            string projectId,
            CancellationToken cancellationToken = default)
        {
            if (projectId == "project-old")
            {
                MarkOldRequest();
                await _release.Task;
            }

            return Environment(false);
        }

        private void MarkOldRequest()
        {
            if (Interlocked.Increment(ref _oldRequestCount) == 3)
            {
                OldRequestsStarted.TrySetResult();
            }
        }
    }
}
