using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentControlPlaneTests
{
    [Fact]
    public async Task LatestSnapshotsUseOwnerScopedProtocolCommands()
    {
        var host = new ControlPlaneHost();
        var client = new WindowsLocalAgentControlPlaneClient(host);

        var models = await client.LatestModelsAsync("user-1");
        var capabilities = await client.LatestCapabilitiesAsync("user-1", "main_chat");

        Assert.Single(models);
        Assert.Equal("model-1", models[0].ModelConfigRef);
        Assert.Equal("user-1", capabilities.OwnerUserId);
        Assert.Equal("main_chat", capabilities.ProfileKey);
        Assert.Equal(
            ["list_latest_model_config_snapshots", "get_latest_capability_policy_snapshot"],
            host.Commands.Select(command => command.GetProperty("type").GetString()).ToArray());
        Assert.All(host.Commands, command =>
            Assert.Equal("user-1", command.GetProperty("owner_user_id").GetString()));
        Assert.Equal("main_chat", host.Commands[1].GetProperty("profile_key").GetString());
    }

    [Fact]
    public void ModelSnapshotProtocolContainsReferenceButNoCredentialValue()
    {
        var snapshot = new WindowsLocalAgentModelSnapshot(
            "user-1",
            "model-1",
            "sha256-revision",
            "env:CHATOS_LOCAL_AGENT_MODEL_MODEL_1",
            "https://example.invalid/v1",
            "model-1",
            "openai",
            true,
            false,
            null,
            0.7,
            4096,
            "medium",
            false,
            null,
            null,
            null);

        var json = JsonSerializer.Serialize(snapshot, new JsonSerializerOptions
        {
            PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        });

        Assert.Contains("credential_ref", json);
        Assert.DoesNotContain("api_key", json);
        Assert.DoesNotContain("test-secret", json);
    }

    [Fact]
    public void MainChatCatalogOnlyPublishesTaskSchemaOverlays()
    {
        var names = WindowsLocalAgentCapabilityCatalog.MainChatTools
            .Select(tool => tool.GetProperty("name").GetString())
            .ToArray();

        Assert.Equal(
            ["create_task", "create_tasks_with_prerequisites"],
            names);
        Assert.All(WindowsLocalAgentCapabilityCatalog.MainChatTools, tool =>
            Assert.Equal("function", tool.GetProperty("type").GetString()));
    }

    [Fact]
    public void MainChatTaskSchemaFreezesInstalledPluginChoices()
    {
        var tools = WindowsLocalAgentCapabilityCatalog.MainChatToolsFor(
        [
            new("open-computer-use@chatos-marketplace", "Computer Use", "Desktop control."),
            new("chatos-browser-cdp@chatos-marketplace", "Browser CDP", "Browser pages."),
        ]);
        var create = tools.Single(tool =>
            tool.GetProperty("name").GetString() == "create_task");
        var values = create
            .GetProperty("parameters")
            .GetProperty("properties")
            .GetProperty("plugin_hints")
            .GetProperty("items")
            .GetProperty("properties")
            .GetProperty("plugin_key")
            .GetProperty("enum")
            .EnumerateArray()
            .Select(value => value.GetString())
            .ToArray();

        Assert.Equal(
            ["chatos-browser-cdp@chatos-marketplace", "open-computer-use@chatos-marketplace"],
            values);
        var titles = create
            .GetProperty("parameters")
            .GetProperty("properties")
            .GetProperty("plugin_hints")
            .GetProperty("items")
            .GetProperty("properties")
            .GetProperty("plugin_key")
            .GetProperty("oneOf")
            .EnumerateArray()
            .Select(value => value.GetProperty("title").GetString()!)
            .ToArray();
        Assert.Contains(titles, title =>
            title.Contains("Browser pages.", StringComparison.Ordinal) &&
            title.Contains("only for websites", StringComparison.Ordinal));
        Assert.Contains(titles, title =>
            title.Contains("Desktop control.", StringComparison.Ordinal) &&
            title.Contains("native desktop applications", StringComparison.Ordinal));
    }

    [Fact]
    public void TaskExecutionCatalogPublishesProjectAndTerminalTools()
    {
        var names = WindowsLocalAgentCapabilityCatalog.TaskExecutionTools
            .Select(tool => tool.GetProperty("name").GetString()!)
            .ToArray();

        Assert.Equal(
            new[] {
                "local_attachment_read", "project_list", "project_read", "project_search",
                "project_write", "terminal_exec",
                "capability_search", "capability_describe", "capability_skill_activate",
                "capability_skill_read_resource", "capability_invoke",
                "remote_connection_controller_test_connection",
                "remote_connection_controller_run_command",
                "remote_connection_controller_list_directory",
                "remote_connection_controller_read_file",
                "remote_connection_controller_download_file",
                "remote_connection_controller_upload_file",
            },
            names);
        Assert.True(names.ToHashSet().SetEquals(
            WindowsLocalAgentCapabilityCatalog.TaskExecutionToolNames));
        Assert.All(WindowsLocalAgentCapabilityCatalog.TaskExecutionTools, tool =>
        {
            Assert.Equal("function", tool.GetProperty("type").GetString());
            Assert.Equal(JsonValueKind.Object, tool.GetProperty("parameters").ValueKind);
        });
        Assert.True(new[] {
            "project_list", "project_read", "project_search", "project_write", "terminal_exec",
            "remote_connection_controller_test_connection",
            "remote_connection_controller_run_command",
            "remote_connection_controller_list_directory",
            "remote_connection_controller_read_file",
            "remote_connection_controller_download_file",
            "remote_connection_controller_upload_file",
        }.ToHashSet().SetEquals(WindowsLocalAgentCapabilityCatalog.ProjectToolNames));
        Assert.True(new[] {
            "capability_search", "capability_describe", "capability_skill_activate",
            "capability_skill_read_resource", "capability_invoke",
        }.ToHashSet().SetEquals(WindowsLocalAgentCapabilityCatalog.PluginToolNames));
    }

    [Fact]
    public void TaskToolAuthorizationPreservesTheSelectedTaskScope()
    {
        using var document = JsonDocument.Parse("""
        {
          "tool_options": {
            "requires_execution": true,
            "enabled_builtin_kinds": ["CodeMaintainerRead", "CodeMaintainerWrite"],
            "plugin_hints": [{"plugin_key": "plugin-1"}]
          }
        }
        """);

        var authorization = WindowsLocalAgentTaskToolAuthorization.Resolve(
            document.RootElement);

        Assert.True(authorization.Allows("project_read"));
        Assert.True(authorization.Allows("project_write"));
        Assert.False(authorization.Allows("terminal_exec"));
        Assert.True(authorization.Allows("capability_search"));
        Assert.True(authorization.PluginKeys.SetEquals(["plugin-1"]));
    }

    private sealed class ControlPlaneHost : ILocalAgentHostClient
    {
        private static readonly JsonSerializerOptions JsonOptions = new()
        {
            PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        };

        public List<JsonElement> Commands { get; } = [];

        public string? ActiveOwnerUserId => "user-1";

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
            var encoded = JsonSerializer.SerializeToElement(command, JsonOptions);
            Commands.Add(encoded);
            var type = encoded.GetProperty("type").GetString();
            object result = type switch
            {
                "list_latest_model_config_snapshots" => new
                {
                    type = "model_config_snapshots",
                    snapshots = new[] { ModelSnapshot() },
                },
                "get_latest_capability_policy_snapshot" => new
                {
                    type = "capability_policy_snapshot",
                    snapshot = CapabilitySnapshot(),
                },
                _ => throw new InvalidOperationException($"Unexpected command: {type}"),
            };
            var json = JsonSerializer.Serialize(result, JsonOptions);
            return Task.FromResult(JsonSerializer.Deserialize<TResponse>(json, JsonOptions)!);
        }

        private static WindowsLocalAgentModelSnapshot ModelSnapshot() => new(
            "user-1", "model-1", "revision-1",
            "env:CHATOS_LOCAL_AGENT_MODEL_MODEL_1",
            "https://example.invalid/v1", "gpt-test", "openai", true, false,
            null, null, null, "medium", false, null, null, null);

        private static WindowsLocalAgentCapabilitySnapshot CapabilitySnapshot() => new(
            "user-1", "main_chat", "policy-1", "Use task tools.", [], []);
    }
}
