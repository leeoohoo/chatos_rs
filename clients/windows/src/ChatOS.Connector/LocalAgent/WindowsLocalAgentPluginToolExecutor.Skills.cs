using System.Diagnostics;
using System.Text.Json;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.LocalAgent;

internal sealed partial class WindowsLocalAgentPluginToolExecutor
{
    private async Task<JsonElement> ActivateSkillAsync(
        RunSession session,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var (option, skillName) = SkillIdentity(session, arguments);
        var plugin = await LoadAsync(session, option, cancellationToken).ConfigureAwait(false);
        EnsureSkillAvailable(plugin, skillName);
        var snapshot = PluginSkillSnapshotLoader.CreateLocalSnapshot(
            option.Record, skillName);
        var result = PluginSkillSnapshotLoader.Activate(
            option.Record, skillName, snapshot);
        plugin.SkillSnapshots[skillName] = snapshot;
        plugin.ActivatedSkills.Add(skillName);
        return result;
    }

    private async Task<JsonElement> ReadSkillResourceAsync(
        RunSession session,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var (option, skillName) = SkillIdentity(session, arguments);
        var plugin = await LoadAsync(session, option, cancellationToken).ConfigureAwait(false);
        if (!plugin.ActivatedSkills.Contains(skillName) ||
            !plugin.SkillSnapshots.TryGetValue(skillName, out var snapshot))
        {
            throw new InvalidOperationException(
                "The Plugin Skill must be activated before reading its resources.");
        }
        var relativePath = WindowsLocalAgentProjectToolExecutor.RequiredString(
            arguments, "relative_path");
        var offset = Integer(arguments, "offset") ?? 0;
        var limit = Integer(arguments, "limit") ?? 32_000;
        return PluginSkillSnapshotLoader.ReadResource(
            option.Record, skillName, snapshot, relativePath, offset, limit);
    }

    private static void EnsureSkillAvailable(LoadedPlugin plugin, string skillName)
    {
        WindowsAgentCapabilityAuthorization.RequireDisclosedSkill(
            plugin.Tools.Select(tool => new WindowsAgentCapabilitySkillGate(
                tool.Token,
                tool.SkillGate?.CatalogSkillNames ?? Array.Empty<string>())),
            plugin.DescribedTools,
            skillName);
    }

    private static (PluginOption Option, string SkillName) SkillIdentity(
        RunSession session,
        JsonElement arguments)
    {
        var skillRef = WindowsLocalAgentProjectToolExecutor.RequiredString(
            arguments, "skill_ref");
        var (optionToken, skillName) = WindowsAgentCapabilityAuthorization.ParseSkillRef(
            skillRef,
            session.Options.Select(value => value.Token).ToHashSet(StringComparer.Ordinal));
        var option = session.Options.FirstOrDefault(value => value.Token == optionToken)
            ?? throw new UnreachableException();
        return (option, skillName);
    }

    private static int? Integer(JsonElement value, string name)
    {
        if (!value.TryGetProperty(name, out var property) ||
            property.ValueKind == JsonValueKind.Null) return null;
        if (!property.TryGetInt32(out var result))
        {
            throw new WindowsAgentCapabilityDisclosureException();
        }
        return result;
    }
}
