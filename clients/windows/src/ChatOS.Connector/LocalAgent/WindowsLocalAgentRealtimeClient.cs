using System.Runtime.CompilerServices;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentRealtimeClient : IRealtimeClient
{
    private readonly WindowsLocalAgentConversationClient _conversations;
    private readonly WindowsLocalAgentRuntimeClient _runtime;
    private readonly object _gate = new();
    private string? _ownerUserId;

    public WindowsLocalAgentRealtimeClient(
        WindowsLocalAgentConversationClient conversations,
        WindowsLocalAgentRuntimeClient runtime)
    {
        _conversations = conversations;
        _runtime = runtime;
    }

    public void Configure(string ownerUserId)
    {
        lock (_gate) _ownerUserId = ownerUserId;
    }

    public void Reset()
    {
        lock (_gate) _ownerUserId = null;
    }

    public async IAsyncEnumerable<ConversationRealtimeSignal> StreamConversationAsync(
        string conversationId,
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(conversationId);
        var owner = RequireOwner();
        ulong? observedVersion = null;
        long cursor = 0;
        while (!cancellationToken.IsCancellationRequested)
        {
            try
            {
                if (observedVersion is null)
                {
                    var detail = await _conversations.GetAsync(
                        owner, conversationId, cancellationToken).ConfigureAwait(false);
                    observedVersion = detail.Conversation.Version;
                    yield return Signal(conversationId, detail);
                }

                var page = await _runtime.ListEventPageAsync(
                    owner, cursor, cancellationToken: cancellationToken).ConfigureAwait(false);
                cursor = page.NextCursor;
                if (page.Events.Count == 0)
                {
                    await Task.Delay(TimeSpan.FromMilliseconds(400), cancellationToken)
                        .ConfigureAwait(false);
                    continue;
                }

                var current = await _conversations.GetAsync(
                    owner, conversationId, cancellationToken).ConfigureAwait(false);
                if (observedVersion == current.Conversation.Version) continue;
                observedVersion = current.Conversation.Version;
                yield return Signal(conversationId, current);
            }
            catch (LocalAgentHostRequestException exception) when (exception.Code == "not_found")
            {
                await Task.Delay(TimeSpan.FromMilliseconds(400), cancellationToken)
                    .ConfigureAwait(false);
            }
        }
    }

    public async IAsyncEnumerable<PetActivityEvent> StreamPetActivitiesAsync(
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        var owner = RequireOwner();
        long cursor = 0;
        yield return new PetActivityEvent.Reconcile();
        while (!cancellationToken.IsCancellationRequested)
        {
            var page = await _runtime.ListEventPageAsync(
                owner, cursor, cancellationToken: cancellationToken).ConfigureAwait(false);
            cursor = page.NextCursor;
            if (page.Events.Count != 0)
            {
                yield return new PetActivityEvent.Reconcile();
                continue;
            }
            await Task.Delay(TimeSpan.FromMilliseconds(400), cancellationToken)
                .ConfigureAwait(false);
        }
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
