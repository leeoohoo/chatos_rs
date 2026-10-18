using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentConversationCommandService : IConversationCommandService
{
    private sealed record Context(
        string OwnerUserId,
        string MainCapabilityPolicyRevision,
        string PluginInstallerCapabilityPolicyRevision);

    private readonly WindowsLocalAgentConversationClient _client;
    private readonly WindowsLocalAgentConversationRuntimeSettingsService _runtimeSettings;
    private readonly WindowsLocalAgentAttachmentVault _attachments;
    private readonly WindowsLocalAgentPlatformToolWorker _toolWorker;
    private readonly object _contextGate = new();
    private readonly object _pendingStartGate = new();
    private readonly Dictionary<string, StartLocalConversationTurnCommand> _pendingStarts = new(StringComparer.Ordinal);
    private Context? _context;

    public WindowsLocalAgentConversationCommandService(
        WindowsLocalAgentConversationClient client,
        WindowsLocalAgentConversationRuntimeSettingsService runtimeSettings,
        WindowsLocalAgentAttachmentVault attachments,
        WindowsLocalAgentPlatformToolWorker toolWorker)
    {
        _client = client;
        _runtimeSettings = runtimeSettings;
        _attachments = attachments;
        _toolWorker = toolWorker;
    }

    public void Configure(string ownerUserId, WindowsLocalAgentBootstrapSnapshot bootstrap)
    {
        if (bootstrap.OwnerUserId != ownerUserId ||
            bootstrap.MainChatCapabilities.OwnerUserId != ownerUserId ||
            bootstrap.MainChatCapabilities.ProfileKey != "main_chat" ||
            bootstrap.PluginInstallerCapabilities.OwnerUserId != ownerUserId ||
            bootstrap.PluginInstallerCapabilities.ProfileKey != "main_chat" ||
            !bootstrap.PluginInstallerCapabilities.Tools.All(value =>
                value.ValueKind == JsonValueKind.Object &&
                value.TryGetProperty("name", out var name) &&
                name.ValueKind == JsonValueKind.String &&
                name.GetString()?.StartsWith("plugin_installer_", StringComparison.Ordinal) == true))
        {
            throw new InvalidOperationException(
                "Local Agent conversation command bootstrap is invalid.");
        }
        lock (_contextGate)
        {
            _context = new(
                ownerUserId,
                bootstrap.MainChatCapabilities.CapabilityPolicyRevision,
                bootstrap.PluginInstallerCapabilities.CapabilityPolicyRevision);
        }
    }

    public void Reset()
    {
        lock (_contextGate) _context = null;
        lock (_pendingStartGate) _pendingStarts.Clear();
    }

    public async Task<ConversationCommandAck> SendNewTurnAsync(
        ConversationSendCommand command,
        CancellationToken cancellationToken = default)
    {
        var context = RequireContext();
        var selection = await _runtimeSettings.ResolveSelectionAsync(
            command.ConversationId,
            cancellationToken).ConfigureAwait(false);
        var conversation = await EnsureConversationAsync(
            context.OwnerUserId,
            command.ConversationId,
            cancellationToken).ConfigureAwait(false);
        var isPluginInstallerConversation = UsesPluginInstallerCapability(
            conversation.Conversation.Resource);
        if (isPluginInstallerConversation && command.ExplicitCapabilitySelection is not null)
        {
            throw new InvalidOperationException(
                "The dedicated Plugin installer conversation does not accept ordinary Plugin selections.");
        }
        StartLocalConversationTurnCommand start;
        lock (_pendingStartGate)
        {
            if (!_pendingStarts.TryGetValue(command.TurnId, out start!))
            {
                var metadata = new Dictionary<string, object?>
                {
                    ["source"] = isPluginInstallerConversation
                        ? "native_plugin_installer_chat"
                        : "native_main_chat",
                    ["reasoning_enabled"] = command.ReasoningEnabled ??
                        selection.Settings.ReasoningEnabled,
                };
                if (command.ExplicitCapabilitySelection is { } explicitSelection)
                {
                    metadata["explicit_capability_selection"] = ExplicitSelection(explicitSelection);
                }
                start = new(
                    "start_conversation_turn",
                    context.OwnerUserId,
                    command.ConversationId,
                    conversation.Conversation.Version,
                    command.TurnId,
                    $"message_{Guid.NewGuid():N}",
                    $"run_{Guid.NewGuid():N}",
                    command.Content,
                    JsonSerializer.SerializeToElement(metadata),
                    _attachments.Authorize(
                        command.Attachments, context.OwnerUserId, command.ConversationId),
                    selection.ModelSnapshot.ModelConfigRef,
                    selection.ModelSnapshot.ModelConfigRevision,
                    isPluginInstallerConversation
                        ? context.PluginInstallerCapabilityPolicyRevision
                        : context.MainCapabilityPolicyRevision,
                    32);
                _pendingStarts.Add(command.TurnId, start);
            }
            else if (!Matches(
                start,
                command,
                context.OwnerUserId,
                isPluginInstallerConversation))
            {
                throw new InvalidOperationException(
                    "The conversation turn retry identity was reused with different content.");
            }
        }
        var result = await _client.StartTurnAsync(start, cancellationToken).ConfigureAwait(false);
        lock (_pendingStartGate) _pendingStarts.Remove(command.TurnId);
        _toolWorker.Wake();
        return new(
            true,
            result.Turn.TurnId,
            result.Message?.MessageId ?? start.MessageId);
    }

    private static bool Matches(
        StartLocalConversationTurnCommand start,
        ConversationSendCommand command,
        string ownerUserId,
        bool isPluginInstallerConversation)
    {
        if (!string.Equals(start.OwnerUserId, ownerUserId, StringComparison.Ordinal) ||
            !string.Equals(start.ConversationId, command.ConversationId, StringComparison.Ordinal) ||
            !string.Equals(start.TurnId, command.TurnId, StringComparison.Ordinal) ||
            !string.Equals(start.Message, command.Content, StringComparison.Ordinal) ||
            !start.Attachments.Select(static attachment => attachment.AttachmentId)
                .SequenceEqual(
                    command.Attachments.Select(static attachment => attachment.Id),
                    StringComparer.Ordinal) ||
            !string.Equals(
                start.MessageMetadata.GetProperty("source").GetString(),
                isPluginInstallerConversation ? "native_plugin_installer_chat" : "native_main_chat",
                StringComparison.Ordinal))
        {
            return false;
        }

        if (command.ReasoningEnabled is { } reasoning &&
            start.MessageMetadata.GetProperty("reasoning_enabled").GetBoolean() != reasoning)
        {
            return false;
        }

        var hasFrozenSelection = start.MessageMetadata.TryGetProperty(
            "explicit_capability_selection", out var frozenSelection);
        if (command.ExplicitCapabilitySelection is not { } requestedSelection)
        {
            return !hasFrozenSelection;
        }

        return hasFrozenSelection &&
            string.Equals(
                frozenSelection.GetProperty("kind").GetString(),
                requestedSelection.Kind,
                StringComparison.Ordinal) &&
            string.Equals(
                frozenSelection.GetProperty("display_name").GetString(),
                requestedSelection.DisplayName,
                StringComparison.Ordinal) &&
            string.Equals(
                frozenSelection.GetProperty("plugin_key").GetString(),
                requestedSelection.PluginKey,
                StringComparison.Ordinal) &&
            string.Equals(
                frozenSelection.GetProperty("component_key").GetString(),
                requestedSelection.ComponentKey,
                StringComparison.Ordinal);
    }

    public async Task<ConversationCommandAck> SendGuidanceAsync(
        ConversationSendCommand command,
        CancellationToken cancellationToken = default)
    {
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
        var attachments = _attachments.Authorize(
            command.Attachments, context.OwnerUserId, command.ConversationId);
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
            attachments), cancellationToken).ConfigureAwait(false);
        _toolWorker.Wake();
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

    internal static bool UsesPluginInstallerCapability(
        WindowsLocalConversationResourceBinding? resource) =>
        resource is not null &&
        string.Equals(
            resource.Kind,
            WindowsLocalAgentWorkspaceService.ContactResourceKind,
            StringComparison.Ordinal) &&
        string.Equals(
            resource.ResourceId,
            WindowsLocalAgentWorkspaceService.PluginInstallerContactId,
            StringComparison.Ordinal);

    private static IReadOnlyDictionary<string, string> ExplicitSelection(
        ConversationExplicitCapabilitySelection selection)
    {
        if (!string.Equals(selection.Kind, "plugin", StringComparison.Ordinal) ||
            string.IsNullOrWhiteSpace(selection.DisplayName) ||
            string.IsNullOrWhiteSpace(selection.PluginKey) ||
            string.IsNullOrWhiteSpace(selection.ComponentKey))
        {
            throw new InvalidOperationException(
                "The explicit Main Chat Plugin component selection is invalid.");
        }
        return new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["kind"] = "plugin",
            ["display_name"] = selection.DisplayName.Trim(),
            ["plugin_key"] = selection.PluginKey.Trim(),
            ["component_key"] = selection.ComponentKey.Trim(),
        };
    }

}
