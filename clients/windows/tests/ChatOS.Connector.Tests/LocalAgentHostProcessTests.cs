using ChatOS.Connector.LocalAgent;

namespace ChatOS.Connector.Tests;

public sealed class LocalAgentHostProcessTests
{
    [Fact]
    public void MemoryCredentialIsChildEnvironmentOnly()
    {
        var options = new LocalAgentHostOptions(
            @"C:\ChatOS\chatos_local_agent_host.exe",
            @"C:\ChatOS\local-agent.sqlite3",
            TimeSpan.FromSeconds(10),
            new Uri("https://gateway.example/api/memory"),
            "local_agent",
            TimeSpan.FromMilliseconds(12_345));
        var start = LocalAgentHostProcessLauncher.CreateStartInfo(
            options,
            "user-1",
            new Dictionary<string, string>
            {
                ["CHATOS_LOCAL_AGENT_MODEL_MODEL_1"] = "model-secret",
                ["CHATOS_MEMORY_ACCESS_TOKEN"] = "memory-secret",
            });

        Assert.Contains("--memory-base-url", start.ArgumentList);
        Assert.Contains("https://gateway.example/api/memory", start.ArgumentList);
        Assert.Contains("--memory-source-id", start.ArgumentList);
        Assert.Contains("local_agent", start.ArgumentList);
        Assert.Contains("12345", start.ArgumentList);
        Assert.DoesNotContain("memory-secret", start.ArgumentList);
        Assert.Equal("memory-secret", start.Environment["CHATOS_MEMORY_ACCESS_TOKEN"]);
        Assert.Equal("model-secret", start.Environment["CHATOS_LOCAL_AGENT_MODEL_MODEL_1"]);
        var arguments = start.ArgumentList.ToArray();
        AssertReadOnly(arguments, "local_attachment_read");
        AssertReadOnly(arguments, "project_list");
        AssertReadOnly(arguments, "project_read");
        AssertReadOnly(arguments, "project_search");
        AssertReadOnly(arguments, "capability_search");
        AssertReadOnly(arguments, "capability_describe");
        AssertReadOnly(arguments, "capability_skill_activate");
        AssertReadOnly(arguments, "capability_skill_read_resource");
        AssertApprovalExempt(arguments, "capability_invoke");
        Assert.DoesNotContain("project_write", arguments);
        Assert.DoesNotContain("terminal_exec", arguments);
    }

    private static void AssertReadOnly(IReadOnlyList<string> arguments, string toolName)
    {
        var index = Array.IndexOf(arguments.ToArray(), toolName);
        Assert.True(index > 0);
        Assert.Equal("--read-only-tool", arguments[index - 1]);
    }

    private static void AssertApprovalExempt(IReadOnlyList<string> arguments, string toolName)
    {
        var index = Array.IndexOf(arguments.ToArray(), toolName);
        Assert.True(index > 0);
        Assert.Equal("--approval-exempt-tool", arguments[index - 1]);
    }
}
