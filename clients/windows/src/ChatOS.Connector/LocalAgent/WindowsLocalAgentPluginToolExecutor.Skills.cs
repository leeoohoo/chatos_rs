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
        var option = Option(session, arguments);
        var plugin = await LoadAsync(session, option, cancellationToken).ConfigureAwait(false);
        var skillName = WindowsLocalAgentProjectToolExecutor.RequiredString(
            arguments, "skill_name");
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
        var option = Option(session, arguments);
        var plugin = await LoadAsync(session, option, cancellationToken).ConfigureAwait(false);
        var skillName = WindowsLocalAgentProjectToolExecutor.RequiredString(
            arguments, "skill_name");
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
        var available = plugin.Tools
            .SelectMany(tool =>
                tool.SkillGate?.CatalogSkillNames ?? Array.Empty<string>())
            .Contains(skillName, StringComparer.Ordinal);
        if (!available)
        {
            throw new InvalidOperationException(
                "The Plugin Skill is not required by the described tools.");
        }
    }

    private static int? Integer(JsonElement value, string name) =>
        value.TryGetProperty(name, out var property) && property.TryGetInt32(out var result)
            ? result
            : null;
}
