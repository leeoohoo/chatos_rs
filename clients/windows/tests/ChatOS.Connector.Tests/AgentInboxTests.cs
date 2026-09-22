using System.Text.Json;
using ChatOS.Connector.AgentTeams;
using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class AgentInboxTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(), "chatos-inbox-tests", Guid.NewGuid().ToString("N"));
    private string DatabasePath => Path.Combine(_directory, "state.db");
    private SqliteAgentTeamStore _store = null!;

    public async Task InitializeAsync()
    {
        var database = new LocalStateDatabase(DatabasePath);
        await database.InitializeAsync();
        _store = new SqliteAgentTeamStore(database);
    }

    public Task DisposeAsync()
    {
        if (Directory.Exists(_directory)) Directory.Delete(_directory, recursive: true);
        return Task.CompletedTask;
    }

    [Fact]
    public async Task PerConversationCursorIsMonotonicAndExcludesOwnReplies()
    {
        var (agent, room) = await CreateConversationAsync("team-a");
        await CompleteAllPendingAsync();
        var first = (await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "first"))).Message;
        await Task.Delay(2);
        var second = (await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Agent, agent.Id, "own reply"))).Message;
        await Task.Delay(2);
        var third = (await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "third"))).Message;

        var unread = await _store.ListUnreadMessagesAsync("alice", room.Id, agent.Id);
        Assert.Contains(unread, value => value.Id == first.Id);
        Assert.DoesNotContain(unread, value => value.Id == second.Id);
        Assert.Contains(unread, value => value.Id == third.Id);
        await _store.MarkReadAsync("alice", room.Id, agent.Id, third.Id);
        Assert.Empty(await _store.ListUnreadMessagesAsync("alice", room.Id, agent.Id));

        await _store.MarkReadAsync("alice", room.Id, agent.Id, first.Id);
        Assert.Empty(await _store.ListUnreadMessagesAsync("alice", room.Id, agent.Id));
        await Task.Delay(2);
        var fourth = (await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "fourth"))).Message;
        Assert.Equal(fourth.Id, Assert.Single(await _store.ListUnreadMessagesAsync(
            "alice", room.Id, agent.Id)).Id);
    }

    [Fact]
    public async Task AccountInboxGroupsAccessibleConversationsAndMarksThemReadAtomically()
    {
        var agent = await CreateAgentAsync("worker");
        var firstRoom = await CreateTeamAsync("team-a", agent);
        var secondRoom = await CreateTeamAsync("team-b", agent);
        var outsider = await CreateAgentAsync("outsider");
        var hiddenRoom = await CreateTeamAsync("hidden", outsider);
        await CompleteAllPendingAsync();
        var first = (await _store.PostMessageAsync("alice", firstRoom.Id,
            new(AgentMessageSenderKind.Human, null, "alpha"))).Message;
        await Task.Delay(2);
        var second = (await _store.PostMessageAsync("alice", secondRoom.Id,
            new(AgentMessageSenderKind.Human, null, "beta"))).Message;
        _ = await _store.PostMessageAsync("alice", hiddenRoom.Id,
            new(AgentMessageSenderKind.Human, null, "secret"));

        var inbox = await _store.ReadAllUnreadMessagesAndMarkReadAsync(
            "alice", agent.Id, 200);
        Assert.Equal(2, inbox.Count);
        Assert.Contains(inbox, value => value.Room.Id == firstRoom.Id &&
            value.Messages.Any(message => message.Id == first.Id));
        Assert.Contains(inbox, value => value.Room.Id == secondRoom.Id &&
            value.Messages.Any(message => message.Id == second.Id));
        Assert.DoesNotContain(inbox, value => value.Room.Id == hiddenRoom.Id);
        Assert.Empty(await _store.ReadAllUnreadMessagesAndMarkReadAsync(
            "alice", agent.Id, 200));
        Assert.Empty(await _store.ListUnreadMessagesAsync(
            "alice", firstRoom.Id, agent.Id));
    }

    [Fact]
    public async Task InboxToolReadsAcrossRoomsAndSchemaV16SurvivesRestart()
    {
        var (agent, room) = await CreateConversationAsync("team-a");
        await CompleteAllPendingAsync();
        var posted = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "new work"));
        var member = Assert.Single(await _store.ListMembersAsync("alice", room.Id));
        var delivery = Assert.IsType<AgentDelivery>(await _store.ClaimNextDeliveryAsync("alice"));
        var executor = new AgentTeamToolExecutor(_store, null!);
        var result = await executor.ExecuteAsync(agent, member, room, delivery,
            new AgentToolCall("inbox", "chat_read_all_unread", "{}"),
            CancellationToken.None);
        using var document = JsonDocument.Parse(result.Content);
        Assert.True(document.RootElement.GetProperty("marked_read").GetBoolean());
        Assert.True(document.RootElement.GetProperty("message_count").GetInt32() >= 1);
        Assert.Contains(executor.AllDefinitions(agent, room, delivery), value =>
            value.Name == "chat_read_all_unread");
        Assert.Empty(await _store.ListUnreadMessagesAsync("alice", room.Id, agent.Id));

        var reopenedDatabase = new LocalStateDatabase(DatabasePath);
        await reopenedDatabase.InitializeAsync();
        var reopened = new SqliteAgentTeamStore(reopenedDatabase);
        Assert.Empty(await reopened.ListUnreadMessagesAsync("alice", room.Id, agent.Id));
        await Task.Delay(2);
        var later = (await reopened.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "later"))).Message;
        var unread = Assert.Single(await reopened.ListUnreadMessagesAsync(
            "alice", room.Id, agent.Id));
        Assert.Equal(later.Id, unread.Id);
        Assert.NotEqual(posted.Message.Id, unread.Id);
    }

    [Fact]
    public async Task InboxEnforcesMembershipAndAccountScope()
    {
        var (agent, room) = await CreateConversationAsync("team-a");
        var other = await CreateAgentAsync("other");
        var notMember = await Assert.ThrowsAsync<AgentTeamException>(() =>
            _store.ListUnreadMessagesAsync("alice", room.Id, other.Id));
        Assert.Equal(AgentTeamError.NotMember, notMember.Code);
        var otherAccount = await Assert.ThrowsAsync<AgentTeamException>(() =>
            _store.ReadAllUnreadMessagesAndMarkReadAsync("bob", agent.Id));
        Assert.Equal(AgentTeamError.NotFound, otherAccount.Code);
    }

    private async Task<(AgentProfile Agent, AgentRoom Room)> CreateConversationAsync(string name)
    {
        var agent = await CreateAgentAsync(name);
        return (agent, await CreateTeamAsync(name, agent));
    }

    private Task<AgentProfile> CreateAgentAsync(string name) =>
        _store.CreateAgentAsync("alice",
            new(name, $"{name} description", $"You are {name}", "model-1"));

    private Task<AgentRoom> CreateTeamAsync(string name, AgentProfile manager) =>
        _store.CreateRoomAsync("alice", Guid.NewGuid().ToString("N"),
            new(name, "deliver"), manager.Id);

    private async Task CompleteAllPendingAsync()
    {
        while (await _store.ClaimNextDeliveryAsync("alice") is { } delivery)
            await _store.CompleteDeliveryAsync("alice", delivery.Id, null);
    }
}
