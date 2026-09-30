using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalAgentConversationRuntimeSettings(
    string OwnerUserId,
    string ConversationId,
    string SelectedModelConfigRef,
    string SelectedModelConfigRevision,
    string? SelectedThinkingLevel,
    string? RemoteConnectionId,
    bool ReasoningEnabled,
    ulong Version,
    long UpdatedAtUnixMs);

internal sealed record GetLocalConversationRuntimeSettingsCommand(
    string Type,
    string OwnerUserId,
    string ConversationId);

internal sealed record PutLocalConversationRuntimeSettingsCommand(
    string Type,
    string OwnerUserId,
    string ConversationId,
    string SelectedModelConfigRef,
    string SelectedModelConfigRevision,
    string? SelectedThinkingLevel,
    string? RemoteConnectionId,
    bool ReasoningEnabled,
    ulong? ExpectedVersion);

internal sealed record LocalConversationRuntimeSettingsResult(
    string Type,
    WindowsLocalAgentConversationRuntimeSettings Settings);

public sealed class WindowsLocalAgentConversationRuntimeSettingsClient(
    ILocalAgentHostClient host)
{
    public async Task<WindowsLocalAgentConversationRuntimeSettings> GetAsync(
        string ownerUserId,
        string conversationId,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            GetLocalConversationRuntimeSettingsCommand,
            LocalConversationRuntimeSettingsResult>(
                new(
                    "get_conversation_runtime_settings",
                    ownerUserId,
                    conversationId),
                cancellationToken).ConfigureAwait(false);
        return RequireSettings(response);
    }

    internal async Task<WindowsLocalAgentConversationRuntimeSettings> PutAsync(
        PutLocalConversationRuntimeSettingsCommand command,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            PutLocalConversationRuntimeSettingsCommand,
            LocalConversationRuntimeSettingsResult>(
                command,
                cancellationToken).ConfigureAwait(false);
        return RequireSettings(response);
    }

    private static WindowsLocalAgentConversationRuntimeSettings RequireSettings(
        LocalConversationRuntimeSettingsResult response) =>
        response.Type == "conversation_runtime_settings"
            ? response.Settings
            : throw new InvalidDataException(
                "Local Agent Host returned invalid conversation runtime settings.");
}
