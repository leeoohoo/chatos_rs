using System.Text.Json;
using ChatOS.Connector.LocalAgent;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentControlPlaneTests
{
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
            new("plugin-2", "Plugin Two", "second"),
            new("plugin-1", "Plugin One", "first"),
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

        Assert.Equal(["plugin-1", "plugin-2"], values);
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
}
