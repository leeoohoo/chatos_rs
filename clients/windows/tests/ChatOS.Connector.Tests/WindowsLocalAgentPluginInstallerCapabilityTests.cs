using ChatOS.Connector.LocalAgent;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentPluginInstallerCapabilityTests
{
    [Fact]
    public void InstallerCapabilityContainsOnlyDedicatedInstallerTools()
    {
        var names = WindowsLocalAgentCapabilityCatalog.PluginInstallerTools
            .Select(value => value.GetProperty("name").GetString())
            .ToHashSet(StringComparer.Ordinal);

        Assert.True(names.SetEquals(
            WindowsLocalAgentCapabilityCatalog.PluginInstallerToolNames));
        Assert.DoesNotContain("create_task", names);
        Assert.DoesNotContain("project_read", names);
        Assert.DoesNotContain("project_write", names);
        Assert.DoesNotContain("terminal_exec", names);
        Assert.Contains("不要创建项目或任务", WindowsLocalAgentCapabilityCatalog.PluginInstallerInstructions);
        Assert.Contains("rea-agents", WindowsLocalAgentCapabilityCatalog.PluginInstallerInstructions);
        Assert.Contains("Git 获取禁 hooks", WindowsLocalAgentCapabilityCatalog.PluginInstallerInstructions);
        Assert.Contains(WindowsLocalAgentCapabilityCatalog.PluginInstallerInspectNpmToolName, names);
        Assert.Contains(WindowsLocalAgentCapabilityCatalog.PluginInstallerInstallNpmToolName, names);

        var inspectSource = WindowsLocalAgentCapabilityCatalog.PluginInstallerTools.Single(value =>
            value.GetProperty("name").GetString() ==
            WindowsLocalAgentCapabilityCatalog.PluginInstallerInspectSourceToolName);
        var properties = inspectSource.GetProperty("parameters").GetProperty("properties");
        Assert.Contains("git_repository", properties.GetProperty("source_kind")
            .GetProperty("enum").EnumerateArray().Select(value => value.GetString()));
        Assert.True(properties.TryGetProperty("git_reference", out _));
        Assert.True(properties.TryGetProperty("subdirectory", out _));
    }

    [Fact]
    public void OrdinaryMainChatAndTaskCatalogsNeverPublishInstallerTools()
    {
        var ordinary = WindowsLocalAgentCapabilityCatalog.MainChatTools
            .Select(value => value.GetProperty("name").GetString())
            .Where(value => value is not null)
            .Select(value => value!)
            .ToHashSet(StringComparer.Ordinal);
        var task = WindowsLocalAgentCapabilityCatalog.TaskExecutionToolNames;

        Assert.Empty(ordinary.Intersect(
            WindowsLocalAgentCapabilityCatalog.PluginInstallerToolNames,
            StringComparer.Ordinal));
        Assert.Empty(task.Intersect(
            WindowsLocalAgentCapabilityCatalog.PluginInstallerToolNames,
            StringComparer.Ordinal));
    }

    [Fact]
    public void InstallerRevisionIsStableAndBoundToMainCapabilityRevision()
    {
        var first = WindowsLocalAgentBootstrapService.PluginInstallerCapabilityRevision("main-1");
        var repeat = WindowsLocalAgentBootstrapService.PluginInstallerCapabilityRevision("main-1");
        var changed = WindowsLocalAgentBootstrapService.PluginInstallerCapabilityRevision("main-2");

        Assert.Equal(first, repeat);
        Assert.NotEqual(first, changed);
        Assert.StartsWith("plugin-installer-", first);
    }
}
