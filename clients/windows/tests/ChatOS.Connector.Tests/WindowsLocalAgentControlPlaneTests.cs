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
    public void MainChatCatalogPublishesAttachmentAndRustReservedTaskTools()
    {
        var names = WindowsLocalAgentCapabilityCatalog.MainChatTools
            .Select(tool => tool.GetProperty("name").GetString())
            .ToArray();

        Assert.Equal(
            new string?[] {
                "local_attachment_read", "create_task", "create_tasks_with_prerequisites",
            },
            names);
        Assert.All(WindowsLocalAgentCapabilityCatalog.MainChatTools, tool =>
            Assert.Equal("function", tool.GetProperty("type").GetString()));
    }

    [Fact]
    public void TaskExecutionCatalogPublishesProjectAndTerminalTools()
    {
        var names = WindowsLocalAgentCapabilityCatalog.TaskExecutionTools
            .Select(tool => tool.GetProperty("name").GetString()!)
            .ToArray();

        Assert.Equal(
            new[] {
                "project_list", "project_read", "project_search", "project_write", "terminal_exec",
            },
            names);
        Assert.True(names.ToHashSet().SetEquals(
            WindowsLocalAgentCapabilityCatalog.TaskExecutionToolNames));
        Assert.All(WindowsLocalAgentCapabilityCatalog.TaskExecutionTools, tool =>
        {
            Assert.Equal("function", tool.GetProperty("type").GetString());
            Assert.Equal(JsonValueKind.Object, tool.GetProperty("parameters").ValueKind);
        });
    }
}
