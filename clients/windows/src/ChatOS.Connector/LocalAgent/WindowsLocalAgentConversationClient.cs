using System.Text.Json;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalConversationRecord(
    string ConversationId,
    string OwnerUserId,
    string Title,
    ulong Version,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs,
    WindowsLocalConversationResourceBinding? Resource = null);

internal sealed record WindowsLocalConversationResourceBinding(
    string Kind,
    string ResourceId);

internal sealed record WindowsLocalConversationTurnRecord(
    string TurnId,
    string ConversationId,
    string UserMessageId,
    string RunId,
    string Status,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs);

internal sealed record WindowsLocalConversationMessageRecord(
    string MessageId,
    string ConversationId,
    string TurnId,
    ulong Ordinal,
    string Role,
    JsonElement Content,
    JsonElement Metadata,
    long CreatedAtUnixMs);

internal sealed record WindowsLocalConversationAttachmentRecord(
    string AttachmentId,
    string ConversationId,
    string TurnId,
    string MessageId,
    ulong Ordinal,
    string DisplayName,
    string MediaType,
    ulong ByteSize,
    string Sha256,
    string AuthorizedLocalRef,
    JsonElement Metadata,
    long CreatedAtUnixMs);

internal sealed record WindowsLocalConversationDetail(
    WindowsLocalConversationRecord Conversation,
    IReadOnlyList<WindowsLocalConversationTurnRecord> Turns,
    IReadOnlyList<WindowsLocalConversationMessageRecord> Messages,
    IReadOnlyList<WindowsLocalConversationAttachmentRecord> Attachments);

internal sealed record WindowsLocalConversationTurnMutation(
    WindowsLocalConversationRecord Conversation,
    WindowsLocalConversationTurnRecord Turn,
    WindowsLocalConversationMessageRecord? Message,
    IReadOnlyList<WindowsLocalConversationAttachmentRecord> Attachments);

internal sealed record WindowsLocalConversationHistoryPage(
    WindowsLocalConversationRecord Conversation,
    IReadOnlyList<WindowsLocalConversationTurnRecord> Turns,
    IReadOnlyList<WindowsLocalConversationMessageRecord> Messages,
    IReadOnlyList<WindowsLocalConversationAttachmentRecord> Attachments,
    ulong? NextBeforeOrdinal);

internal sealed record WindowsLocalConversationPage(
    IReadOnlyList<WindowsLocalConversationRecord> Conversations,
    long? NextBeforeUpdatedAtUnixMs,
    string? NextBeforeConversationId);

internal sealed record CreateLocalConversationCommand(
    string Type,
    string ConversationId,
    string OwnerUserId,
    string Title,
    WindowsLocalConversationResourceBinding? Resource = null);

internal sealed record GetLocalConversationCommand(
    string Type,
    string OwnerUserId,
    string ConversationId);

internal sealed record GetLocalConversationHistoryCommand(
    string Type,
    string OwnerUserId,
    string ConversationId,
    ulong? BeforeOrdinal,
    uint Limit);

internal sealed record ListLocalConversationsCommand(
    string Type,
    string OwnerUserId,
    long? BeforeUpdatedAtUnixMs,
    string? BeforeConversationId,
    uint Limit);

internal sealed record StartLocalConversationTurnCommand(
    string Type,
    string OwnerUserId,
    string ConversationId,
    ulong ExpectedConversationVersion,
    string TurnId,
    string MessageId,
    string RunId,
    string Message,
    JsonElement MessageMetadata,
    IReadOnlyList<WindowsLocalConversationAttachmentSpec> Attachments,
    string ModelConfigRef,
    string ModelConfigRevision,
    string CapabilityPolicyRevision,
    uint MaxIterations);

internal sealed record GuideLocalConversationTurnCommand(
    string Type,
    string OwnerUserId,
    string ConversationId,
    ulong ExpectedConversationVersion,
    string TurnId,
    ulong? ExpectedRunVersion,
    string MessageId,
    string Message,
    JsonElement MessageMetadata,
    IReadOnlyList<WindowsLocalConversationAttachmentSpec> Attachments);

internal sealed record ResumeLocalConversationTurnCommand(
    string Type, string OwnerUserId, string ConversationId,
    ulong ExpectedConversationVersion, string TurnId, ulong ExpectedRunVersion,
    string ExpectedRunStatus, string MessageId, string Message,
    JsonElement MessageMetadata,
    IReadOnlyList<WindowsLocalConversationAttachmentSpec> Attachments,
    string Reason);

internal sealed record CancelLocalConversationTurnCommand(
    string Type,
    string OwnerUserId,
    string ConversationId,
    ulong ExpectedConversationVersion,
    string TurnId,
    ulong? ExpectedRunVersion,
    string Reason);

internal sealed record LocalConversationResult(
    string Type,
    WindowsLocalConversationDetail Conversation);

internal sealed record LocalConversationTurnMutationResult(
    string Type,
    WindowsLocalConversationTurnMutation Result);

internal sealed record LocalConversationHistoryResult(
    string Type,
    WindowsLocalConversationHistoryPage Page);

internal sealed record LocalConversationsResult(
    string Type,
    WindowsLocalConversationPage Page);

public sealed class WindowsLocalAgentConversationClient(ILocalAgentHostClient host)
{
    internal Task<WindowsLocalConversationDetail> CreateAsync(
        string ownerUserId,
        string conversationId,
        CancellationToken cancellationToken) => SendConversationAsync(
            new CreateLocalConversationCommand(
                "create_conversation",
                conversationId,
                ownerUserId,
                "Conversation"),
            cancellationToken);

    internal Task<WindowsLocalConversationDetail> CreateAsync(
        string ownerUserId,
        string conversationId,
        string title,
        WindowsLocalConversationResourceBinding resource,
        CancellationToken cancellationToken) => SendConversationAsync(
            new CreateLocalConversationCommand(
                "create_conversation",
                conversationId,
                ownerUserId,
                title,
                resource),
            cancellationToken);

    internal Task<WindowsLocalConversationDetail> GetAsync(
        string ownerUserId,
        string conversationId,
        CancellationToken cancellationToken) => SendConversationAsync(
            new GetLocalConversationCommand(
                "get_conversation",
                ownerUserId,
                conversationId),
            cancellationToken);

    internal async Task<WindowsLocalConversationPage> ListAsync(
        string ownerUserId,
        long? beforeUpdatedAtUnixMs,
        string? beforeConversationId,
        uint limit,
        CancellationToken cancellationToken)
    {
        var response = await host.SendAsync<
            ListLocalConversationsCommand,
            LocalConversationsResult>(new(
                "list_conversations",
                ownerUserId,
                beforeUpdatedAtUnixMs,
                beforeConversationId,
                limit), cancellationToken).ConfigureAwait(false);
        return response.Type == "conversations"
            ? response.Page
            : throw new InvalidDataException(
                "Local Agent Host returned an invalid conversation list result.");
    }

    internal async Task<WindowsLocalConversationHistoryPage> HistoryAsync(
        string ownerUserId,
        string conversationId,
        ulong? beforeOrdinal,
        uint limit,
        CancellationToken cancellationToken)
    {
        var response = await host.SendAsync<
            GetLocalConversationHistoryCommand,
            LocalConversationHistoryResult>(new(
                "get_conversation_history",
                ownerUserId,
                conversationId,
                beforeOrdinal,
                limit), cancellationToken).ConfigureAwait(false);
        return response.Type == "conversation_history"
            ? response.Page
            : throw new InvalidDataException(
                "Local Agent Host returned an invalid conversation history result.");
    }

    internal async Task<WindowsLocalConversationTurnMutation> StartTurnAsync(
        StartLocalConversationTurnCommand command,
        CancellationToken cancellationToken)
    {
        var response = await host.SendAsync<
            StartLocalConversationTurnCommand,
            LocalConversationTurnMutationResult>(command, cancellationToken)
            .ConfigureAwait(false);
        return RequireMutation(response, "conversation_turn_started");
    }

    internal async Task<WindowsLocalConversationTurnMutation> GuideTurnAsync(
        GuideLocalConversationTurnCommand command,
        CancellationToken cancellationToken)
    {
        var response = await host.SendAsync<
            GuideLocalConversationTurnCommand,
            LocalConversationTurnMutationResult>(command, cancellationToken)
            .ConfigureAwait(false);
        return RequireMutation(response, "conversation_turn_updated");
    }

    internal async Task<WindowsLocalConversationTurnMutation> CancelTurnAsync(
        CancelLocalConversationTurnCommand command,
        CancellationToken cancellationToken)
    {
        var response = await host.SendAsync<
            CancelLocalConversationTurnCommand,
            LocalConversationTurnMutationResult>(command, cancellationToken)
            .ConfigureAwait(false);
        return RequireMutation(response, "conversation_turn_updated");
    }

    internal async Task<WindowsLocalConversationTurnMutation> ResumeTurnAsync(
        ResumeLocalConversationTurnCommand command,
        CancellationToken cancellationToken)
    {
        var response = await host.SendAsync<
            ResumeLocalConversationTurnCommand,
            LocalConversationTurnMutationResult>(command, cancellationToken)
            .ConfigureAwait(false);
        return RequireMutation(response, "conversation_turn_updated");
    }

    private async Task<WindowsLocalConversationDetail> SendConversationAsync<TCommand>(
        TCommand command,
        CancellationToken cancellationToken)
        where TCommand : notnull
    {
        var response = await host.SendAsync<TCommand, LocalConversationResult>(
            command,
            cancellationToken).ConfigureAwait(false);
        return response.Type == "conversation"
            ? response.Conversation
            : throw new InvalidDataException(
                "Local Agent Host returned an invalid conversation result.");
    }

    private static WindowsLocalConversationTurnMutation RequireMutation(
        LocalConversationTurnMutationResult response,
        string expectedType) => response.Type == expectedType
            ? response.Result
            : throw new InvalidDataException(
                "Local Agent Host returned an invalid conversation turn result.");
}
