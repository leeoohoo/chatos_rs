using ChatOS.Connector.LocalAgent;

namespace ChatOS.Connector.Tests;

public sealed class WindowsAgentCapabilityAuthorizationTests
{
    [Fact]
    public void CatalogOnlyStateCannotAuthorizePluginInvocation()
    {
        var error = Assert.Throws<WindowsAgentCapabilityAuthorizationException>(() =>
            WindowsAgentCapabilityAuthorization.RequireDescribedTool(
                new HashSet<string>(StringComparer.Ordinal), "tool_1"));

        Assert.Contains("described", error.Message, StringComparison.OrdinalIgnoreCase);
        Assert.Equal(error.Message,
            WindowsLocalAgentPlatformToolWorker.SafeErrorSummary(error));
    }

    [Fact]
    public void DescribingAnotherToolDoesNotRevokeAnEarlierTool()
    {
        var described = new HashSet<string>(StringComparer.Ordinal)
        {
            "tool_1",
            "tool_2",
        };

        WindowsAgentCapabilityAuthorization.RequireDescribedTool(described, "tool_1");
        WindowsAgentCapabilityAuthorization.RequireDescribedTool(described, "tool_2");
    }

    [Fact]
    public void SkillActivationRequiresExactRunScopedReferenceAndDescribedTool()
    {
        var options = new HashSet<string>(StringComparer.Ordinal) { "plugin_1" };
        var identity = WindowsAgentCapabilityAuthorization.ParseSkillRef(
            "plugin-skill:plugin_1:navigation", options);

        Assert.Equal("plugin_1", identity.PluginOption);
        Assert.Equal("navigation", identity.SkillName);
        Assert.Throws<WindowsAgentCapabilityAuthorizationException>(() =>
            WindowsAgentCapabilityAuthorization.ParseSkillRef("navigation", options));
        Assert.Throws<WindowsAgentCapabilityAuthorizationException>(() =>
            WindowsAgentCapabilityAuthorization.ParseSkillRef(
                "plugin-skill:plugin_2:navigation", options));

        var gates = new[]
        {
            new WindowsAgentCapabilitySkillGate("tool_1", ["navigation"]),
            new WindowsAgentCapabilitySkillGate("tool_2", ["editing"]),
        };
        WindowsAgentCapabilityAuthorization.RequireDisclosedSkill(
            gates,
            new HashSet<string>(StringComparer.Ordinal) { "tool_1" },
            "navigation");
        Assert.Throws<WindowsAgentCapabilityAuthorizationException>(() =>
            WindowsAgentCapabilityAuthorization.RequireDisclosedSkill(
                gates,
                new HashSet<string>(StringComparer.Ordinal) { "tool_1" },
                "editing"));

        var invalidRef = Assert.Throws<WindowsAgentCapabilityAuthorizationException>(() =>
            WindowsAgentCapabilityAuthorization.ParseSkillRef(
                "plugin-skill:plugin_2:navigation", options));
        Assert.Equal(invalidRef.Message,
            WindowsLocalAgentPlatformToolWorker.SafeErrorSummary(invalidRef));
    }
}
