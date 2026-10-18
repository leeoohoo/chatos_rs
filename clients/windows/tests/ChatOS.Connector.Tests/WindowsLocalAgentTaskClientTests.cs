using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentTaskClientTests
{
    [Fact]
    public async Task RestartUsesCurrentTaskVersionAndReason()
    {
        var host = new TaskHost();
        var client = new WindowsLocalAgentTaskClient(host);
        var task = new WindowsLocalTask(
            "graph-1", "owner-1", "conversation_turn", "turn-1", "task-1",
            "Task", "model-1", JsonSerializer.SerializeToElement(new { }), "running",
            "run-1", 7, 1, 2);

        var graph = await client.RestartAsync(
            "owner-1", task, "restart from the beginning", CancellationToken.None);

        Assert.Equal("graph-1", graph.GraphId);
        var command = Assert.IsType<RestartLocalTaskCommand>(host.Command);
        Assert.Equal("restart_task", command.Type);
        Assert.Equal("owner-1", command.OwnerUserId);
        Assert.Equal("task-1", command.TaskId);
        Assert.Equal(7UL, command.ExpectedVersion);
        Assert.Equal("restart from the beginning", command.Reason);
    }

    [Fact]
    public async Task MatchingGraphsPushesConversationTurnFilterIntoHostQuery()
    {
        var host = new TaskHost();
        var client = new WindowsLocalAgentTaskClient(host);

        _ = await client.MatchingGraphsAsync(
            "owner-1", "turn-1", null, CancellationToken.None);

        var command = Assert.IsType<ListLocalTaskGraphsCommand>(host.Command);
        Assert.Equal("conversation_turn", command.SourceEntityType);
        Assert.Equal("turn-1", command.SourceEntityId);
    }

    [Fact]
    public async Task MessageGraphUsesConversationAndTurnScope()
    {
        var host = new TaskHost();
        var client = new WindowsLocalAgentTaskClient(host);

        var graph = await client.MessageGraphAsync(
            "owner-1", "conversation-1", "turn-1", "message-1", CancellationToken.None);

        Assert.Empty(graph.Nodes);
        var command = Assert.IsType<GetLocalMessageTaskGraphCommand>(host.Command);
        Assert.Equal("get_message_task_graph", command.Type);
        Assert.Equal("conversation-1", command.SourceConversationId);
        Assert.Equal("turn-1", command.SourceTurnId);
        Assert.Equal("message-1", command.SourceUserMessageId);
    }

    [Fact]
    public async Task EmptyMessageGraphFallsBackToTurnGraphForDisplayCancellationAndRetry()
    {
        var host = new EmptyMessageGraphFallbackHost();
        var service = new WindowsLocalAgentMessageTaskGraphService(
            new WindowsLocalAgentTaskClient(host), new WindowsLocalAgentRuntimeClient(host),
            new WindowsLocalAgentControlPlaneClient(host));
        service.Configure("owner-1");
        var lookup = new MessageTaskLookup("conversation-1", "turn-1", "message-1");

        var graph = await service.FetchGraphAsync("message-1", lookup);
        await service.CancelTaskAsync("message-1", "task-1", lookup, "acceptance cancel");
        var retried = await service.RetryRunAsync(
            "message-1", "run-1", lookup, "try the current settings");

        var node = Assert.Single(graph.Nodes);
        Assert.Equal("task-1", node.Task.Id);
        var cancel = Assert.IsType<CancelLocalTaskCommand>(
            host.Commands.Last(command => command is CancelLocalTaskCommand));
        Assert.Equal("task-1", cancel.TaskId);
        Assert.Equal(7UL, cancel.ExpectedVersion);
        Assert.Equal("acceptance cancel", cancel.Reason);
        Assert.Equal("run-2", retried.Id);
        var retry = Assert.IsType<RetryLocalTaskCommand>(
            host.Commands.Last(command => command is RetryLocalTaskCommand));
        Assert.Equal("task-1", retry.TaskId);
        Assert.Equal(7UL, retry.ExpectedVersion);
        Assert.Equal("try the current settings", retry.RetryInstruction);
        Assert.Equal(3, host.Commands.Count(command => command is GetLocalMessageTaskGraphCommand));
        Assert.Equal(3, host.Commands.Count(command => command is ListLocalTaskGraphsCommand));
    }

    [Fact]
    public async Task MessageGraphMapsPrerequisiteTitlesAndStatusesForDependentNodes()
    {
        var host = new DependencyMessageGraphHost();
        var service = new WindowsLocalAgentMessageTaskGraphService(
            new WindowsLocalAgentTaskClient(host), new WindowsLocalAgentRuntimeClient(host),
            new WindowsLocalAgentControlPlaneClient(host));
        service.Configure("owner-1");

        var graph = await service.FetchGraphAsync(
            "message-1", new MessageTaskLookup("conversation-1", "turn-1", "message-1"));

        var prerequisite = Assert.Single(graph.Nodes, node => node.Task.Id == "task-a");
        var dependent = Assert.Single(graph.Nodes, node => node.Task.Id == "task-b");
        Assert.Empty(prerequisite.Task.PrerequisiteTasks);
        Assert.Equal(["task-a"], dependent.Task.PrerequisiteTaskIds);
        var reference = Assert.Single(dependent.Task.PrerequisiteTasks);
        Assert.Equal("task-a", reference.Id);
        Assert.Equal("Prepare inputs", reference.Title);
        Assert.Equal("running", reference.Status);
    }

    [Fact]
    public async Task RunMapsFrozenAcceptanceContractEvidenceAndExactFinalResponse()
    {
        var host = new AcceptanceHost();
        var service = new WindowsLocalAgentMessageTaskGraphService(
            new WindowsLocalAgentTaskClient(host), new WindowsLocalAgentRuntimeClient(host),
            new WindowsLocalAgentControlPlaneClient(host));
        service.Configure("owner-1");

        var detail = await service.FetchRunAsync(
            "message-1", "run-accept", new MessageTaskLookup(
                "conversation-1", "turn-1", "message-1"));

        var contract = Assert.IsType<MessageTaskAcceptanceContract>(detail.Task.AcceptanceContract);
        Assert.Equal(1, contract.Version);
        Assert.Equal(12, contract.FinalResponseMaxChars);
        var report = Assert.IsType<MessageTaskAcceptanceReport>(detail.Run.AcceptanceReport);
        Assert.Equal("完成😀", report.FinalResponse);
        Assert.Equal(3, report.FinalResponseChars);
        Assert.Equal(MessageTaskResultDisposition.EvidenceIncomplete, report.Disposition);
        var check = Assert.Single(report.Checks);
        Assert.Equal("Inspect the artifact", check.Criterion);
        Assert.Equal("not_verified", check.Status);
        Assert.Equal(MessageTaskResultDisposition.EvidenceIncomplete, detail.Run.ResultDisposition);
        var plan = Assert.IsType<MessageTaskRequirementPlan>(detail.Task.RequirementPlan);
        Assert.Equal(2UL, plan.Snapshot.Revision);
        Assert.Equal("# Frozen proposal\n\nInspect safely.", plan.Snapshot.Markdown);
        Assert.Equal("Inspect the frozen artifact", plan.TaskScope.Prompt);
        Assert.Contains("inspection report", plan.TaskScope.ExpectedOutputs);
        Assert.Equal("openai/gpt-6.1-sol", detail.Task.DefaultModelConfig?.DisplayName);
        Assert.Equal("revision-1", detail.Task.DefaultModelConfigRevision);
        Assert.Equal("high", detail.Task.ThinkingLevel);
    }

    private sealed class TaskHost : ILocalAgentHostClient
    {
        public object? Command { get; private set; }
        public string? ActiveOwnerUserId => "owner-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> environment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            Command = command;
            if (command is ListLocalTaskGraphsCommand)
            {
                object list = new LocalTaskGraphsResult(
                    "task_graphs", new WindowsLocalTaskGraphPage([], null, null));
                return Task.FromResult((TResponse)list);
            }
            if (command is GetLocalMessageTaskGraphCommand)
            {
                object messageGraph = new LocalMessageTaskGraphTypedResult(
                    "message_task_graph", new WindowsLocalMessageTaskGraph(
                        [], [], [], "conversation-1", "turn-1", "message-1"));
                return Task.FromResult((TResponse)messageGraph);
            }
            object response = new LocalTaskGraphTypedResult("task_graph", new WindowsLocalTaskGraph(
                "graph-1", "owner-1", "conversation_turn", "turn-1", "running", [], [], 1));
            return Task.FromResult((TResponse)response);
        }
    }

    private sealed class AcceptanceHost : ILocalAgentHostClient
    {
        private const string PlanMarkdown = "# Frozen proposal\n\nInspect safely.";
        private static readonly string PlanHash = Convert.ToHexString(
            SHA256.HashData(Encoding.UTF8.GetBytes(PlanMarkdown))).ToLowerInvariant();
        private static readonly JsonElement Input = JsonSerializer.SerializeToElement(new
        {
            objective = "Inspect",
            thinking_level = "high",
            source_conversation_id = "conversation-1",
            acceptance_report_version = 1,
            acceptance_criteria = new[] { "Inspect the artifact" },
            final_response_max_chars = 12,
            requirement_plan = new
            {
                snapshot = new
                {
                    plan_id = "plan-accept",
                    revision = 2UL,
                    title = "Frozen inspection proposal",
                    markdown = PlanMarkdown,
                    confirmed_at_unix_ms = 1234L,
                },
                markdown_sha256 = PlanHash,
                task_scope = new
                {
                    title = "Acceptance task",
                    prompt = "Inspect the frozen artifact",
                    expected_outputs = new[] { "inspection report" },
                    acceptance_criteria = new[] { "evidence is attached" },
                },
            },
        });
        private static readonly JsonElement Outcome = JsonSerializer.SerializeToElement(new
        {
            content = "完成😀",
            task_acceptance = new
            {
                status = "failed",
                reason = "The artifact could not be opened",
                final_response = "完成😀",
                final_response_chars = 999,
                acceptance_checks = new[]
                {
                    new
                    {
                        criterion_index = 0,
                        status = "not_verified",
                        evidence = "The file picker did not return an artifact",
                    },
                },
            },
        });
        private static readonly WindowsLocalTask TaskRecord = new(
            "graph-accept", "owner-1", "conversation_turn", "turn-1", "task-accept",
            "Acceptance task", "model-1", Input, "failed", "run-accept", 4, 1, 2)
        {
            ModelConfigRevision = "revision-1",
        };
        private static readonly WindowsLocalAgentRun RunRecord = new(
            "run-accept", "owner-1", "task", "task-accept", "task_execution",
            JsonSerializer.SerializeToElement(new { }), "succeeded", 3, Outcome, 1, 2);

        public string? ActiveOwnerUserId => "owner-1";
        public Task StartForOwnerAsync(string ownerUserId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> environment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            object response = command switch
            {
                GetLocalRunCommand => new GetLocalRunResult("run", RunRecord),
                GetLocalMessageTaskGraphCommand => new LocalMessageTaskGraphTypedResult(
                    "message_task_graph", new WindowsLocalMessageTaskGraph(
                        ["task-accept"],
                        [new WindowsLocalMessageTaskGraphNode(TaskRecord, 0, true, true)],
                        [], "conversation-1", "turn-1", "message-1")),
                GetLocalTaskRunsCommand => new LocalTaskRunsResult(
                    "task_runs", "task-accept", [RunRecord]),
                ListLocalEventsCommand => new ListLocalEventsResult("events", [], 0),
                WindowsLocalAgentControlPlaneClient.GetModelCommand =>
                    new WindowsLocalAgentControlPlaneClient.ModelResult(
                        "model_config_snapshot",
                        new WindowsLocalAgentModelSnapshot(
                            "owner-1", "model-1", "revision-1", "env:MODEL_1",
                            "https://example.invalid/v1", "gpt-6.1-sol", "openai",
                            true, false, null, null, null, "medium", false,
                            null, null, null)),
                _ => throw new NotSupportedException(command.GetType().Name),
            };
            return Task.FromResult((TResponse)response);
        }
    }

    private sealed class EmptyMessageGraphFallbackHost : ILocalAgentHostClient
    {
        private readonly WindowsLocalTask _task = new(
            "graph-1", "owner-1", "conversation_turn", "turn-1", "task-1",
            "Acceptance task", "model-1",
            JsonSerializer.SerializeToElement(new
            {
                source_conversation_id = "conversation-1",
                source_turn_id = "turn-1",
                objective = "Wait for cancellation",
            }),
            "running", "run-1", 7, 1, 2);

        public List<object> Commands { get; } = [];
        public string? ActiveOwnerUserId => "owner-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> environment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            Commands.Add(command);
            object response = command switch
            {
                GetLocalMessageTaskGraphCommand => new LocalMessageTaskGraphTypedResult(
                    "message_task_graph", new WindowsLocalMessageTaskGraph(
                        [], [], [], "conversation-1", "turn-1", "message-1")),
                ListLocalTaskGraphsCommand => new LocalTaskGraphsResult(
                    "task_graphs", new WindowsLocalTaskGraphPage(
                        [new WindowsLocalTaskGraphSummary(
                            "graph-1", "conversation_turn", "turn-1", 2)],
                        null, null)),
                GetLocalTaskGraphCommand => new LocalTaskGraphTypedResult(
                    "task_graph", new WindowsLocalTaskGraph(
                        "graph-1", "owner-1", "conversation_turn", "turn-1",
                        "running", [_task], [], 1)),
                GetLocalRunCommand => new GetLocalRunResult(
                    "run", new WindowsLocalAgentRun(
                        "run-1", "owner-1", "task", "task-1", "task_execution",
                        JsonSerializer.SerializeToElement(new { }), "failed", 3,
                        JsonSerializer.SerializeToElement(new { error = "provider rejected request" }),
                        1, 2)),
                GetLocalTaskRunsCommand => new LocalTaskRunsResult(
                    "task_runs", "task-1",
                    [new WindowsLocalAgentRun(
                        "run-1", "owner-1", "task", "task-1", "task_execution",
                        JsonSerializer.SerializeToElement(new { }), "failed", 3,
                        JsonSerializer.SerializeToElement(new { error = "provider rejected request" }),
                        1, 2)]),
                CancelLocalTaskCommand => new LocalTaskGraphTypedResult(
                    "task_graph", new WindowsLocalTaskGraph(
                        "graph-1", "owner-1", "conversation_turn", "turn-1",
                        "cancelled", [_task with { Status = "cancelled", Version = 8 }], [], 1)),
                RetryLocalTaskCommand => new LocalTaskGraphTypedResult(
                    "task_graph", new WindowsLocalTaskGraph(
                        "graph-1", "owner-1", "conversation_turn", "turn-1",
                        "running", [_task with { Status = "running", ActiveRunId = "run-2" }], [], 2)),
                _ => throw new NotSupportedException(command.GetType().Name),
            };
            return Task.FromResult((TResponse)response);
        }
    }

    private sealed class DependencyMessageGraphHost : ILocalAgentHostClient
    {
        private static readonly WindowsLocalTask Prerequisite = new(
            "graph-1", "owner-1", "conversation_turn", "turn-1", "task-a",
            "Prepare inputs", "model-1", JsonSerializer.SerializeToElement(new { }),
            "running", "run-a", 3, 1, 3);

        private static readonly WindowsLocalTask Dependent = new(
            "graph-1", "owner-1", "conversation_turn", "turn-1", "task-b",
            "Use inputs", "model-1", JsonSerializer.SerializeToElement(new { }),
            "pending", null, 1, 2, 2);

        public string? ActiveOwnerUserId => "owner-1";

        public Task StartForOwnerAsync(string ownerUserId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> environment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            if (command is not GetLocalMessageTaskGraphCommand)
                throw new NotSupportedException(command.GetType().Name);
            object response = new LocalMessageTaskGraphTypedResult(
                "message_task_graph",
                new WindowsLocalMessageTaskGraph(
                    ["task-a"],
                    [
                        new WindowsLocalMessageTaskGraphNode(Prerequisite, 0, true, true),
                        new WindowsLocalMessageTaskGraphNode(Dependent, 1, false, true),
                    ],
                    [new WindowsLocalMessageTaskGraphEdge("task-a", "task-b", "prerequisite")],
                    "conversation-1", "turn-1", "message-1"));
            return Task.FromResult((TResponse)response);
        }
    }
}
