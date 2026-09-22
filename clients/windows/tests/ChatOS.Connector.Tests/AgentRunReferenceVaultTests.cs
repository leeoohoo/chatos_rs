using ChatOS.Connector.AgentTeams;
using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class AgentRunReferenceVaultTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(), "chatos-reference-tests", Guid.NewGuid().ToString("N"));
    private SqliteAgentTeamStore _store = null!;

    public async Task InitializeAsync()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        _store = new SqliteAgentTeamStore(database);
    }

    public Task DisposeAsync()
    {
        if (Directory.Exists(_directory)) Directory.Delete(_directory, recursive: true);
        return Task.CompletedTask;
    }

    [Fact]
    public void ReferencesAreStableInsideOneRunAndInvalidInAnotherRun()
    {
        var first = new AgentRunReferenceVault();
        var second = new AgentRunReferenceVault();
        var reference = first.AgentReference("durable-agent-id");

        Assert.StartsWith("agent_", reference, StringComparison.Ordinal);
        Assert.Equal(reference, first.AgentReference("durable-agent-id"));
        Assert.Equal("durable-agent-id", first.AgentId(reference));
        Assert.Null(second.AgentId(reference));
        Assert.Null(first.AgentId("durable-agent-id"));
    }

    [Fact]
    public async Task ToolResponsesExposeOnlyRunScopedReferencesAndRejectDurableIds()
    {
        var manager = await CreateAgentAsync("manager");
        var worker = await CreateAgentAsync("worker");
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("team", "deliver"), manager.Id);
        await _store.UpsertMemberAsync("alice", room.Id, worker.Id,
            new("developer", "implement"));
        await CompleteAllPendingAsync();
        var todo = await _store.CreateTodoAsync("alice",
            new(room.Id, worker.Id, "task"));
        var asset = await _store.UpsertAssetAsync("alice", room.Id, null, manager.Id,
            AgentTeamAssetCategory.Plan, "plan", "content", null);
        var posted = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "work", [manager.Id]));
        var delivery = Assert.IsType<AgentDelivery>(await _store.ClaimNextDeliveryAsync("alice"));
        var member = Assert.Single(await _store.ListMembersAsync("alice", room.Id), value =>
            value.AgentId == manager.Id);
        var executor = new AgentTeamToolExecutor(_store, null!);
        var references = new AgentRunReferenceVault();

        var members = await executor.ExecuteAsync(manager, member, room, delivery,
            new("members", "team_members", "{}"), CancellationToken.None, references);
        var todos = await executor.ExecuteAsync(manager, member, room, delivery,
            new("todos", "todo_list", "{}"), CancellationToken.None, references);
        var assets = await executor.ExecuteAsync(manager, member, room, delivery,
            new("assets", "asset_list", "{}"), CancellationToken.None, references);
        var inbox = await executor.ExecuteAsync(manager, member, room, delivery,
            new("inbox", "chat_read_all_unread", "{}"), CancellationToken.None, references);
        var workspace = await executor.ExecuteAsync(manager, member, room, delivery,
            new("workspace", "agent_workspace_snapshot", "{}"),
            CancellationToken.None, references);
        var combined = string.Join('\n', members.Content, todos.Content, assets.Content,
            inbox.Content, workspace.Content);

        Assert.Contains("agent_", combined, StringComparison.Ordinal);
        Assert.Contains("todo_", combined, StringComparison.Ordinal);
        Assert.Contains("asset_", combined, StringComparison.Ordinal);
        Assert.Contains("conversation_", combined, StringComparison.Ordinal);
        Assert.DoesNotContain(manager.Id, combined, StringComparison.Ordinal);
        Assert.DoesNotContain(worker.Id, combined, StringComparison.Ordinal);
        Assert.DoesNotContain(room.Id, combined, StringComparison.Ordinal);
        Assert.DoesNotContain(todo.Id, combined, StringComparison.Ordinal);
        Assert.DoesNotContain(asset.Id, combined, StringComparison.Ordinal);
        Assert.DoesNotContain(posted.Message.Id, combined, StringComparison.Ordinal);

        var rawIdRejected = await Assert.ThrowsAsync<AgentTeamException>(() =>
            executor.ExecuteAsync(manager, member, room, delivery,
                new("direct", "direct_send",
                    $$"""{"target_agent_ref":"{{worker.Id}}","content":"hello"}"""),
                CancellationToken.None, references));
        Assert.Equal(AgentTeamError.InvalidField, rawIdRejected.Code);
        var validReference = references.AgentReference(worker.Id);
        var sent = await executor.ExecuteAsync(manager, member, room, delivery,
            new("direct-2", "direct_send",
                $$"""{"target_agent_ref":"{{validReference}}","content":"hello"}"""),
            CancellationToken.None, references);
        Assert.Contains("conversation_", sent.Content, StringComparison.Ordinal);
        Assert.DoesNotContain(worker.Id, sent.Content, StringComparison.Ordinal);
    }

    private Task<AgentProfile> CreateAgentAsync(string name) =>
        _store.CreateAgentAsync("alice",
            new(name, $"{name} description", $"You are {name}", "model-1"));

    private async Task CompleteAllPendingAsync()
    {
        while (await _store.ClaimNextDeliveryAsync("alice") is { } delivery)
            await _store.CompleteDeliveryAsync("alice", delivery.Id, null);
    }
}
