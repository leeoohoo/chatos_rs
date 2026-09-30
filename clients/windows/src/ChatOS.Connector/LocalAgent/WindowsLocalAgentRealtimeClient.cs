using System.Runtime.CompilerServices;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentRealtimeClient : IRealtimeClient
{
    private readonly WindowsLocalAgentConversationClient _conversations;
    private readonly object _gate = new();
    private string? _ownerUserId;

    public WindowsLocalAgentRealtimeClient(WindowsLocalAgentConversationClient conversations)
    {
        _conversations = conversations;
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
        while (!cancellationToken.IsCancellationRequested)
        {
            try
            {
                var detail = await _conversations.GetAsync(
                    owner,
                    conversationId,
                    cancellationToken).ConfigureAwait(false);
                if (observedVersion != detail.Conversation.Version)
                {
                    observedVersion = detail.Conversation.Version;
                    yield return new ConversationRealtimeSignal(
                        $"local-{conversationId}-{observedVersion}",
                        Clamp(observedVersion.Value),
                        conversationId,
                        null,
                        ConversationRealtimeKind.Updated,
                        "local_conversation_changed",
                        DateTimeOffset.FromUnixTimeMilliseconds(
                            detail.Conversation.UpdatedAtUnixMs));
                }
            }
            catch (LocalAgentHostRequestException exception) when (exception.Code == "not_found")
            {
            }
            await Task.Delay(TimeSpan.FromMilliseconds(400), cancellationToken)
                .ConfigureAwait(false);
        }
    }

    public async IAsyncEnumerable<PetActivityEvent> StreamPetActivitiesAsync(
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        _ = RequireOwner();
        while (!cancellationToken.IsCancellationRequested)
        {
            yield return new PetActivityEvent.Reconcile();
            await Task.Delay(TimeSpan.FromSeconds(2), cancellationToken).ConfigureAwait(false);
        }
    }

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
