using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentPetActivityTests
{
    [Fact]
    public async Task MapsLocalRunsWithoutCallingThePetBackend()
    {
        var host = new RunHost();
        var service = new WindowsLocalAgentPetActivityService(
            new WindowsLocalAgentRuntimeClient(host));
        service.Configure("user-1");

        var activities = await service.FetchOpenActivitiesAsync();

        var activity = Assert.Single(activities);
        Assert.Equal(PetActivitySource.Chat, activity.Source);
        Assert.Equal(PetActivityKind.WaitingForUser, activity.Kind);
        Assert.Equal("conversation-1", activity.Route.ConversationId);
        Assert.Equal("local-ask:run-1", activity.Route.PromptId);
    }

    private sealed class RunHost : ILocalAgentHostClient
    {
        public string? ActiveOwnerUserId => "user-1";

        public Task StartForOwnerAsync(string ownerUserId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default) where TCommand : notnull
        {
            var list = Assert.IsType<ListLocalRunsCommand>(command);
            IReadOnlyList<WindowsLocalAgentRun> runs = list.Scope == "active"
                ? [new WindowsLocalAgentRun(
                    "run-1", "user-1", "conversation_turn", "turn-1", "main_chat",
                    JsonSerializer.SerializeToElement(new {
                        conversation_id = "conversation-1", turn_id = "turn-1",
                    }),
                    "waiting_user", 4, null, 1_000, DateTimeOffset.UtcNow.ToUnixTimeMilliseconds())]
                : [];
            object response = new ListLocalRunsResult(
                "runs", new WindowsLocalAgentRunPage(runs, null, null));
            return Task.FromResult((TResponse)response);
        }
    }
}
