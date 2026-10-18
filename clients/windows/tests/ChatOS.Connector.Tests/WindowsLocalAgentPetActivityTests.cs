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
        Assert.Equal(PetActivitySource.AskUserPrompt, activity.Source);
        Assert.Equal(PetActivityKind.WaitingForUser, activity.Kind);
        Assert.Equal("conversation-1", activity.Route.ConversationId);
        Assert.Equal("local-ask:run-1", activity.Route.PromptId);
    }

    [Fact]
    public async Task WaitingTaskRunIsMappedAsAnAskUserActivitySoPetCanLoadTheForm()
    {
        var run = new WindowsLocalAgentRun(
            "task-run-1", "user-1", "task", "task-1", "task_execution",
            JsonSerializer.SerializeToElement(new
            {
                source_conversation_id = "conversation-1",
                source_turn_id = "turn-1",
            }),
            "waiting_user", 7, null, 1_000, DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
        var service = new WindowsLocalAgentPetActivityService(
            new WindowsLocalAgentRuntimeClient(new RunHost(run)));
        service.Configure("user-1");

        var activity = Assert.Single(await service.FetchOpenActivitiesAsync());

        Assert.Equal(PetActivitySource.AskUserPrompt, activity.Source);
        Assert.Equal("需要你的回复", activity.Title);
        Assert.Equal("conversation-1", activity.Route.ConversationId);
        Assert.Equal("turn-1", activity.Route.TurnId);
        Assert.Equal("local-ask:task-run-1", activity.Route.PromptId);
        Assert.Equal("task-1", activity.Route.TaskId);
    }

    private sealed class RunHost : ILocalAgentHostClient
    {
        private readonly WindowsLocalAgentRun _run;

        public RunHost(WindowsLocalAgentRun? run = null)
        {
            _run = run ?? new WindowsLocalAgentRun(
                "run-1", "user-1", "conversation_turn", "turn-1", "main_chat",
                JsonSerializer.SerializeToElement(new
                {
                    conversation_id = "conversation-1", turn_id = "turn-1",
                }),
                "waiting_user", 4, null, 1_000, DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
        }

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
                ? [_run]
                : [];
            object response = new ListLocalRunsResult(
                "runs", new WindowsLocalAgentRunPage(runs, null, null));
            return Task.FromResult((TResponse)response);
        }
    }
}
