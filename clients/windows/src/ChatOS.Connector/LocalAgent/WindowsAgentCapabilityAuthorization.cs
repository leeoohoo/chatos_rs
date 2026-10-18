namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsAgentCapabilitySkillGate(
    string ToolOption,
    IReadOnlyList<string> SkillNames);

internal static class WindowsAgentCapabilityAuthorization
{
    public static void RequireDescribedTool(
        IReadOnlySet<string> describedTools,
        string toolOption)
    {
        if (!describedTools.Contains(toolOption))
        {
            throw new WindowsAgentCapabilityAuthorizationException(
                "The Plugin tool must be described for this Run before invocation.");
        }
    }

    public static (string PluginOption, string SkillName) ParseSkillRef(
        string skillRef,
        IReadOnlySet<string> pluginOptions)
    {
        const string prefix = "plugin-skill:";
        if (!skillRef.StartsWith(prefix, StringComparison.Ordinal))
        {
            throw InvalidSkillRef();
        }
        var payload = skillRef[prefix.Length..];
        var separator = payload.IndexOf(':');
        if (separator <= 0 || separator == payload.Length - 1)
        {
            throw InvalidSkillRef();
        }
        var pluginOption = payload[..separator];
        var skillName = payload[(separator + 1)..];
        if (!pluginOptions.Contains(pluginOption)) throw InvalidSkillRef();
        return (pluginOption, skillName);
    }

    public static void RequireDisclosedSkill(
        IEnumerable<WindowsAgentCapabilitySkillGate> tools,
        IReadOnlySet<string> describedTools,
        string skillName)
    {
        var available = tools
            .Where(tool => describedTools.Contains(tool.ToolOption))
            .SelectMany(tool => tool.SkillNames)
            .Contains(skillName, StringComparer.Ordinal);
        if (!available)
        {
            throw new WindowsAgentCapabilityAuthorizationException(
                "The Plugin Skill was not disclosed by a described tool in this Run.");
        }
    }

    private static WindowsAgentCapabilityAuthorizationException InvalidSkillRef() =>
        new("The Plugin Skill reference is invalid or expired.");
}

internal sealed class WindowsAgentCapabilityAuthorizationException(string message)
    : InvalidOperationException(message);
