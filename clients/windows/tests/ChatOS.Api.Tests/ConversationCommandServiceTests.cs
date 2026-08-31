using System.Net;
using System.Text.Json;
using ChatOS.Api.Conversation;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class ConversationCommandServiceTests
{
    [Fact]
    public async Task SendNewTurnUsesAuthoritativeRuntimeAndSelectedModel()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        string? sendBody = null;
        var client = ApiTestClient.Create(store, request => request.RequestUri?.AbsolutePath switch
        {
            "/api/chatos/conversations/c%2F1/runtime-settings" => StubHttpMessageHandler.Json("""
                {"selected_model_id":"model-2","reasoning_enabled":true,"plan_mode_enabled":false,"remote_connection_id":"remote-1","workspace_root":"C:\\repo"}
                """),
            "/api/chatos/ai-model-configs" => StubHttpMessageHandler.Json("""
                [
                  {"id":"model-1","name":"Fallback","model_name":"fallback","enabled":true},
                  {"id":"model-2","name":"Selected","model_name":"gpt-test","thinking_level":"high","temperature":0.2,"enabled":true}
                ]
                """),
            "/api/chatos/agent/chat/send" => CaptureSend(request, body => sendBody = body),
            _ => throw new InvalidOperationException(request.RequestUri?.ToString()),
        });
        var service = new ConversationCommandService(client, new EmptyAttachmentService());

        var acknowledgement = await service.SendNewTurnAsync(new ConversationSendCommand(
            "c/1",
            "turn-client-1",
            "开始实现",
            Array.Empty<ConversationAttachmentDraft>(),
            PlanModeEnabled: true));

        Assert.Equal("turn-server-1", acknowledgement.TurnId);
        Assert.Equal("message-1", acknowledgement.UserMessageId);
        using var document = JsonDocument.Parse(sendBody!);
        var root = document.RootElement;
        Assert.True(root.GetProperty("reasoning_enabled").GetBoolean());
        Assert.True(root.GetProperty("plan_mode").GetBoolean());
        Assert.Equal("model-2", root.GetProperty("model_config_id").GetString());
        Assert.Equal("gpt-test", root.GetProperty("ai_model_config").GetProperty("model_name").GetString());
        Assert.Equal("remote-1", root.GetProperty("remote_connection_id").GetString());
    }

    [Fact]
    public async Task GuidanceConflictMapsToDomainException()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, _ =>
            StubHttpMessageHandler.Json(
                "{\"detail\":\"turn has ended\"}",
                HttpStatusCode.Conflict));
        var service = new ConversationCommandService(client, new EmptyAttachmentService());
        var command = new ConversationSendCommand(
            "c1",
            "turn-1",
            "继续",
            Array.Empty<ConversationAttachmentDraft>());

        await Assert.ThrowsAsync<GuidanceTargetInactiveException>(() =>
            service.SendGuidanceAsync(command));
    }

    [Fact]
    public async Task StopTurnSendsStableTurnIdentity()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            var body = request.Content!.ReadAsStringAsync().GetAwaiter().GetResult();
            Assert.Contains("\"conversation_id\":\"c1\"", body, StringComparison.Ordinal);
            Assert.Contains("\"turn_id\":\"turn-1\"", body, StringComparison.Ordinal);
            return StubHttpMessageHandler.Json("{\"success\":true}");
        });
        var service = new ConversationCommandService(client, new EmptyAttachmentService());

        await service.StopTurnAsync("c1", "turn-1");
    }

    private static HttpResponseMessage CaptureSend(
        HttpRequestMessage request,
        Action<string> capture)
    {
        capture(request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
        return StubHttpMessageHandler.Json("""
            {"accepted":true,"turn_id":"turn-server-1","source_user_message_id":"message-1"}
            """);
    }

    private sealed class EmptyAttachmentService : IConversationAttachmentService
    {
        public Task<IReadOnlyList<ConversationAttachmentReference>> UploadAsync(
            IReadOnlyList<ConversationAttachmentDraft> attachments,
            string conversationId,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ConversationAttachmentReference>>(
                Array.Empty<ConversationAttachmentReference>());
    }
}
