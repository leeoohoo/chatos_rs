using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentTaskClientTests
{
    [Fact]
    public async Task RestartUsesCurrentTaskVersionAndReason()
    {
        var host = new TaskHost();
        var client = new WindowsLocalAgentTaskClient(host);
        var task = new WindowsLocalTask(
            "graph-1", "owner-1", "conversation_turn", "turn-1", "task-1",
            "Task", "model-1", JsonSerializer.SerializeToElement(new { }), "running",
            "run-1", 7, 1, 2);

        var graph = await client.RestartAsync(
            "owner-1", task, "restart from the beginning", CancellationToken.None);

        Assert.Equal("graph-1", graph.GraphId);
        var command = Assert.IsType<RestartLocalTaskCommand>(host.Command);
        Assert.Equal("restart_task", command.Type);
        Assert.Equal("owner-1", command.OwnerUserId);
        Assert.Equal("task-1", command.TaskId);
        Assert.Equal(7UL, command.ExpectedVersion);
        Assert.Equal("restart from the beginning", command.Reason);
    }

    [Fact]
    public async Task MatchingGraphsPushesConversationTurnFilterIntoHostQuery()
    {
        var host = new TaskHost();
        var client = new WindowsLocalAgentTaskClient(host);

        _ = await client.MatchingGraphsAsync(
            "owner-1", "turn-1", null, CancellationToken.None);

        var command = Assert.IsType<ListLocalTaskGraphsCommand>(host.Command);
        Assert.Equal("conversation_turn", command.SourceEntityType);
        Assert.Equal("turn-1", command.SourceEntityId);
    }

    private sealed class TaskHost : ILocalAgentHostClient
    {
        public object? Command { get; private set; }
        public string? ActiveOwnerUserId => "owner-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> environment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            Command = command;
            if (command is ListLocalTaskGraphsCommand)
            {
                object list = new LocalTaskGraphsResult(
                    "task_graphs", new WindowsLocalTaskGraphPage([], null, null));
                return Task.FromResult((TResponse)list);
            }
            object response = new LocalTaskGraphTypedResult("task_graph", new WindowsLocalTaskGraph(
                "graph-1", "owner-1", "conversation_turn", "turn-1", "running", [], [], 1));
            return Task.FromResult((TResponse)response);
        }
    }
}
