using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentRecoveryTests
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.SnakeCaseLower) },
    };

    [Fact]
    public async Task RecoveryAtomicallyRestoresMainChatTaskRunsAndCursor()
    {
        var main = Run("main", "main_chat", "conversation", "thread-1", null);
        var taskRun = Run("task-run", "task_runner", "task", "task-1", "project-1");
        var task = TaskSnapshot(taskRun);
        var client = new RecoveryClient(main, taskRun, task);
        var store = new WindowsLocalAgentProjectionStore();
        var recovery = new WindowsLocalAgentStartupRecovery(store);

        await recovery.RestoreAsync("user-1", client);

        var snapshot = Assert.IsType<WindowsLocalAgentProjectionSnapshot>(await store.GetAsync());
        Assert.Equal((ulong)9, snapshot.AcknowledgedEventSequence);
        Assert.Equal(["main", "task-run"], snapshot.Runs.Keys.Order(StringComparer.Ordinal));
        Assert.Single(snapshot.Tasks);
        var recoveredMain = snapshot.Runs[main.RunId];
        Assert.Collection(
            Assert.IsAssignableFrom<IReadOnlyList<LocalAgentRunTimelineEvent>>(
                recoveredMain.Detail?.Events),
            item => Assert.Equal("event-1", item.EventId),
            item => Assert.Equal("event-2", item.EventId));
        Assert.Equal("turn-1", recoveredMain.MainChatBinding?.TurnId);
    }

    [Fact]
    public async Task RecoveryRejectsTaskRunThatDoesNotPreserveFrozenProject()
    {
        var run = Run("task-run", "task_runner", "task", "task-1", "project-wrong");
        var task = TaskSnapshot(run) with { ProjectId = "project-1" };
        var client = new RecoveryClient(null, run, task);
        var store = new WindowsLocalAgentProjectionStore();

        await Assert.ThrowsAsync<InvalidDataException>(() =>
            new WindowsLocalAgentStartupRecovery(store).RestoreAsync("user-1", client));

        Assert.Null(await store.GetAsync());
    }

    [Fact]
    public async Task EventDrainUsesCurrentClientAppliesPageThenAcknowledges()
    {
        var original = Run("task-run", "task_runner", "task", "task-1", "project-1");
        var updated = original with { Status = LocalAgentRunStatus.ModelRunning, Version = 2 };
        var task = TaskSnapshot(updated);
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "user-1",
            new Dictionary<string, WindowsLocalAgentRecoveredRun>
            {
                [original.RunId] = new(original, null, null),
            },
            new Dictionary<string, LocalAgentTaskSnapshot> { [task.TaskId] = task },
            4,
            4));
        var first = new EventClient(4, []);
        var second = new EventClient(4,
        [
            new LocalAgentUIEvent(
                5,
                DateTimeOffset.Parse("2026-09-13T00:00:00Z"),
                new LocalAgentTaggedValue(
                    "run_snapshot",
                    JsonSerializer.SerializeToElement(updated, JsonOptions))),
        ]) { CurrentTask = task, CurrentRun = updated };
        var account = new SwappingAccountSession(first, second);
        var hub = new WindowsLocalAgentEventHub(account, store);

        var empty = await hub.DrainAvailableAsync("user-1");
        var applied = await hub.DrainAvailableAsync("user-1");

        Assert.Equal(0, empty.AppliedEventCount);
        Assert.Equal(1, applied.AppliedEventCount);
        Assert.Equal((ulong)5, applied.AcknowledgedSequence);
        Assert.Equal([(ulong)5], second.Acknowledgements);
        var snapshot = Assert.IsType<WindowsLocalAgentProjectionSnapshot>(await store.GetAsync());
        Assert.Equal(LocalAgentRunStatus.ModelRunning, snapshot.Runs[updated.RunId].Run.Status);
        Assert.Equal("fresh", Assert.Single(snapshot.Runs[updated.RunId].Detail!.Events).Message);
        Assert.Equal((ulong)5, snapshot.Runs[updated.RunId].SnapshotEventSequence);
        Assert.Equal((ulong)5, snapshot.AcknowledgedEventSequence);
        Assert.Equal(2, account.ClientRequests);
    }

    [Fact]
    public async Task InvalidEventPageIsNeverAcknowledged()
    {
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "user-1",
            new Dictionary<string, WindowsLocalAgentRecoveredRun>(),
            new Dictionary<string, LocalAgentTaskSnapshot>(),
            4,
            4));
        var client = new EventClient(4,
        [
            new LocalAgentUIEvent(
                5,
                DateTimeOffset.Parse("2026-09-13T00:00:00Z"),
                new LocalAgentTaggedValue("unknown_event", EmptyJson())),
        ]);
        var hub = new WindowsLocalAgentEventHub(new SwappingAccountSession(client), store);

        await Assert.ThrowsAsync<InvalidDataException>(() =>
            hub.DrainAvailableAsync("user-1"));

        Assert.Empty(client.Acknowledgements);
        var snapshot = Assert.IsType<WindowsLocalAgentProjectionSnapshot>(await store.GetAsync());
        Assert.Equal((ulong)4, snapshot.AcknowledgedEventSequence);
    }

    [Fact]
    public async Task ClientRuntimeFailsClosedWhenRecoveryFails()
    {
        var calls = new List<string>();
        var account = new RuntimeAccountSession(calls);
        var recovery = new RuntimeRecovery(calls)
        {
            Error = new InvalidDataException("invalid snapshot"),
        };
        var events = new RuntimeEventHub(calls);
        var store = new RecordingProjectionStore(calls);
        var runtime = new WindowsLocalAgentClientRuntime(account, recovery, events, store);

        await Assert.ThrowsAsync<InvalidDataException>(() => runtime.ActivateAsync("user-1"));

        Assert.Equal(
            ["events.stop", "store.reset", "account.activate", "account.client",
             "recovery.restore", "events.stop", "store.reset", "account.logout"],
            calls);
        Assert.Equal(1, account.LogoutCount);
        Assert.Equal(0, events.StartCount);
    }

    [Fact]
    public async Task ClientRuntimeRetriesWholeRecoveryOnReplacementEndpoint()
    {
        var calls = new List<string>();
        var account = new RestartingRuntimeAccountSession();
        var recovery = new FlakyRuntimeRecovery();
        var events = new RuntimeEventHub(calls);
        var store = new RecordingProjectionStore(calls);
        var runtime = new WindowsLocalAgentClientRuntime(
            account, recovery, events, store, TimeSpan.Zero, 3);

        await runtime.ActivateAsync("user-1");

        Assert.Equal(2, recovery.Attempts);
        Assert.Equal(2, account.ClientRequests);
        Assert.Equal(1, events.StartCount);
        Assert.Equal(["pipe-1", "pipe-2"], account.ObservedEndpoints);
    }

    private static LocalAgentRunSnapshot Run(
        string id,
        string profile,
        string ownerType,
        string ownerId,
        string? projectId) => new(
        id, profile, "user-1", ownerType, ownerId, projectId,
        LocalAgentRunStatus.Queued, 1, 0, 0, 0,
        "model-1", 1, EmptyJson(), "memory_engine", "prompt-1", "capability-1",
        null, null, null, null,
        DateTimeOffset.Parse("2026-09-13T00:00:00Z"),
        DateTimeOffset.Parse("2026-09-13T00:00:00Z"));

    private static LocalAgentTaskSnapshot TaskSnapshot(LocalAgentRunSnapshot run) => new(
        run.OwnerEntityId, 1, "thread-1", "turn-1", run.ProjectId!, run.RunId, run.RunId,
        [run.RunId], "Build it", ["Done"], "queued", run.ModelConfigId,
        run.ModelConfigRevision, run.CreatedAt, run.UpdatedAt);

    private static LocalAgentMainChatRunBinding Binding(LocalAgentRunSnapshot run) => new(
        run.RunId,
        run.OwnerEntityId,
        "turn-1",
        "message-1",
        new LocalAgentStoredMessage(
            "message-1", run.RunId, run.OwnerEntityId, "turn-1", 1,
            LocalAgentStoredMessageRole.User, "hello", null, null, null, null,
            LocalAgentStoredMessageMode.Semantic, "main_chat",
            LocalAgentStoredMemorySyncStatus.Synced,
            run.CreatedAt));

    private static JsonElement EmptyJson() =>
        JsonDocument.Parse("{}").RootElement.Clone();

    private sealed class RecoveryClient(
        LocalAgentRunSnapshot? main,
        LocalAgentRunSnapshot taskRun,
        LocalAgentTaskSnapshot taskSnapshot) : StubClient
    {
        public override Task<LocalAgentRunPage> ListRunsAsync(string? cursor = null, uint limit = 100,
            CancellationToken cancellationToken = default) => Task.FromResult(
            new LocalAgentRunPage(main is null ? [taskRun] : [main, taskRun], null));

        public override Task<LocalAgentTaskPage> ListTasksAsync(string? cursor = null, uint limit = 100,
            CancellationToken cancellationToken = default) => Task.FromResult(
            new LocalAgentTaskPage([taskSnapshot], null));

        public override Task<LocalAgentRunDetail> GetRunDetailAsync(string runId, uint eventLimit = 40,
            uint eventOffset = 0, CancellationToken cancellationToken = default)
        {
            if (runId == taskRun.RunId)
            {
                return Task.FromResult(new LocalAgentRunDetail(
                    taskRun, [], [], 0, false, 9));
            }
            var run = main ?? throw new InvalidOperationException();
            var all = new[]
            {
                new LocalAgentRunTimelineEvent("event-1", "model", "one", run.CreatedAt),
                new LocalAgentRunTimelineEvent("event-2", "model", "two", run.CreatedAt),
            };
            return Task.FromResult(eventOffset == 0
                ? new LocalAgentRunDetail(run, [all[0]], [], 2, true, 8)
                : new LocalAgentRunDetail(run, [all[1]], [], 2, false, 9));
        }

        public override Task<LocalAgentMainChatRunBinding> GetMainChatRunBindingAsync(
            string runId, CancellationToken cancellationToken = default) =>
            Task.FromResult(Binding(main ?? throw new InvalidOperationException()));

        public override Task<ulong> GetUIEventCursorAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<ulong>(9);
    }

    private sealed class EventClient(ulong cursor, IReadOnlyList<LocalAgentUIEvent> events) : StubClient
    {
        public LocalAgentTaskSnapshot? CurrentTask { get; init; }
        public LocalAgentRunSnapshot? CurrentRun { get; init; }
        public List<ulong> Acknowledgements { get; } = [];
        public override Task<ulong> GetUIEventCursorAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(cursor);
        public override Task<LocalAgentEventPage> SubscribeRunEventsAsync(ulong afterSequence,
            uint limit = 200, CancellationToken cancellationToken = default) => Task.FromResult(
            events.Count == 0
                ? new LocalAgentEventPage([], afterSequence, false)
                : new LocalAgentEventPage(events, events[^1].EventSeq, false));
        public override Task<LocalAgentTaskSnapshot> GetTaskAsync(string taskId,
            CancellationToken cancellationToken = default) => Task.FromResult(CurrentTask!);
        public override Task<LocalAgentRunDetail> GetRunDetailAsync(
            string runId,
            uint eventLimit = 40,
            uint eventOffset = 0,
            CancellationToken cancellationToken = default)
        {
            var run = CurrentRun ?? throw new InvalidOperationException();
            return Task.FromResult(new LocalAgentRunDetail(
                run,
                [new LocalAgentRunTimelineEvent("event-current", "model", "fresh", run.UpdatedAt)],
                [],
                1,
                false,
                5));
        }
        public override Task<ulong> AcknowledgeUIEventsAsync(ulong throughSequence,
            CancellationToken cancellationToken = default)
        {
            Acknowledgements.Add(throughSequence);
            return Task.FromResult(throughSequence);
        }
    }

    private sealed class SwappingAccountSession(params ILocalAgentIPCClient[] clients)
        : IWindowsLocalAgentAccountSession
    {
        private int _index;
        public int ClientRequests { get; private set; }
        public Task<ILocalAgentIPCClient> GetClientAsync(string accountId,
            CancellationToken cancellationToken = default)
        {
            ClientRequests++;
            return Task.FromResult(clients[Math.Min(_index++, clients.Length - 1)]);
        }
        public Task ActivateAsync(string accountId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
        public Task UpdateAccessTokenAsync(string accountId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task LogoutAsync() => Task.CompletedTask;
        public Task<WindowsLocalAgentHostState> GetStateAsync() => Task.FromResult(
            new WindowsLocalAgentHostState(
                WindowsLocalAgentHostStatus.Running,
                "user-1",
                1,
                "pipe-1"));
        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }

    private abstract class StubClient : ILocalAgentIPCClient
    {
        public virtual Task<LocalAgentResponse> SendAsync(LocalAgentCommand command,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public virtual Task<string> AcceptAsync(LocalAgentCommand command,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public virtual Task<LocalAgentRunCreatedResponse> CreateMainChatTurnAsync(
            LocalAgentCreateMainChatTurn command, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
        public virtual Task<LocalAgentRunCreatedResponse> CreateTaskAsync(LocalAgentCreateTask command,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public virtual Task<LocalAgentRunCreatedResponse> RetryTaskAsync(LocalAgentRetryTask command,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public virtual Task<LocalAgentRunSnapshot> GetRunAsync(string runId,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public virtual Task<LocalAgentRunDetail> GetRunDetailAsync(string runId, uint eventLimit = 40,
            uint eventOffset = 0, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
        public virtual Task<LocalAgentTaskSnapshot> GetTaskAsync(string taskId,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public virtual Task<LocalAgentTaskGraphSnapshot> GetTaskGraphAsync(string sourceThreadId,
            string sourceTurnId, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
        public virtual Task<LocalAgentTaskRunDetail> GetTaskRunDetailAsync(string taskId, string runId,
            uint eventLimit = 40, uint eventOffset = 0,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public virtual Task<LocalAgentMainChatRunBinding> GetMainChatRunBindingAsync(string runId,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public virtual Task<LocalAgentRunPage> ListRunsAsync(string? cursor = null, uint limit = 100,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public virtual Task<LocalAgentTaskPage> ListTasksAsync(string? cursor = null, uint limit = 100,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public virtual Task<LocalAgentEventPage> SubscribeRunEventsAsync(ulong afterSequence,
            uint limit = 200, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
        public virtual Task<ulong> GetUIEventCursorAsync(
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public virtual Task<ulong> AcknowledgeUIEventsAsync(ulong throughSequence,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }

    private sealed class RuntimeAccountSession(List<string> calls) : IWindowsLocalAgentAccountSession
    {
        public int LogoutCount { get; private set; }
        public Task ActivateAsync(string accountId, CancellationToken cancellationToken = default)
        { calls.Add("account.activate"); return Task.CompletedTask; }
        public Task UpdateAccessTokenAsync(string accountId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task LogoutAsync()
        { calls.Add("account.logout"); LogoutCount++; return Task.CompletedTask; }
        public Task<ILocalAgentIPCClient> GetClientAsync(string accountId,
            CancellationToken cancellationToken = default)
        { calls.Add("account.client"); return Task.FromResult<ILocalAgentIPCClient>(new EmptyClient()); }
        public Task<WindowsLocalAgentHostState> GetStateAsync() => Task.FromResult(
            new WindowsLocalAgentHostState(
                WindowsLocalAgentHostStatus.Running,
                "user-1",
                1,
                "pipe-1"));
        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }

    private sealed class EmptyClient : StubClient { }

    private sealed class RestartingRuntimeAccountSession : IWindowsLocalAgentAccountSession
    {
        private int _stateIndex;
        public int ClientRequests { get; private set; }
        public List<string> ObservedEndpoints { get; } = [];
        public Task ActivateAsync(string accountId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
        public Task UpdateAccessTokenAsync(string accountId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task LogoutAsync() => Task.CompletedTask;
        public Task<ILocalAgentIPCClient> GetClientAsync(string accountId,
            CancellationToken cancellationToken = default)
        {
            ClientRequests++;
            return Task.FromResult<ILocalAgentIPCClient>(new EmptyClient());
        }
        public Task<WindowsLocalAgentHostState> GetStateAsync()
        {
            var endpoint = _stateIndex++ == 0 ? "pipe-1" : "pipe-2";
            ObservedEndpoints.Add(endpoint);
            return Task.FromResult(new WindowsLocalAgentHostState(
                WindowsLocalAgentHostStatus.Running, "user-1", 1, endpoint));
        }
        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }

    private sealed class FlakyRuntimeRecovery : IWindowsLocalAgentStartupRecovery
    {
        public int Attempts { get; private set; }
        public Task RestoreAsync(string accountId, ILocalAgentIPCClient client,
            CancellationToken cancellationToken = default)
        {
            Attempts++;
            return Attempts == 1
                ? Task.FromException(new IOException("old endpoint closed"))
                : Task.CompletedTask;
        }
    }

    private sealed class RuntimeRecovery(List<string> calls) : IWindowsLocalAgentStartupRecovery
    {
        public Exception? Error { get; init; }
        public Task RestoreAsync(string accountId, ILocalAgentIPCClient client,
            CancellationToken cancellationToken = default)
        {
            calls.Add("recovery.restore");
            return Error is null ? Task.CompletedTask : Task.FromException(Error);
        }
    }

    private sealed class RuntimeEventHub(List<string> calls) : IWindowsLocalAgentEventHub
    {
        public int StartCount { get; private set; }
        public Task StartAsync(string accountId, CancellationToken cancellationToken = default)
        { calls.Add("events.start"); StartCount++; return Task.CompletedTask; }
        public Task StopAsync()
        { calls.Add("events.stop"); return Task.CompletedTask; }
        public Task<WindowsLocalAgentEventDrainResult> DrainAvailableAsync(string accountId,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }

    private sealed class RecordingProjectionStore(List<string> calls) : IWindowsLocalAgentProjectionStore
    {
        public event EventHandler<WindowsLocalAgentProjectionSnapshot>? Changed
        {
            add { }
            remove { }
        }
        public event EventHandler? Cleared
        {
            add { }
            remove { }
        }
        public Task ReplaceAsync(WindowsLocalAgentProjectionSnapshot snapshot,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task ApplyPageAsync(string accountId, IReadOnlyList<WindowsLocalAgentResolvedEvent> events,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task MarkAcknowledgedAsync(string accountId, ulong throughSequence,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<WindowsLocalAgentProjectionSnapshot?> GetAsync(
            CancellationToken cancellationToken = default) => Task.FromResult<WindowsLocalAgentProjectionSnapshot?>(null);
        public Task ResetAsync(CancellationToken cancellationToken = default)
        { calls.Add("store.reset"); return Task.CompletedTask; }
    }
}
