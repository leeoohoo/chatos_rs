using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentAskUserPromptTests
{
    [Fact]
    public async Task ReadsPendingPromptFromLocalRunEvents()
    {
        var host = new AskUserHost();
        var service = new WindowsLocalAgentAskUserPromptService(
            new WindowsLocalAgentRuntimeClient(host),
            new WindowsLocalAgentConversationClient(host));
        service.Configure("user-1");

        var prompts = await service.FetchPromptsAsync("conversation-1");

        var prompt = Assert.Single(prompts);
        Assert.Equal("local-ask:run-1", prompt.Id);
        Assert.Equal("需要确认", prompt.Title);
        Assert.Equal("继续执行吗？", prompt.Message);
        Assert.True(prompt.IsPending);
    }

    private sealed class AskUserHost : ILocalAgentHostClient
    {
        private static readonly WindowsLocalAgentRun Run = new(
            "run-1", "user-1", "conversation_turn", "turn-1", "main_chat",
            JsonSerializer.SerializeToElement(new {
                conversation_id = "conversation-1", turn_id = "turn-1",
            }),
            "waiting_user", 3, null, 1_000, 2_000);

        public string? ActiveOwnerUserId => "user-1";
        public Task StartForOwnerAsync(string ownerUserId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
        public Task RestartForOwnerAsync(
            string ownerUserId, IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command, CancellationToken cancellationToken = default) where TCommand : notnull
        {
            object response = command switch
            {
                ListLocalRunsCommand => new ListLocalRunsResult(
                    "runs", new WindowsLocalAgentRunPage([Run], null, null)),
                ListLocalEventsCommand => new ListLocalEventsResult(
                    "events",
                    [new WindowsLocalAgentEvent(
                        1, "event-1", "run-1", "user_input_requested",
                        JsonSerializer.SerializeToElement(new {
                            prompt = new {
                                title = "需要确认", message = "继续执行吗？",
                                allow_cancel = true,
                            },
                        }),
                        1_500)],
                    1),
                _ => throw new InvalidOperationException($"Unexpected command: {typeof(TCommand).Name}"),
            };
            return Task.FromResult((TResponse)response);
        }
    }
}
