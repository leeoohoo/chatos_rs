using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentConversationCommandService : IConversationCommandService
{
    private sealed record Context(
        string OwnerUserId,
        string CapabilityPolicyRevision);

    private readonly WindowsLocalAgentConversationClient _client;
    private readonly WindowsLocalAgentConversationRuntimeSettingsService _runtimeSettings;
    private readonly object _contextGate = new();
    private Context? _context;

    public WindowsLocalAgentConversationCommandService(
        WindowsLocalAgentConversationClient client,
        WindowsLocalAgentConversationRuntimeSettingsService runtimeSettings)
    {
        _client = client;
        _runtimeSettings = runtimeSettings;
    }

    public void Configure(string ownerUserId, WindowsLocalAgentBootstrapSnapshot bootstrap)
    {
        if (bootstrap.OwnerUserId != ownerUserId ||
            bootstrap.MainChatCapabilities.OwnerUserId != ownerUserId ||
            bootstrap.MainChatCapabilities.ProfileKey != "main_chat")
        {
            throw new InvalidOperationException(
                "Local Agent conversation command bootstrap is invalid.");
        }
        lock (_contextGate)
        {
            _context = new(
                ownerUserId,
                bootstrap.MainChatCapabilities.CapabilityPolicyRevision);
        }
    }

    public void Reset()
    {
        lock (_contextGate) _context = null;
    }

    public async Task<ConversationCommandAck> SendNewTurnAsync(
        ConversationSendCommand command,
        CancellationToken cancellationToken = default)
    {
        RequireNoAttachments(command);
        var context = RequireContext();
        var selection = await _runtimeSettings.ResolveSelectionAsync(
            command.ConversationId,
            cancellationToken).ConfigureAwait(false);
        var conversation = await EnsureConversationAsync(
            context.OwnerUserId,
            command.ConversationId,
            cancellationToken).ConfigureAwait(false);
        var messageId = $"message_{Guid.NewGuid():N}";
        var result = await _client.StartTurnAsync(new(
            "start_conversation_turn",
            context.OwnerUserId,
            command.ConversationId,
            conversation.Conversation.Version,
            command.TurnId,
            messageId,
            $"run_{Guid.NewGuid():N}",
            command.Content,
            JsonSerializer.SerializeToElement(new Dictionary<string, object?>
            {
                ["source"] = "native_main_chat",
                ["reasoning_enabled"] = command.ReasoningEnabled ??
                    selection.Settings.ReasoningEnabled,
            }),
            [],
            selection.ModelSnapshot.ModelConfigRef,
            selection.ModelSnapshot.ModelConfigRevision,
            context.CapabilityPolicyRevision,
            32), cancellationToken).ConfigureAwait(false);
        return new(
            true,
            result.Turn.TurnId,
            result.Message?.MessageId ?? messageId);
    }

    public async Task<ConversationCommandAck> SendGuidanceAsync(
        ConversationSendCommand command,
        CancellationToken cancellationToken = default)
    {
        RequireNoAttachments(command);
        var context = RequireContext();
        var detail = await _client.GetAsync(
            context.OwnerUserId,
            command.ConversationId,
            cancellationToken).ConfigureAwait(false);
        if (!detail.Turns.Any(turn =>
            turn.TurnId == command.TurnId && turn.Status == "running"))
        {
            throw new GuidanceTargetInactiveException();
        }
        var messageId = $"message_{Guid.NewGuid():N}";
        var result = await _client.GuideTurnAsync(new(
            "guide_conversation_turn",
            context.OwnerUserId,
            command.ConversationId,
            detail.Conversation.Version,
            command.TurnId,
            null,
            messageId,
            command.Content,
            JsonSerializer.SerializeToElement(new Dictionary<string, object?>
            {
                ["source"] = "native_guidance",
            }),
            []), cancellationToken).ConfigureAwait(false);
        return new(
            true,
            result.Turn.TurnId,
            result.Message?.MessageId ?? messageId);
    }

    public async Task StopTurnAsync(
        string conversationId,
        string? turnId,
        CancellationToken cancellationToken = default)
    {
        var context = RequireContext();
        var detail = await _client.GetAsync(
            context.OwnerUserId,
            conversationId,
            cancellationToken).ConfigureAwait(false);
        var target = turnId is null
            ? detail.Turns.LastOrDefault(turn => turn.Status == "running")
            : detail.Turns.FirstOrDefault(turn => turn.TurnId == turnId);
        if (target is null || target.Status != "running")
        {
            throw new GuidanceTargetInactiveException();
        }
        _ = await _client.CancelTurnAsync(new(
            "cancel_conversation_turn",
            context.OwnerUserId,
            conversationId,
            detail.Conversation.Version,
            target.TurnId,
            null,
            "user requested stop"), cancellationToken).ConfigureAwait(false);
    }

    private async Task<WindowsLocalConversationDetail> EnsureConversationAsync(
        string ownerUserId,
        string conversationId,
        CancellationToken cancellationToken)
    {
        try
        {
            return await _client.GetAsync(ownerUserId, conversationId, cancellationToken)
                .ConfigureAwait(false);
        }
        catch (LocalAgentHostRequestException exception) when (exception.Code == "not_found")
        {
            try
            {
                return await _client.CreateAsync(ownerUserId, conversationId, cancellationToken)
                    .ConfigureAwait(false);
            }
            catch (LocalAgentHostRequestException conflict) when (conflict.Code == "conflict")
            {
                return await _client.GetAsync(ownerUserId, conversationId, cancellationToken)
                    .ConfigureAwait(false);
            }
        }
    }

    private Context RequireContext()
    {
        lock (_contextGate)
        {
            return _context ?? throw new InvalidOperationException(
                "Local Agent conversation commands are not configured.");
        }
    }

    private static void RequireNoAttachments(ConversationSendCommand command)
    {
        if (command.Attachments.Count != 0)
        {
            throw new InvalidOperationException(
                "Windows Local Agent attachment authorization is not configured yet.");
        }
    }
}
