using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentRealtimeClientTests
{
    [Fact]
    public async Task ConversationStreamReadsAgainOnlyAfterLocalEventCursorAdvances()
    {
        var host = new RealtimeHost();
        var client = CreateClient(host);
        client.Configure("owner-1");
        using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        await using var iterator = client.StreamConversationAsync(
            "conversation-1", cancellation.Token).GetAsyncEnumerator(cancellation.Token);

        Assert.True(await iterator.MoveNextAsync());
        Assert.Equal(1, iterator.Current.EventSequence);
        Assert.True(await iterator.MoveNextAsync());
        Assert.Equal(2, iterator.Current.EventSequence);
        Assert.Equal(
            ["get_conversation", "list_events", "get_conversation"],
            host.CommandTypes);
    }

    [Fact]
    public async Task PetStreamReconcilesFromLocalEventPages()
    {
        var host = new RealtimeHost();
        var client = CreateClient(host);
        client.Configure("owner-1");
        using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        await using var iterator = client.StreamPetActivitiesAsync(
            cancellation.Token).GetAsyncEnumerator(cancellation.Token);

        Assert.True(await iterator.MoveNextAsync());
        Assert.IsType<PetActivityEvent.Reconcile>(iterator.Current);
        Assert.True(await iterator.MoveNextAsync());
        Assert.IsType<PetActivityEvent.Reconcile>(iterator.Current);
        Assert.Equal(["list_events"], host.CommandTypes);
    }

    private static WindowsLocalAgentRealtimeClient CreateClient(ILocalAgentHostClient host) =>
        new(new WindowsLocalAgentConversationClient(host), new WindowsLocalAgentRuntimeClient(host));

    private sealed class RealtimeHost : ILocalAgentHostClient
    {
        private int _conversationReads;
        public List<string> CommandTypes { get; } = [];
        public string? ActiveOwnerUserId => "owner-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            object response = command switch
            {
                GetLocalConversationCommand => Conversation(),
                ListLocalEventsCommand events => Events(events.AfterCursor),
                _ => throw new InvalidOperationException(
                    $"Unexpected realtime command: {typeof(TCommand).Name}"),
            };
            return Task.FromResult((TResponse)response);
        }

        private LocalConversationResult Conversation()
        {
            CommandTypes.Add("get_conversation");
            var version = (ulong)++_conversationReads;
            return new LocalConversationResult(
                "conversation",
                new WindowsLocalConversationDetail(
                    new WindowsLocalConversationRecord(
                        "conversation-1", "owner-1", "Conversation", version, 1, (long)version),
                    [],
                    [],
                    []));
        }

        private ListLocalEventsResult Events(long afterCursor)
        {
            CommandTypes.Add("list_events");
            return new ListLocalEventsResult(
                "events",
                afterCursor == 0
                    ? [new WindowsLocalAgentEvent(
                        1,
                        "event-1",
                        "run-1",
                        "run_updated",
                        JsonSerializer.SerializeToElement(new { }),
                        1)]
                    : [],
                afterCursor == 0 ? 1 : afterCursor);
        }
    }
}
