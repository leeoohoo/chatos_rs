using System.Runtime.CompilerServices;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentRealtimeClient : IRealtimeClient
{
    private readonly WindowsLocalAgentConversationClient _conversations;
    private readonly WindowsLocalAgentEventHub _eventHub;
    private readonly object _gate = new();
    private string? _ownerUserId;

    public WindowsLocalAgentRealtimeClient(
        WindowsLocalAgentConversationClient conversations,
        WindowsLocalAgentEventHub eventHub)
    {
        _conversations = conversations;
        _eventHub = eventHub;
    }

    public void Configure(string ownerUserId)
    {
        lock (_gate) _ownerUserId = ownerUserId;
        _eventHub.Configure(ownerUserId);
    }

    public void Reset()
    {
        lock (_gate) _ownerUserId = null;
        _eventHub.Reset();
    }

    public async IAsyncEnumerable<ConversationRealtimeSignal> StreamConversationAsync(
        string conversationId,
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(conversationId);
        var owner = RequireOwner();
        ulong? observedVersion = null;
        var observedRunIds = new HashSet<string>(StringComparer.Ordinal);
        var initial = await GetConversationOrNullAsync(
            owner, conversationId, cancellationToken).ConfigureAwait(false);
        if (initial is not null)
        {
            observedVersion = initial.Conversation.Version;
            observedRunIds = initial.Turns.Select(value => value.RunId)
                .ToHashSet(StringComparer.Ordinal);
            yield return Signal(conversationId, initial);
        }

        await foreach (var update in _eventHub.UpdatesAsync(cancellationToken).ConfigureAwait(false))
        {
            if (!string.Equals(update.OwnerUserId, owner, StringComparison.Ordinal)) continue;
            if (!update.IsReconcile && !EventsAffectConversation(
                    update.Events, conversationId, observedRunIds)) continue;
            var current = await GetConversationOrNullAsync(
                owner, conversationId, cancellationToken).ConfigureAwait(false);
            if (current is null) continue;
            observedRunIds = current.Turns.Select(value => value.RunId)
                .ToHashSet(StringComparer.Ordinal);
            if (observedVersion == current.Conversation.Version) continue;
            observedVersion = current.Conversation.Version;
            yield return Signal(conversationId, current);
        }
    }

    private async Task<WindowsLocalConversationDetail?> GetConversationOrNullAsync(
        string ownerUserId,
        string conversationId,
        CancellationToken cancellationToken)
    {
        try
        {
            return await _conversations.GetAsync(ownerUserId, conversationId, cancellationToken)
                .ConfigureAwait(false);
        }
        catch (LocalAgentHostRequestException exception) when (exception.Code == "not_found")
        {
            return null;
        }
    }

    public async IAsyncEnumerable<PetActivityEvent> StreamPetActivitiesAsync(
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        var owner = RequireOwner();
        yield return new PetActivityEvent.Reconcile();
        await foreach (var update in _eventHub.UpdatesAsync(cancellationToken).ConfigureAwait(false))
        {
            if (!string.Equals(update.OwnerUserId, owner, StringComparison.Ordinal)) continue;
            if (update.IsReconcile || ShouldRefreshPet(update.Events))
                yield return new PetActivityEvent.Reconcile();
        }
    }

    internal static bool EventsAffectConversation(
        IReadOnlyList<WindowsLocalAgentEvent> events,
        string conversationId,
        IReadOnlySet<string> knownRunIds) => events.Any(value =>
            knownRunIds.Contains(value.RunId) ||
            value.Payload is { ValueKind: System.Text.Json.JsonValueKind.Object } payload &&
            payload.TryGetProperty("conversation_id", out var routed) &&
            routed.ValueKind == System.Text.Json.JsonValueKind.String &&
            string.Equals(routed.GetString(), conversationId, StringComparison.Ordinal));

    internal static bool ShouldRefreshPet(IReadOnlyList<WindowsLocalAgentEvent> events)
    {
        var neutral = new HashSet<string>(StringComparer.Ordinal)
        {
            "task_graph_written_back",
            "task_state_reconciled",
            "tool_invocation_approved",
            "tool_invocation_claimed",
            "tool_invocation_completed",
        };
        return events.Any(value => !neutral.Contains(value.EventType));
    }

    private static ConversationRealtimeSignal Signal(
        string conversationId,
        WindowsLocalConversationDetail detail) => new(
            $"local-{conversationId}-{detail.Conversation.Version}",
            Clamp(detail.Conversation.Version),
            conversationId,
            null,
            ConversationRealtimeKind.Updated,
            "local_conversation_changed",
            DateTimeOffset.FromUnixTimeMilliseconds(detail.Conversation.UpdatedAtUnixMs));

    private string RequireOwner()
    {
        lock (_gate)
        {
            return _ownerUserId ?? throw new InvalidOperationException(
                "Local Agent realtime is not configured.");
        }
    }

    private static long Clamp(ulong value) =>
        value > long.MaxValue ? long.MaxValue : (long)value;
}
