using System.Text.Json;
using ChatOS.Connector.AgentTeams;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.Tests;

public sealed class AgentPluginToolRuntimeTests
{
    [Fact]
    public async Task RunSessionRelaysNamespacedToolAndStopsPlugin()
    {
        var directory = Path.Combine(Path.GetTempPath(), "chatos-agent-plugin-tests",
            Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            var sessions = new PluginRuntimeSessionStore();
            var client = new RecordingPluginClient();
            var identity = new PluginRuntimeIdentity("run-1", "plugin.example", "release-1",
                "1.0.0", new string('a', 64), "server", "session-1", "workspace-1",
                "project-1");
            using var definitionDocument = JsonDocument.Parse("""
                {"name":"echo","description":"Echo","inputSchema":{"type":"object"}}
                """);
            await sessions.InsertAsync(identity, client,
                [definitionDocument.RootElement.Clone()], new HashSet<string>(), false,
                directory, directory, directory, "Example");
            var definition = new AgentToolDefinition("mcp_0_0_0_echo", "Echo",
                JsonSerializer.SerializeToElement(new { type = "object" }));
            var binding = new AgentPluginToolRuntime.PluginBinding(identity, "session-1",
                directory, "workspace-1", "project-1", "Follow plugin instructions.",
                [new("mcp_0_0_0_echo", "echo", definition, PluginToolPolicy.Parse(
                    definitionDocument.RootElement))]);

            await using (var run = new AgentPluginRunSession(sessions,
                new PluginArtifactRegistry(), "alice", "device-1", [binding]))
            {
                Assert.Single(run.Definitions);
                Assert.Contains("Follow plugin instructions.", run.Instructions,
                    StringComparison.Ordinal);
                var result = await run.ExecuteAsync(new AgentToolCall(
                    "call-1", "mcp_0_0_0_echo", "{\"value\":\"hello\"}"),
                    CancellationToken.None);
                Assert.Contains("hello", result, StringComparison.Ordinal);
                Assert.Equal("echo", client.ToolName);
            }

            Assert.True(client.Terminated);
        }
        finally
        {
            if (Directory.Exists(directory)) Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task RunSessionRejectsNonObjectArgumentsBeforePluginInvocation()
    {
        var directory = Path.Combine(Path.GetTempPath(), "chatos-agent-plugin-tests",
            Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            var sessions = new PluginRuntimeSessionStore();
            var client = new RecordingPluginClient();
            var identity = new PluginRuntimeIdentity("run-1", "plugin.example", "release-1",
                "1.0.0", new string('a', 64), "server", "session-2", null);
            await sessions.InsertAsync(identity, client, [], new HashSet<string>(), false,
                directory, directory, directory, "Example");
            var definition = new AgentToolDefinition("mcp_0_0_0_echo", "Echo", new { });
            var binding = new AgentPluginToolRuntime.PluginBinding(identity, "session-2",
                directory, null, null, null,
                [new("mcp_0_0_0_echo", "echo", definition, PluginToolPolicy.Parse(
                    JsonSerializer.SerializeToElement(new { }))) ]);
            await using var run = new AgentPluginRunSession(sessions,
                new PluginArtifactRegistry(), "alice", "device-1", [binding]);

            var error = await Assert.ThrowsAsync<ChatOS.Core.Domain.AgentTeamException>(() =>
                run.ExecuteAsync(new AgentToolCall(
                    "call-1", "mcp_0_0_0_echo", "[]"), CancellationToken.None));

            Assert.Equal(ChatOS.Core.Domain.AgentTeamError.InvalidField, error.Code);
            Assert.Null(client.ToolName);
        }
        finally
        {
            if (Directory.Exists(directory)) Directory.Delete(directory, recursive: true);
        }
    }

    private sealed class RecordingPluginClient : IPluginMcpClient
    {
        public string? ToolName { get; private set; }
        public bool Terminated { get; private set; }

        public Task StartAsync(CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task<PluginMcpInitialization> InitializeAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new PluginMcpInitialization(null, []));

        public Task<JsonElement> CallToolAsync(
            string name,
            JsonElement arguments,
            TimeSpan timeout,
            CancellationToken cancellationToken = default)
        {
            ToolName = name;
            return Task.FromResult(JsonSerializer.SerializeToElement(new
            {
                echoed = arguments.GetProperty("value").GetString(),
            }));
        }

        public Task TerminateAsync()
        {
            Terminated = true;
            return Task.CompletedTask;
        }

        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }
}
