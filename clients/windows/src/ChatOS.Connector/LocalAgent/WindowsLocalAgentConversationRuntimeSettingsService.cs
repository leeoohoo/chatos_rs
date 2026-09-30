using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentConversationRuntimeSettingsService :
    IConversationRuntimeSettingsService
{
    private sealed record Context(
        string OwnerUserId,
        IReadOnlyList<WindowsLocalAgentModelSnapshot> ModelSnapshots,
        IReadOnlyList<ConversationModelOption> ModelOptions);

    private readonly WindowsLocalAgentConversationRuntimeSettingsClient _client;
    private readonly object _contextGate = new();
    private Context? _context;

    public WindowsLocalAgentConversationRuntimeSettingsService(
        WindowsLocalAgentConversationRuntimeSettingsClient client)
    {
        _client = client;
    }

    public void Configure(
        string ownerUserId,
        WindowsLocalAgentBootstrapSnapshot bootstrap)
    {
        if (bootstrap.OwnerUserId != ownerUserId ||
            bootstrap.ModelSnapshots.Count == 0 ||
            bootstrap.ModelSnapshots.Count != bootstrap.ModelOptions.Count ||
            bootstrap.ModelSnapshots.Any(snapshot => snapshot.OwnerUserId != ownerUserId) ||
            bootstrap.ModelOptions.Any(option => !bootstrap.ModelSnapshots.Any(snapshot =>
                snapshot.ModelConfigRef == option.Id)))
        {
            throw new InvalidOperationException(
                "Local Agent conversation settings bootstrap is invalid.");
        }
        lock (_contextGate)
        {
            _context = new Context(
                ownerUserId,
                bootstrap.ModelSnapshots.ToArray(),
                bootstrap.ModelOptions.ToArray());
        }
    }

    public void Reset()
    {
        lock (_contextGate) _context = null;
    }

    public async Task<ConversationRuntimeSettings> FetchAsync(
        string conversationId,
        CancellationToken cancellationToken = default) =>
        Map(await EnsureSettingsAsync(conversationId, cancellationToken).ConfigureAwait(false));

    public Task<IReadOnlyList<ConversationModelOption>> FetchAvailableModelsAsync(
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(RequireContext().ModelOptions);
    }

    public async Task<ConversationRuntimeSettings> UpdateModelAsync(
        string conversationId,
        string modelId,
        CancellationToken cancellationToken = default)
    {
        var context = RequireContext();
        var snapshot = context.ModelSnapshots.FirstOrDefault(value =>
            value.ModelConfigRef == modelId) ?? throw new InvalidOperationException(
                "The selected Local Agent model is unavailable.");
        var current = await EnsureSettingsAsync(conversationId, cancellationToken)
            .ConfigureAwait(false);
        var level = DefaultThinkingLevel(snapshot);
        return Map(await PutAsync(
            current,
            snapshot,
            level,
            level is not null and not "none",
            cancellationToken).ConfigureAwait(false));
    }

    public async Task<ConversationRuntimeSettings> UpdateReasoningAsync(
        string conversationId,
        bool enabled,
        CancellationToken cancellationToken = default)
    {
        var current = await EnsureSettingsAsync(conversationId, cancellationToken)
            .ConfigureAwait(false);
        var snapshot = SnapshotFor(current);
        var level = enabled ? EnabledThinkingLevel(current, snapshot) : "none";
        return Map(await PutAsync(
            current,
            snapshot,
            level,
            enabled,
            cancellationToken).ConfigureAwait(false));
    }

    internal async Task<WindowsLocalAgentConversationRuntimeSelection> ResolveSelectionAsync(
        string conversationId,
        CancellationToken cancellationToken = default)
    {
        var settings = await EnsureSettingsAsync(conversationId, cancellationToken)
            .ConfigureAwait(false);
        return new(settings, SnapshotFor(settings));
    }

    private async Task<WindowsLocalAgentConversationRuntimeSettings> EnsureSettingsAsync(
        string conversationId,
        CancellationToken cancellationToken)
    {
        var context = RequireContext();
        try
        {
            return await _client.GetAsync(
                context.OwnerUserId,
                conversationId,
                cancellationToken).ConfigureAwait(false);
        }
        catch (LocalAgentHostRequestException exception) when (exception.Code == "not_found")
        {
        }

        var snapshot = context.ModelSnapshots[0];
        var level = DefaultThinkingLevel(snapshot);
        try
        {
            return await _client.PutAsync(new(
                "put_conversation_runtime_settings",
                context.OwnerUserId,
                conversationId,
                snapshot.ModelConfigRef,
                snapshot.ModelConfigRevision,
                level,
                null,
                level is not null and not "none",
                null), cancellationToken).ConfigureAwait(false);
        }
        catch (LocalAgentHostRequestException exception) when (exception.Code == "conflict")
        {
            return await _client.GetAsync(
                context.OwnerUserId,
                conversationId,
                cancellationToken).ConfigureAwait(false);
        }
    }

    private Task<WindowsLocalAgentConversationRuntimeSettings> PutAsync(
        WindowsLocalAgentConversationRuntimeSettings current,
        WindowsLocalAgentModelSnapshot snapshot,
        string? selectedThinkingLevel,
        bool reasoningEnabled,
        CancellationToken cancellationToken) =>
        _client.PutAsync(new(
            "put_conversation_runtime_settings",
            current.OwnerUserId,
            current.ConversationId,
            snapshot.ModelConfigRef,
            snapshot.ModelConfigRevision,
            selectedThinkingLevel,
            current.RemoteConnectionId,
            reasoningEnabled,
            current.Version), cancellationToken);

    private WindowsLocalAgentModelSnapshot SnapshotFor(
        WindowsLocalAgentConversationRuntimeSettings settings) =>
        RequireContext().ModelSnapshots.FirstOrDefault(snapshot =>
            snapshot.ModelConfigRef == settings.SelectedModelConfigRef &&
            snapshot.ModelConfigRevision == settings.SelectedModelConfigRevision)
        ?? throw new InvalidOperationException(
            "The selected Local Agent model revision is unavailable.");

    private ConversationRuntimeSettings Map(
        WindowsLocalAgentConversationRuntimeSettings settings)
    {
        var context = RequireContext();
        _ = SnapshotFor(settings);
        var option = context.ModelOptions.FirstOrDefault(value =>
            value.Id == settings.SelectedModelConfigRef) ?? throw new InvalidOperationException(
                "The selected Local Agent model is unavailable.");
        return new ConversationRuntimeSettings(
            settings.SelectedModelConfigRef,
            option.DisplayName,
            settings.SelectedThinkingLevel,
            settings.ReasoningEnabled);
    }

    private Context RequireContext()
    {
        lock (_contextGate)
        {
            return _context ?? throw new InvalidOperationException(
                "Local Agent conversation settings are not configured.");
        }
    }

    private static string? EnabledThinkingLevel(
        WindowsLocalAgentConversationRuntimeSettings current,
        WindowsLocalAgentModelSnapshot snapshot)
    {
        var selected = NormalizeThinkingLevel(current.SelectedThinkingLevel, snapshot.Provider);
        if (selected is not null and not "none") return selected;
        var configured = DefaultThinkingLevel(snapshot);
        return configured is not null and not "none" ? configured : null;
    }

    private static string? DefaultThinkingLevel(WindowsLocalAgentModelSnapshot snapshot) =>
        NormalizeThinkingLevel(snapshot.ThinkingLevel, snapshot.Provider);

    private static string? NormalizeThinkingLevel(string? level, string provider)
    {
        var normalized = level?.Trim().ToLowerInvariant();
        if (string.IsNullOrEmpty(normalized)) return null;
        if (normalized is "off" or "disabled" or "none") return "none";
        var normalizedProvider = provider.Trim().ToLowerInvariant().Replace('-', '_');
        if (normalized is "max" or "xhigh")
        {
            return normalizedProvider == "deepseek" ? "max" : "xhigh";
        }
        if (normalized == "minimal" &&
            normalizedProvider is "openai_compatible" or "compatible")
        {
            return "low";
        }
        return normalized;
    }
}

internal sealed record WindowsLocalAgentConversationRuntimeSelection(
    WindowsLocalAgentConversationRuntimeSettings Settings,
    WindowsLocalAgentModelSnapshot ModelSnapshot);
