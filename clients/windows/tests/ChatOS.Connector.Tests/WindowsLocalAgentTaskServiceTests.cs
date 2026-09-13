using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentTaskServiceTests
{
    private static readonly DateTimeOffset Now = DateTimeOffset.Parse("2026-09-13T00:00:00Z");
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.SnakeCaseLower) },
    };

    [Fact]
    public async Task GraphUsesRestoredAccountAndExactSourceIdentity()
    {
        var run = Run("run-1", LocalAgentRunStatus.ModelRunning, 4);
        var task = TaskSnapshot("run-1", ["run-1"]);
        var client = new TaskClient(task, new Dictionary<string, LocalAgentRunSnapshot>
        {
            [run.RunId] = run,
        });
        var account = new TaskAccountSession(client);
        var service = new WindowsLocalAgentTaskService(await StoreAsync(task, run), account);

        var graph = await service.GetGraphAsync("thread-1", "turn-1");

        Assert.Equal("user-1", account.RequestedAccountId);
        Assert.Equal(("thread-1", "turn-1"), client.GraphSource);
        Assert.Equal("task-1", Assert.Single(graph.Nodes).Task.Task.TaskId);
    }

    [Fact]
    public async Task RetryPreservesFrozenProjectExecutionAndHistoricalRun()
    {
        var previous = Run("run-1", LocalAgentRunStatus.Failed, 4);
        var next = previous with
        {
            RunId = "run-2",
            Status = LocalAgentRunStatus.Queued,
            Version = 1,
        };
        var before = TaskSnapshot("run-1", ["run-1"]);
        var after = before with
        {
            Revision = 2,
            CurrentRunId = "run-2",
            RunIds = ["run-1", "run-2"],
        };
        var client = new TaskClient(before, new Dictionary<string, LocalAgentRunSnapshot>
        {
            [previous.RunId] = previous,
            [next.RunId] = next,
        })
        {
            RetryResponse = new LocalAgentRunCreatedResponse("operation-2", next),
            TaskAfterRetry = after,
        };
        var service = new WindowsLocalAgentTaskService(await StoreAsync(before, previous),
            new TaskAccountSession(client));

        var created = await service.RetryCurrentRunAsync("task-1", "run-1", "try again");

        Assert.Equal("run-2", created.Run.RunId);
        Assert.Equal(new LocalAgentRetryTask("task-1", "run-1", "try again"), client.RetryCommand);
        Assert.Equal("project-1", created.Run.ProjectId);
        Assert.Equal(["run-1", "run-2"], client.TaskAfterRetry?.RunIds);
    }

    [Fact]
    public async Task RetryRejectsAnyFrozenProjectChange()
    {
        var previous = Run("run-1", LocalAgentRunStatus.Failed, 4);
        var changed = previous with
        {
            RunId = "run-2",
            ProjectId = "project-other",
            Status = LocalAgentRunStatus.Queued,
            Version = 1,
        };
        var task = TaskSnapshot("run-1", ["run-1"]);
        var client = new TaskClient(task, new Dictionary<string, LocalAgentRunSnapshot>
        {
            [previous.RunId] = previous,
        })
        {
            RetryResponse = new LocalAgentRunCreatedResponse("operation-2", changed),
        };
        var service = new WindowsLocalAgentTaskService(await StoreAsync(task, previous),
            new TaskAccountSession(client));

        await Assert.ThrowsAsync<InvalidDataException>(() =>
            service.RetryCurrentRunAsync("task-1", "run-1", null));
    }

    private static async Task<WindowsLocalAgentProjectionStore> StoreAsync(
        LocalAgentTaskSnapshot task,
        LocalAgentRunSnapshot run)
    {
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "user-1",
            new Dictionary<string, WindowsLocalAgentRecoveredRun>
            {
                [run.RunId] = new(run, null, null),
            },
            new Dictionary<string, LocalAgentTaskSnapshot>
            {
                [task.TaskId] = task,
            },
            0,
            0));
        return store;
    }

    private static LocalAgentTaskSnapshot TaskSnapshot(
        string currentRunId,
        IReadOnlyList<string> runIds) => new(
        "task-1", 1, "thread-1", "turn-1", "project-1", "run-1", currentRunId,
        runIds, "Build it", ["Done"], "running", "model-1", 3, Now, Now);

    private static LocalAgentRunSnapshot Run(
        string id,
        LocalAgentRunStatus status,
        ulong version) => new(
        id, "task_runner", "user-1", "task", "task-1", "project-1", status,
        version, 0, 0, 0, "model-1", 3, EmptyJson(), "provider_compaction",
        "prompt-1", "capability-1", null, null, null, null, Now, Now);

    private static JsonElement EmptyJson() => JsonDocument.Parse("{}").RootElement.Clone();

    private sealed class TaskAccountSession(ILocalAgentIPCClient client)
        : IWindowsLocalAgentAccountSession
    {
        public string? RequestedAccountId { get; private set; }
        public Task<ILocalAgentIPCClient> GetClientAsync(string accountId,
            CancellationToken cancellationToken = default)
        {
            RequestedAccountId = accountId;
            return Task.FromResult(client);
        }
        public Task ActivateAsync(string accountId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
        public Task UpdateAccessTokenAsync(string accountId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task LogoutAsync() => Task.CompletedTask;
        public Task<WindowsLocalAgentHostState> GetStateAsync() => throw new NotSupportedException();
        public Task<IReadOnlyList<LocalAgentAttachmentReference>> StageAttachmentsAsync(
            string accountId, IReadOnlyList<ConversationAttachmentDraft> attachments,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DiscardStagedAttachmentsAsync(
            string accountId, IReadOnlyList<LocalAgentAttachmentReference> references) =>
            throw new NotSupportedException();
        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }

    private sealed class TaskClient(
        LocalAgentTaskSnapshot task,
        IReadOnlyDictionary<string, LocalAgentRunSnapshot> runs) : LocalAgentIPCClientStub
    {
        public (string ThreadId, string TurnId)? GraphSource { get; private set; }
        public LocalAgentRetryTask? RetryCommand { get; private set; }
        public LocalAgentRunCreatedResponse? RetryResponse { get; init; }
        public LocalAgentTaskSnapshot? TaskAfterRetry { get; init; }
        public LocalAgentCommand? AcceptedCommand { get; private set; }

        public override Task<LocalAgentTaskGraphSnapshot> GetTaskGraphAsync(
            string sourceThreadId,
            string sourceTurnId,
            CancellationToken cancellationToken = default)
        {
            GraphSource = (sourceThreadId, sourceTurnId);
            var run = runs[task.CurrentRunId];
            return Task.FromResult(new LocalAgentTaskGraphSnapshot(
                sourceThreadId,
                sourceTurnId,
                [task.TaskId],
                [new LocalAgentTaskGraphNode(
                    new LocalAgentTaskProjection(
                        task,
                        new LocalAgentTaskRunSummary(run, null, null, null)),
                    0,
                    true)],
                []));
        }

        public override Task<LocalAgentTaskSnapshot> GetTaskAsync(string taskId,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(RetryCommand is null ? task : TaskAfterRetry ?? task);

        public override Task<LocalAgentTaskRunDetail> GetTaskRunDetailAsync(
            string taskId,
            string runId,
            uint eventLimit = 40,
            uint eventOffset = 0,
            CancellationToken cancellationToken = default)
        {
            var run = runs[runId];
            return Task.FromResult(new LocalAgentTaskRunDetail(
                task,
                new LocalAgentTaskRunSummary(run, null, null, null),
                [],
                0,
                false));
        }

        public override Task<LocalAgentRunCreatedResponse> RetryTaskAsync(
            LocalAgentRetryTask command,
            CancellationToken cancellationToken = default)
        {
            RetryCommand = command;
            return Task.FromResult(RetryResponse ?? throw new InvalidOperationException());
        }

        public override Task<string> AcceptAsync(LocalAgentCommand command,
            CancellationToken cancellationToken = default)
        {
            AcceptedCommand = command;
            return Task.FromResult("operation-1");
        }
    }

}
