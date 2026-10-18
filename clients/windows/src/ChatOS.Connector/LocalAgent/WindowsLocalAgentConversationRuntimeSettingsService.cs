using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentConversationRuntimeSettingsService :
    IConversationRuntimeSettingsService
{
    private sealed record Context(
        string OwnerUserId,
        IReadOnlyList<WindowsLocalAgentModelSnapshot> ModelSnapshots,
        IReadOnlyList<ConversationModelOption> ModelOptions,
        IReadOnlyList<ConversationCapabilityOption> CapabilityOptions);

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
                bootstrap.ModelOptions.ToArray(),
                bootstrap.PluginChoices.Select(choice => new ConversationCapabilityOption(
                    "plugin",
                    $"{choice.DisplayName} · {choice.ComponentKey}",
                    choice.Description,
                    choice.PluginKey,
                    choice.ComponentKey)).ToArray());
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

    public Task<IReadOnlyList<ConversationCapabilityOption>> FetchAvailableCapabilitiesAsync(
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.FromResult(RequireContext().CapabilityOptions);
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
        var current = await GetSettingsForExplicitModelUpdateAsync(
            context,
            conversationId,
            cancellationToken).ConfigureAwait(false);
        var level = DefaultThinkingLevel(snapshot);
        return Map(await PutAsync(
            current,
            snapshot,
            level,
            level is not null and not "none",
            cancellationToken).ConfigureAwait(false));
    }

    private async Task<WindowsLocalAgentConversationRuntimeSettings> GetSettingsForExplicitModelUpdateAsync(
        Context context,
        string conversationId,
        CancellationToken cancellationToken)
    {
        try
        {
            // Deliberately do not resolve the stored revision here. Fetch/Run paths stay
            // fail-closed; this path exists only after an explicit human model selection and
            // uses optimistic versioning to replace the stale snapshot with that exact choice.
            return await _client.GetAsync(
                context.OwnerUserId,
                conversationId,
                cancellationToken).ConfigureAwait(false);
        }
        catch (LocalAgentHostRequestException exception) when (exception.Code == "not_found")
        {
            var snapshot = DefaultSnapshot(context);
            var level = DefaultThinkingLevel(snapshot);
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

    public async Task<ConversationRuntimeSettings> UpdateReasoningLevelAsync(
        string conversationId,
        string level,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(level);
        var current = await EnsureSettingsAsync(conversationId, cancellationToken)
            .ConfigureAwait(false);
        var snapshot = SnapshotFor(current);
        var normalized = NormalizeThinkingLevel(level, snapshot) ?? "none";
        var option = RequireContext().ModelOptions.FirstOrDefault(value =>
            value.Id == snapshot.ModelConfigRef);
        if (option is null)
        {
            throw new InvalidOperationException("The selected Local Agent model is unavailable.");
        }

        var supported = option.EffectiveThinkingLevels;
        if (supported.Count > 0 && !supported.Contains(normalized, StringComparer.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                $"The reasoning level '{normalized}' is not supported by the selected model.");
        }

        return Map(await PutAsync(
            current,
            snapshot,
            normalized,
            !string.Equals(normalized, "none", StringComparison.Ordinal),
            cancellationToken).ConfigureAwait(false));
    }

    public async Task<ConversationRuntimeSettings> UpdateRemoteConnectionAsync(
        string conversationId,
        string? remoteConnectionId,
        CancellationToken cancellationToken = default)
    {
        var current = await EnsureSettingsAsync(conversationId, cancellationToken)
            .ConfigureAwait(false);
        var context = RequireContext();
        return Map(await _client.PutAsync(new(
            "put_conversation_runtime_settings",
            context.OwnerUserId,
            conversationId,
            current.SelectedModelConfigRef,
            current.SelectedModelConfigRevision,
            current.SelectedThinkingLevel,
            string.IsNullOrWhiteSpace(remoteConnectionId) ? null : remoteConnectionId,
            current.ReasoningEnabled,
            current.Version), cancellationToken).ConfigureAwait(false));
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
            var current = await _client.GetAsync(
                context.OwnerUserId,
                conversationId,
                cancellationToken).ConfigureAwait(false);
            if (context.ModelSnapshots.Any(snapshot =>
                    snapshot.ModelConfigRef == current.SelectedModelConfigRef &&
                    snapshot.ModelConfigRevision == current.SelectedModelConfigRevision))
            {
                return await ReconcileModelReasoningAsync(current, cancellationToken)
                    .ConfigureAwait(false);
            }

            throw new InvalidOperationException(
                "The selected Local Agent model revision is unavailable.");
        }
        catch (LocalAgentHostRequestException exception) when (exception.Code == "not_found")
        {
        }

        var snapshot = DefaultSnapshot(context);
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

    private async Task<WindowsLocalAgentConversationRuntimeSettings> MigrateStaleSelectionAsync(
        WindowsLocalAgentConversationRuntimeSettings current,
        CancellationToken cancellationToken)
    {
        var context = RequireContext();
        for (var attempt = 0; attempt < 2; attempt++)
        {
            var snapshot = context.ModelSnapshots.FirstOrDefault(value =>
                    value.ModelConfigRef == current.SelectedModelConfigRef)
                ?? DefaultSnapshot(context);
            var option = context.ModelOptions.First(value => value.Id == snapshot.ModelConfigRef);
            var level = current.ReasoningEnabled
                ? NormalizeThinkingLevel(current.SelectedThinkingLevel, snapshot)
                : "none";
            if (level is not null &&
                option.EffectiveThinkingLevels.Count > 0 &&
                !option.EffectiveThinkingLevels.Contains(level, StringComparer.OrdinalIgnoreCase))
            {
                level = DefaultThinkingLevel(snapshot);
            }
            if (current.ReasoningEnabled && level is null or "none")
            {
                level = DefaultThinkingLevel(snapshot);
            }
            var reasoningEnabled = current.ReasoningEnabled && level is not null and not "none";

            try
            {
                return await PutAsync(
                    current,
                    snapshot,
                    level,
                    reasoningEnabled,
                    cancellationToken).ConfigureAwait(false);
            }
            catch (LocalAgentHostRequestException exception)
                when (exception.Code == "conflict" && attempt == 0)
            {
                current = await _client.GetAsync(
                    context.OwnerUserId,
                    current.ConversationId,
                    cancellationToken).ConfigureAwait(false);
                if (context.ModelSnapshots.Any(value =>
                        value.ModelConfigRef == current.SelectedModelConfigRef &&
                        value.ModelConfigRevision == current.SelectedModelConfigRevision))
                {
                    return current;
                }
            }
        }

        throw new InvalidOperationException(
            "The selected Local Agent model revision could not be updated.");
    }

    private Task<WindowsLocalAgentConversationRuntimeSettings> PutAsync(
        WindowsLocalAgentConversationRuntimeSettings current,
        WindowsLocalAgentModelSnapshot snapshot,
        string? selectedThinkingLevel,
        bool reasoningEnabled,
        CancellationToken cancellationToken)
    {
        var capability = WindowsModelReasoningCapabilities.ForModel(snapshot.Model);
        var level = capability?.Normalize(selectedThinkingLevel) ??
            (capability is null ? selectedThinkingLevel : capability.DefaultLevel);
        return _client.PutAsync(new(
            "put_conversation_runtime_settings",
            current.OwnerUserId,
            current.ConversationId,
            snapshot.ModelConfigRef,
            snapshot.ModelConfigRevision,
            level,
            current.RemoteConnectionId,
            capability is not null || reasoningEnabled,
            current.Version), cancellationToken);
    }

    private async Task<WindowsLocalAgentConversationRuntimeSettings> ReconcileModelReasoningAsync(
        WindowsLocalAgentConversationRuntimeSettings stored,
        CancellationToken cancellationToken)
    {
        var snapshot = SnapshotFor(stored);
        var capability = WindowsModelReasoningCapabilities.ForModel(snapshot.Model);
        if (capability is null) return stored;
        var level = capability.Normalize(
            stored.ReasoningEnabled ? stored.SelectedThinkingLevel : "none")
            ?? capability.DefaultLevel;
        if (stored.SelectedThinkingLevel == level && stored.ReasoningEnabled) return stored;
        return await PutAsync(stored, snapshot, level, true, cancellationToken)
            .ConfigureAwait(false);
    }

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
            settings.ReasoningEnabled,
            settings.RemoteConnectionId);
    }

    private Context RequireContext()
    {
        lock (_contextGate)
        {
            return _context ?? throw new InvalidOperationException(
                "Local Agent conversation settings are not configured.");
        }
    }

    private static WindowsLocalAgentModelSnapshot DefaultSnapshot(Context context)
    {
        // Every enabled credentialed model remains explicitly selectable in Main Chat.
        // For a brand-new conversation, prefer a Task-capable model because media-only
        // entries (for example gpt-image or Sora) cannot serve the conversation endpoint.
        // Fall back to the original catalog order when an account has no Task model.
        var preferredId = context.ModelOptions.FirstOrDefault(option =>
            option.TaskEnabled && option.HasApiKey)?.Id;
        return preferredId is null
            ? context.ModelSnapshots[0]
            : context.ModelSnapshots.First(snapshot =>
                snapshot.ModelConfigRef == preferredId);
    }

    private static string? EnabledThinkingLevel(
        WindowsLocalAgentConversationRuntimeSettings current,
        WindowsLocalAgentModelSnapshot snapshot)
    {
        var selected = NormalizeThinkingLevel(current.SelectedThinkingLevel, snapshot);
        if (selected is not null and not "none") return selected;
        var configured = DefaultThinkingLevel(snapshot);
        return configured is not null and not "none" ? configured : null;
    }

    private static string? DefaultThinkingLevel(WindowsLocalAgentModelSnapshot snapshot)
    {
        if (WindowsModelReasoningCapabilities.ForModel(snapshot.Model) is { } capability)
            return capability.Normalize(snapshot.ThinkingLevel) ?? capability.DefaultLevel;
        return NormalizeThinkingLevel(snapshot.ThinkingLevel, snapshot);
    }

    private static string? NormalizeThinkingLevel(
        string? level,
        WindowsLocalAgentModelSnapshot snapshot)
    {
        if (WindowsModelReasoningCapabilities.ForModel(snapshot.Model) is { } capability)
            return capability.Normalize(level) ?? level?.Trim().ToLowerInvariant();
        var normalized = level?.Trim().ToLowerInvariant();
        if (string.IsNullOrEmpty(normalized)) return null;
        if (normalized is "off" or "disabled" or "none") return "none";
        var normalizedProvider = snapshot.Provider.Trim().ToLowerInvariant().Replace('-', '_');
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
