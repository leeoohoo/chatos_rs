using System.Text.Json;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.LocalAgent;

internal sealed partial class WindowsLocalAgentPluginToolExecutor
{
    private sealed record PluginOption(
        string Token,
        LocalConnectorPlugin Catalog,
        InstalledPluginRecord Record);

    private sealed record PluginTool(
        string Token,
        string Name,
        string Description,
        JsonElement InputSchema,
        PluginToolPolicy Policy,
        PluginSkillGate? SkillGate,
        string AdapterSessionId,
        PluginRuntimeIdentity Identity);

    private sealed class LoadedPlugin(
        IReadOnlyList<PluginTool> tools,
        IReadOnlyList<string> adapterSessionIds)
    {
        public IReadOnlyList<PluginTool> Tools { get; } = tools;
        public IReadOnlyList<string> AdapterSessionIds { get; } = adapterSessionIds;
        public Dictionary<string, JsonElement> SkillSnapshots { get; } =
            new(StringComparer.Ordinal);
        public HashSet<string> ActivatedSkills { get; } = new(StringComparer.Ordinal);
    }

    private sealed class RunSession(
        string ownerUserId,
        string runId,
        string conversationId,
        string projectId,
        string projectRoot,
        string workspaceId,
        string deviceId,
        IReadOnlyList<PluginOption> options)
    {
        public string OwnerUserId { get; } = ownerUserId;
        public string RunId { get; } = runId;
        public string ConversationId { get; } = conversationId;
        public string ProjectId { get; } = projectId;
        public string ProjectRoot { get; } = projectRoot;
        public string WorkspaceId { get; } = workspaceId;
        public string DeviceId { get; } = deviceId;
        public IReadOnlyList<PluginOption> Options { get; } = options;
        public Dictionary<string, LoadedPlugin> Loaded { get; } = new(StringComparer.Ordinal);
        public CancellationTokenSource Expiration { get; set; } = new();
        public int DisposeStarted;
    }
}
