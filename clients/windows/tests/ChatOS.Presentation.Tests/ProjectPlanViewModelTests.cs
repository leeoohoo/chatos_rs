using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class ProjectPlanViewModelTests
{
    [Fact]
    public async Task OpenFlattensRequirementHierarchyAndLoadsSelectedDetail()
    {
        var service = new PlanServiceDouble();
        using var viewModel = new ProjectPlanViewModel(
            service,
            new ExecutionServiceDouble(),
            new ImmediateUiDispatcher());

        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));

        Assert.Equal(new[] { "root", "child" }, viewModel.Requirements.Select(static value => value.Id));
        Assert.Equal(1, viewModel.Requirements[1].Depth);
        Assert.Equal("root", viewModel.SelectedRequirement?.Id);
        Assert.Single(viewModel.WorkItems);
        Assert.Single(viewModel.Documents);
        Assert.Equal("group-1", viewModel.Execution?.ExecutionGroupId);
    }

    [Fact]
    public async Task CreateExecutionUsesCurrentRequirementAndReplacesWorkbenchIdentity()
    {
        var service = new PlanServiceDouble();
        using var viewModel = new ProjectPlanViewModel(
            service,
            new ExecutionServiceDouble(),
            new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));
        viewModel.IncludePrerequisiteDependents = true;
        viewModel.PlanningFeedback = "先检查依赖";

        await viewModel.CreateExecutionCommand.ExecuteAsync(null);

        Assert.Equal(("project-1", "root", true, "先检查依赖"), service.LastCreation);
        Assert.Equal("group-new", viewModel.Execution?.ExecutionGroupId);
        Assert.Equal(string.Empty, viewModel.PlanningFeedback);
    }

    [Fact]
    public async Task ConfirmExecutionUsesCompleteIdentityAndRefreshesAuthoritativeStatus()
    {
        var plan = new PlanServiceDouble();
        var execution = new ExecutionServiceDouble
        {
            RefreshedExecution = PlanServiceDouble.Launch("project-1", "root", "group-1") with
            {
                ConfirmationStatus = "confirmed",
                HasStartedRuns = true,
                OverallStatus = "running",
            },
        };
        using var viewModel = new ProjectPlanViewModel(plan, execution, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));

        await viewModel.ConfirmExecutionCommand.ExecuteAsync(null);

        Assert.Equal(
            new ProjectExecutionIdentity("project-1", "root", "group-1", "conversation-1", "contact-1"),
            execution.LastConfirmedIdentity);
        Assert.Equal("running", viewModel.Execution?.OverallStatus);
        Assert.True(viewModel.Execution?.HasStartedRuns);
        Assert.Equal("已确认执行，任务将按依赖顺序运行。", viewModel.ExecutionActionMessage);
        Assert.False(viewModel.IsMutatingExecution);
    }

    [Fact]
    public async Task StopExecutionClearsDiscardedPlanWhenGatewayNoLongerReturnsIt()
    {
        var execution = new ExecutionServiceDouble { RefreshedExecution = null };
        using var viewModel = new ProjectPlanViewModel(
            new PlanServiceDouble(),
            execution,
            new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "ChatOS", "root", "root", null));

        await viewModel.StopExecutionCommand.ExecuteAsync(null);

        Assert.Equal("group-1", execution.LastStoppedIdentity?.ExecutionGroupId);
        Assert.Null(viewModel.Execution);
        Assert.Equal("执行计划已停止，未继续运行的任务已清理。", viewModel.ExecutionActionMessage);
    }

    [Fact]
    public async Task OldConfirmationCannotOverwriteNewProjectSession()
    {
        var execution = new DelayedExecutionServiceDouble();
        using var viewModel = new ProjectPlanViewModel(
            new PlanServiceDouble(),
            execution,
            new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new WorkspaceProject("project-1", "One", "one", "one", null));
        var confirm = viewModel.ConfirmExecutionCommand.ExecuteAsync(null);
        await execution.ConfirmationStarted.Task.WaitAsync(TimeSpan.FromSeconds(2));

        await viewModel.OpenAsync(new WorkspaceProject("project-2", "Two", "two", "two", null));
        execution.ReleaseConfirmation();
        await confirm;

        Assert.Equal("project-2", viewModel.ProjectId);
        Assert.Equal("project-2", viewModel.Execution?.ProjectId);
        Assert.False(viewModel.IsMutatingExecution);
    }

    private sealed class PlanServiceDouble : IProjectPlanService
    {
        public (string ProjectId, string RequirementId, bool Include, string? Feedback)? LastCreation { get; private set; }

        public Task<ProjectPlanSnapshot> FetchPlanAsync(
            string projectId,
            CancellationToken cancellationToken = default) => Task.FromResult(new ProjectPlanSnapshot(
            projectId,
            new[]
            {
                Requirement("child", "root"),
                Requirement("root", null),
            },
            Array.Empty<ProjectWorkItem>(),
            Array.Empty<ProjectPlanEdge>(),
            new ProjectPlanCounts(1, 1, 0, 0)));

        public Task<ProjectPlanSnapshot> FetchWorkItemsAsync(
            string projectId,
            string requirementId,
            CancellationToken cancellationToken = default) => Task.FromResult(new ProjectPlanSnapshot(
            projectId,
            Array.Empty<ProjectRequirement>(),
            new[]
            {
                new ProjectWorkItem("task-1", requirementId, "实现", null, "todo", 1, Array.Empty<string>(), false, null),
            },
            Array.Empty<ProjectPlanEdge>(),
            new ProjectPlanCounts(1, 1, 0, 0)));

        public Task<IReadOnlyList<ProjectRequirementDocument>> FetchDocumentsAsync(
            string projectId,
            string requirementId,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ProjectRequirementDocument>>(new[]
            {
                new ProjectRequirementDocument("doc-1", "实施计划", "plan", "markdown", "# Plan", 1, null),
            });

        public Task<ProjectRequirementExecutionLaunch?> FetchExecutionAsync(
            string projectId,
            string requirementId,
            CancellationToken cancellationToken = default) => Task.FromResult<ProjectRequirementExecutionLaunch?>(
            Launch(projectId, requirementId, "group-1"));

        public Task<ProjectRequirementExecutionLaunch> CreateExecutionAsync(
            string projectId,
            string requirementId,
            bool includePrerequisiteDependents,
            string? planningFeedback,
            CancellationToken cancellationToken = default)
        {
            LastCreation = (projectId, requirementId, includePrerequisiteDependents, planningFeedback);
            return Task.FromResult(Launch(projectId, requirementId, "group-new"));
        }

        private static ProjectRequirement Requirement(string id, string? parent) => new(
            id,
            "project-1",
            parent,
            "requirement",
            id,
            null,
            null,
            null,
            null,
            1,
            "draft",
            null);

        internal static ProjectRequirementExecutionLaunch Launch(
            string projectId,
            string requirementId,
            string groupId) => new(
            projectId,
            requirementId,
            "conversation-1",
            groupId,
            null,
            "pending",
            false,
            "awaiting_confirmation",
            "contact-1",
            1,
            false,
            null,
            null,
                null);
    }

    private class ExecutionServiceDouble : IProjectExecutionService
    {
        public ProjectRequirementExecutionLaunch? RefreshedExecution { get; init; }

        public ProjectExecutionIdentity? LastConfirmedIdentity { get; private set; }

        public ProjectExecutionIdentity? LastStoppedIdentity { get; private set; }

        public virtual Task<ProjectRequirementExecutionLaunch?> FetchExecutionAsync(
            ProjectExecutionIdentity identity,
            CancellationToken cancellationToken = default) => Task.FromResult(
            RefreshedExecution ?? (LastStoppedIdentity is null
                ? PlanServiceDouble.Launch(identity.ProjectId, identity.RequirementId, identity.ExecutionGroupId)
                : null));

        public virtual Task<ProjectExecutionActionResult> ConfirmExecutionAsync(
            ProjectExecutionIdentity identity,
            CancellationToken cancellationToken = default)
        {
            LastConfirmedIdentity = identity;
            return Task.FromResult(new ProjectExecutionActionResult(
                true,
                "running",
                identity.ExecutionGroupId,
                new[] { "task-1" },
                new[] { "task-1" },
                null));
        }

        public virtual Task<ProjectExecutionActionResult> StopExecutionAsync(
            ProjectExecutionIdentity identity,
            CancellationToken cancellationToken = default)
        {
            LastStoppedIdentity = identity;
            return Task.FromResult(new ProjectExecutionActionResult(
                true,
                "stopped",
                identity.ExecutionGroupId,
                Array.Empty<string>(),
                Array.Empty<string>(),
                true));
        }
    }

    private sealed class DelayedExecutionServiceDouble : ExecutionServiceDouble
    {
        private readonly TaskCompletionSource _release = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource ConfirmationStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public void ReleaseConfirmation() => _release.TrySetResult();

        public override async Task<ProjectExecutionActionResult> ConfirmExecutionAsync(
            ProjectExecutionIdentity identity,
            CancellationToken cancellationToken = default)
        {
            ConfirmationStarted.TrySetResult();
            await _release.Task;
            return new ProjectExecutionActionResult(
                true,
                "running",
                identity.ExecutionGroupId,
                Array.Empty<string>(),
                Array.Empty<string>(),
                null);
        }
    }
}
