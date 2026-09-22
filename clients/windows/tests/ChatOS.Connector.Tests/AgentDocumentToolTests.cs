using System.Text;
using System.Text.Json;
using ChatOS.Connector.AgentTeams;
using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class AgentDocumentToolTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(), "chatos-document-tests", Guid.NewGuid().ToString("N"));
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
    public async Task DocumentSendIsOneTimeAndCallReceiptPreventsDuplicateMessages()
    {
        var (manager, worker, room, member, delivery) = await CreateRunningTeamAsync();
        var executor = new AgentTeamToolExecutor(_store, null!);
        var references = new AgentRunReferenceVault();
        var created = await executor.ExecuteAsync(manager, member, room, delivery,
            new("document", "chat_create_document",
                "{\"name\":\"report\",\"title\":\"Report\",\"markdown\":\"# Done\"}"),
            CancellationToken.None, references);
        using var createdJson = JsonDocument.Parse(created.Content);
        var documentReference = createdJson.RootElement.GetProperty("document_ref").GetString()!;
        Assert.EndsWith(".md", createdJson.RootElement.GetProperty("name").GetString(),
            StringComparison.Ordinal);
        var workerReference = references.AgentReference(worker.Id);
        var arguments = $$"""
            {"content":"delivery","mention_agent_refs":["{{workerReference}}"],"document_refs":["{{documentReference}}"]}
            """;
        var call = new AgentToolCall("send-once", "team_send", arguments);

        var first = await executor.ExecuteAsync(manager, member, room, delivery, call,
            CancellationToken.None, references);
        var replayed = await executor.ExecuteAsync(manager, member, room, delivery, call,
            CancellationToken.None, references);
        Assert.Equal(first, replayed);
        var sent = Assert.Single(await _store.ListMessagesAsync("alice", room.Id), value =>
            value.SenderKind == AgentMessageSenderKind.Agent && value.Content == "delivery");
        var metadata = Assert.Single(sent.Attachments);
        var payload = await _store.GetMessageAttachmentAsync("alice", room.Id, metadata.Id);
        Assert.Equal("# Done", Encoding.UTF8.GetString(payload!.Data));

        var conflict = await Assert.ThrowsAsync<AgentTeamException>(() => executor.ExecuteAsync(
            manager, member, room, delivery,
            call with { Arguments = arguments.Replace("delivery", "changed", StringComparison.Ordinal) },
            CancellationToken.None, references));
        Assert.Equal(AgentTeamError.Conflict, conflict.Code);
        var consumed = await Assert.ThrowsAsync<AgentTeamException>(() => executor.ExecuteAsync(
            manager, member, room, delivery, call with { Id = "send-again" },
            CancellationToken.None, references));
        Assert.Equal(AgentTeamError.InvalidField, consumed.Code);
    }

    [Fact]
    public async Task InboxReplyUsesConversationAndMessageReferences()
    {
        var (manager, _, room, member, delivery) = await CreateRunningTeamAsync();
        var second = await _store.CreateRoomAsync("alice", "project-2",
            new("second", "support"), manager.Id);
        await CompleteAllPendingAsync();
        var source = (await _store.PostMessageAsync("alice", second.Id,
            new(AgentMessageSenderKind.Human, null, "question", [manager.Id]))).Message;
        var executor = new AgentTeamToolExecutor(_store, null!);
        var references = new AgentRunReferenceVault();
        var conversationReference = references.ConversationReference(second.Id);
        var messageReference = references.MessageReference(second.Id, source.Id);

        var result = await executor.ExecuteAsync(manager, member, room, delivery,
            new("inbox-reply", "chat_inbox_send", $$"""
                {"conversation_ref":"{{conversationReference}}","reply_to_message_ref":"{{messageReference}}","content":"answer"}
                """), CancellationToken.None, references);

        Assert.Contains("message_", result.Content, StringComparison.Ordinal);
        var reply = Assert.Single(await _store.ListMessagesAsync("alice", second.Id), value =>
            value.SenderKind == AgentMessageSenderKind.Agent);
        Assert.Equal(source.Id, reply.ReplyToMessageId);
        Assert.Equal(source.RootMessageId, reply.RootMessageId);
    }

    [Fact]
    public async Task DocumentNamesCannotEscapeTheRunVault()
    {
        var (manager, _, room, member, delivery) = await CreateRunningTeamAsync();
        var executor = new AgentTeamToolExecutor(_store, null!);
        var error = await Assert.ThrowsAsync<AgentTeamException>(() => executor.ExecuteAsync(
            manager, member, room, delivery,
            new("document", "chat_create_document",
                "{\"name\":\"../escape.md\",\"title\":\"x\",\"markdown\":\"x\"}"),
            CancellationToken.None, new AgentRunReferenceVault()));
        Assert.Equal(AgentTeamError.InvalidField, error.Code);
    }

    private async Task<(AgentProfile Manager, AgentProfile Worker, AgentRoom Room,
        AgentRoomMember Member, AgentDelivery Delivery)> CreateRunningTeamAsync()
    {
        var manager = await CreateAgentAsync("manager");
        var worker = await CreateAgentAsync("worker");
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("team", "deliver"), manager.Id);
        await _store.UpsertMemberAsync("alice", room.Id, worker.Id,
            new("developer", "implement"));
        await CompleteAllPendingAsync();
        _ = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "start", [manager.Id]));
        var delivery = Assert.IsType<AgentDelivery>(await _store.ClaimNextDeliveryAsync("alice"));
        var member = Assert.Single(await _store.ListMembersAsync("alice", room.Id), value =>
            value.AgentId == manager.Id);
        return (manager, worker, room, member, delivery);
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
