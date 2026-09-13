using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentPetActivityServiceTests
{
    private static readonly DateTimeOffset Now = DateTimeOffset.Parse("2026-09-13T08:00:00Z");

    [Fact]
    public async Task Projects_main_chat_current_task_ask_tool_and_review_with_frozen_routes()
    {
        var main = Run("main-1", "main_chat", "conversation", "thread-1",
            LocalAgentRunStatus.ModelRunning, "project-main");
        var taskRun = Run("task-run-1", "task_runner", "task", "task-1",
            LocalAgentRunStatus.ModelRunning, "project-task");
        var ask = Run("ask-run", "main_chat", "conversation", "thread-ask",
            LocalAgentRunStatus.Paused, "project-ask",
            PendingAsk("interaction-1"));
        var tool = Run("tool-run", "main_chat", "conversation", "thread-tool",
            LocalAgentRunStatus.WaitingToolResult, "project-tool");
        var review = Run("review-run", "main_chat", "conversation", "thread-review",
            LocalAgentRunStatus.NeedsReview, "project-review",
            Json("""{"type":"review_unknown_tool_outcome","batch_id":"batch-1"}"""));
        var historical = Run("task-old", "task_runner", "task", "task-1",
            LocalAgentRunStatus.Succeeded, "project-task");
        var task = new LocalAgentTaskSnapshot(
            "task-1", 2, "thread-task", "turn-task", "project-task",
            "task-old", "task-run-1", ["task-old", "task-run-1"], "Design page", ["Looks good"],
            "running", "model-1", 1, Now, Now);
        var projection = Projection(
            [
                Recovered(main, Binding(main, "thread-1", "turn-1", "message-1")),
                Recovered(taskRun),
                Recovered(ask, Binding(ask, "thread-ask", "turn-ask", "message-ask")),
                Recovered(tool, Binding(tool, "thread-tool", "turn-tool", "message-tool"),
                    [Tool(tool, "invocation-1")]),
                Recovered(review, Binding(review, "thread-review", "turn-review", "message-review")),
                Recovered(historical),
            ],
            [task]);
        var store = await StoreAsync(projection);
        using var service = new WindowsLocalAgentPetActivityService(store, new Suppressions());

        var activities = await service.FetchAsync();

        Assert.Equal(5, activities.Count);
        var mainActivity = activities.Single(value => value.Id == "local-run:main-1");
        Assert.Equal("project-main", mainActivity.Route.ProjectId);
        Assert.Equal("thread-1", mainActivity.Route.ConversationId);
        Assert.Equal("turn-1", mainActivity.Route.TurnId);
        Assert.Equal("message-1", mainActivity.Route.MessageId);
        var taskActivity = activities.Single(value =>
            value.Id == "local-task:task-1:task-run-1");
        Assert.Equal("project-task", taskActivity.Route.ProjectId);
        Assert.Equal("thread-task", taskActivity.Route.ConversationId);
        Assert.Equal("turn-task", taskActivity.Route.TurnId);
        Assert.Equal("task-1", taskActivity.Route.TaskId);
        Assert.DoesNotContain(activities, value => value.Route.RunId == "task-old");
        var askActivity = activities.Single(value => value.Id == "local-ask:interaction-1");
        Assert.Equal(PetActivityKind.WaitingForUser, askActivity.Kind);
        Assert.Equal("interaction-1", askActivity.Route.PromptId);
        var toolActivity = activities.Single(value => value.Id == "local-tool:invocation-1");
        Assert.Equal(PetActivitySource.LocalAgentToolApproval, toolActivity.Source);
        Assert.Equal("invocation-1", toolActivity.Route.InvocationId);
        var reviewActivity = activities.Single(value => value.Id == "local-run:review-run");
        Assert.Equal(PetActivityKind.NeedsReview, reviewActivity.Kind);
        Assert.Contains("batch-1", reviewActivity.Detail);
    }

    [Fact]
    public async Task Suppression_is_local_and_a_new_run_version_reappears()
    {
        var suppression = new Suppressions();
        var run = Run("run-1", "main_chat", "conversation", "thread-1",
            LocalAgentRunStatus.Failed, "project-1");
        var store = await StoreAsync(Projection(
            [Recovered(run, Binding(run, "thread-1", "turn-1", "message-1"))], []));
        using var service = new WindowsLocalAgentPetActivityService(store, suppression);
        var activity = Assert.Single(await service.FetchAsync());

        await service.SuppressAsync(activity, PetActivityDisposition.Ignored);
        Assert.Empty(await service.FetchAsync());
        Assert.Single(suppression.Values);

        var newer = run with { Version = 2, UpdatedAt = Now.AddMinutes(1) };
        await store.ReplaceAuthoritativeRunAsync("user-1",
            Recovered(newer, Binding(newer, "thread-1", "turn-1", "message-1")));
        var visible = Assert.Single(await service.FetchAsync());
        Assert.Equal("run-version:2", visible.ActivityVersion);
    }

    [Fact]
    public async Task Projection_change_and_clear_emit_local_change_notifications()
    {
        var run = Run("run-1", "main_chat", "conversation", "thread-1",
            LocalAgentRunStatus.ModelRunning, null);
        var store = await StoreAsync(Projection(
            [Recovered(run, Binding(run, "thread-1", "turn-1", "message-1"))], []));
        using var service = new WindowsLocalAgentPetActivityService(store, new Suppressions());
        var changes = 0;
        service.Changed += (_, _) => changes++;

        await store.ReplaceAuthoritativeRunAsync("user-1", Recovered(
            run with { Version = 2, UpdatedAt = Now.AddSeconds(1) },
            Binding(run with { Version = 2 }, "thread-1", "turn-1", "message-1")));
        await store.ResetAsync();

        Assert.Equal(2, changes);
        Assert.Empty(await service.FetchAsync());
    }

    [Fact]
    public async Task Duplicate_interaction_identity_fails_closed()
    {
        // Unique Run IDs cannot make a duplicated durable interaction identity valid.
        var first = Run("run-1", "main_chat", "conversation", "thread-1",
            LocalAgentRunStatus.Paused, null, PendingAsk("interaction-1"));
        var second = Run("run-2", "main_chat", "conversation", "thread-2",
            LocalAgentRunStatus.Paused, null, PendingAsk("interaction-1"));
        var store = await StoreAsync(Projection(
            [
                Recovered(first, Binding(first, "thread-1", "turn-1", "message-1")),
                Recovered(second, Binding(second, "thread-2", "turn-2", "message-2")),
            ], []));
        using var service = new WindowsLocalAgentPetActivityService(store, new Suppressions());

        await Assert.ThrowsAsync<InvalidDataException>(() => service.FetchAsync());
    }

    private static WindowsLocalAgentProjectionSnapshot Projection(
        IReadOnlyList<WindowsLocalAgentRecoveredRun> runs,
        IReadOnlyList<LocalAgentTaskSnapshot> tasks) => new(
        "user-1",
        runs.ToDictionary(value => value.Run.RunId, StringComparer.Ordinal),
        tasks.ToDictionary(value => value.TaskId, StringComparer.Ordinal),
        0,
        0);

    private static async Task<WindowsLocalAgentProjectionStore> StoreAsync(
        WindowsLocalAgentProjectionSnapshot projection)
    {
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(projection);
        return store;
    }

    private static WindowsLocalAgentRecoveredRun Recovered(
        LocalAgentRunSnapshot run,
        LocalAgentMainChatRunBinding? binding = null,
        IReadOnlyList<LocalAgentToolSnapshot>? tools = null) => new(
        run,
        new LocalAgentRunDetail(run, [], tools ?? [], 0, false, 0),
        binding,
        0);

    private static LocalAgentRunSnapshot Run(
        string runId,
        string profile,
        string ownerType,
        string ownerId,
        LocalAgentRunStatus status,
        string? projectId,
        JsonElement? interaction = null) => new(
        runId, profile, "user-1", ownerType, ownerId, projectId, status, 1, 0, 2, 0,
        "model-1", 1, Json("{}"), "memory_engine", "prompt-1", "capability-1",
        null, interaction, status == LocalAgentRunStatus.Failed
            ? Json("""{"error_message":"failed"}""") : null,
        null, Now, Now);

    private static LocalAgentMainChatRunBinding Binding(
        LocalAgentRunSnapshot run,
        string threadId,
        string turnId,
        string messageId) => new(
        run.RunId, threadId, turnId, messageId,
        new LocalAgentStoredMessage(
            messageId, run.RunId, threadId, turnId, 1,
            LocalAgentStoredMessageRole.User, "hello", null, null, null, null,
            LocalAgentStoredMessageMode.Semantic, "main_chat",
            LocalAgentStoredMemorySyncStatus.Synced,
            Now));

    private static LocalAgentToolSnapshot Tool(
        LocalAgentRunSnapshot run,
        string invocationId) => new(
        invocationId, run.RunId, "batch-1", "call-1", "filesystem.write",
        LocalAgentToolEffect.Write, "sha256:0123456789abcdef",
        LocalAgentToolExecutionStatus.AwaitingApproval, null, null, null, null, null);

    private static JsonElement PendingAsk(string interactionId) => Json($$"""
        {
          "type": "ask_user",
          "interaction_id": "{{interactionId}}",
          "question": {
            "prompt": "Choose a layout",
            "options": [],
            "image_references": ["attachment-grant:image-1"],
            "details": {
              "title": "Choose layout",
              "kind": "design_review",
              "allows_cancel": true,
              "allows_multiple": false
            }
          }
        }
        """);

    private static JsonElement Json(string json) =>
        JsonDocument.Parse(json).RootElement.Clone();

    private sealed class Suppressions : IPetActivitySuppressionStore
    {
        public HashSet<string> Values { get; } = new(StringComparer.Ordinal);
        public Task<bool> IsSuppressedAsync(string stableIdentity, DateTimeOffset now,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(Values.Contains(stableIdentity));
        public Task SuppressAsync(string stableIdentity, PetActivityDisposition disposition,
            DateTimeOffset suppressedAt, DateTimeOffset? expiresAt,
            CancellationToken cancellationToken = default)
        {
            Values.Add(stableIdentity);
            return Task.CompletedTask;
        }
        public Task RemoveAsync(string stableIdentity,
            CancellationToken cancellationToken = default)
        {
            Values.Remove(stableIdentity);
            return Task.CompletedTask;
        }
        public Task PruneExpiredAsync(DateTimeOffset now,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
    }
}
