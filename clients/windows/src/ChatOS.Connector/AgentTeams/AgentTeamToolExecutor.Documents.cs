using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed partial class AgentTeamToolExecutor
{
    public static IReadOnlyList<AgentToolDefinition> DocumentDefinitions { get; } =
    [
        Tool("chat_create_document",
            "创建当前 Run 内的一次性 UTF-8 Markdown 文档草稿，发送时通过 document_refs 附加。", new
            {
                type = "object",
                properties = new
                {
                    name = new { type = "string", maxLength = 512 },
                    title = new { type = "string", maxLength = 512 },
                    markdown = new { type = "string", maxLength = 2 * 1024 * 1024 },
                },
                required = new[] { "name", "title", "markdown" },
                additionalProperties = false,
            }),
        Tool("chat_inbox_send",
            "使用 chat_read_all_unread 返回的临时引用回复原会话；当前 Agent 必须仍是该会话成员。", new
            {
                type = "object",
                properties = new
                {
                    conversation_ref = new { type = "string", maxLength = 600 },
                    reply_to_message_ref = new { type = "string", maxLength = 600 },
                    content = new { type = "string", maxLength = 64_000 },
                    document_refs = DocumentReferenceSchema(),
                },
                required = new[] { "conversation_ref", "reply_to_message_ref", "content" },
                additionalProperties = false,
            }),
    ];

    private static bool IsDurableSend(string name) =>
        name is "team_send" or "direct_send" or "chat_inbox_send";

    private static AgentToolExecutionResult CreateDocument(
        AgentRunReferenceVault references,
        JsonElement arguments)
    {
        var document = references.CreateDocument(RequiredString(arguments, "name"),
            RequiredString(arguments, "title"), RequiredString(arguments, "markdown"));
        return new AgentToolExecutionResult(Json(new
        {
            document_ref = document.Reference,
            name = document.Attachment.Name,
            document.Title,
            size_bytes = document.Attachment.ByteCount,
            sha256 = document.Sha256,
        }));
    }

    private async Task<AgentToolExecutionResult> SendInboxAsync(
        AgentProfile profile,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var roomId = references.RoomId(RequiredString(arguments, "conversation_ref"))
            ?? throw AgentTeamValidation.Invalid("conversation_ref");
        var message = references.Message(RequiredString(arguments, "reply_to_message_ref"))
            ?? throw AgentTeamValidation.Invalid("reply_to_message_ref");
        if (message.RoomId != roomId)
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Reply reference does not belong to the selected conversation.");
        var source = await store.GetMessageAsync(profile.OwnerUserId, roomId, message.MessageId,
            cancellationToken).ConfigureAwait(false) ?? throw new AgentTeamException(
                AgentTeamError.NotFound, "Reply message was not found.");
        var documentReferences = StringArray(arguments, "document_refs", 8);
        var documents = references.ReserveDocuments(documentReferences);
        var post = await store.PostMessageAsync(profile.OwnerUserId, roomId,
            new AgentMessageDraft(AgentMessageSenderKind.Agent, profile.Id,
                RequiredString(arguments, "content"), Attachments: documents,
                ReplyToMessageId: source.Id, RootMessageId: source.RootMessageId,
                HopCount: source.HopCount + 1), cancellationToken).ConfigureAwait(false);
        references.ConsumeDocuments(documentReferences);
        return new AgentToolExecutionResult(Json(new
        {
            conversation_ref = references.ConversationReference(roomId),
            message_ref = references.MessageReference(roomId, post.Message.Id),
            delivery_count = post.Deliveries.Count,
            post.RoutingStopReason,
        }), ResponseMessageId: post.Message.Id);
    }
}
