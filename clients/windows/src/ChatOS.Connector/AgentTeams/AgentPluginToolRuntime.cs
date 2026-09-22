using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed class AgentPluginToolRuntime(
    IInstalledPluginStore installedPlugins,
    ILocalPluginManagementService pluginManagement,
    PluginManifestLoader manifestLoader,
    IPluginMcpClientFactory clientFactory,
    PluginRuntimeSessionStore sessions,
    PluginArtifactRegistry artifacts,
    ConnectorRuntimeContext runtime,
    IProjectRegistry projects,
    CommandApprovalCoordinator approvals)
{
    private const int MaximumTools = 128;

    internal sealed record TodoPluginOption(
        string PluginId,
        string DisplayName,
        string Description);

    public async Task<IReadOnlyList<TodoPluginOption>> ListSelectableTodoPluginsAsync(
        AgentProfile profile,
        AgentRoomMember member,
        CancellationToken cancellationToken)
    {
        var allowed = AllowedPluginIds(profile, member).ToHashSet(StringComparer.Ordinal);
        if (allowed.Count == 0) return [];
        return (await pluginManagement.ListAsync(cancellationToken).ConfigureAwait(false))
            .Where(value => value.Installed && value.Enabled && allowed.Contains(value.PluginId))
            .Select(value => new TodoPluginOption(
                value.PluginId, value.DisplayName, value.Description))
            .Take(20)
            .ToArray();
    }

    public async Task<AgentPluginRunSession?> PrepareAsync(
        AgentProfile profile,
        AgentRoomMember member,
        AgentRoom room,
        string runId,
        CancellationToken cancellationToken,
        IReadOnlyList<string>? selectedPluginIds = null)
    {
        var pluginIds = AllowedPluginIds(profile, member, selectedPluginIds);
        if (pluginIds.Count == 0) return null;

        await runtime.InitializeAsync(cancellationToken).ConfigureAwait(false);
        var connector = runtime.Snapshot.State
            ?? throw new PluginRuntimeException("Local Connector is not paired for Agent Plugin execution.");
        if (!string.Equals(connector.User.Id, profile.OwnerUserId, StringComparison.Ordinal))
            throw new PluginRuntimeException("Agent Plugin execution owner does not match the paired Connector.");

        var project = await ResolveProjectAsync(
            profile.OwnerUserId, room, cancellationToken).ConfigureAwait(false);
        var enabled = (await pluginManagement.ListAsync(cancellationToken).ConfigureAwait(false))
            .Where(value => value.Enabled && value.Installed)
            .ToDictionary(value => value.PluginId, StringComparer.Ordinal);
        var bindings = new List<PluginBinding>();
        try
        {
            for (var pluginIndex = 0; pluginIndex < pluginIds.Count; pluginIndex++)
            {
                if (bindings.Sum(value => value.Tools.Count) >= MaximumTools) break;
                var pluginId = pluginIds[pluginIndex];
                if (!enabled.TryGetValue(pluginId, out var catalog))
                    throw new PluginRuntimeException(
                        $"Agent Plugin '{pluginId}' is not installed or is disabled.");
                var record = await installedPlugins.GetAsync(pluginId, cancellationToken)
                    .ConfigureAwait(false)
                    ?? throw new PluginRuntimeException($"Agent Plugin '{pluginId}' is not installed.");
                var components = await manifestLoader.ListMcpComponentsAsync(record, cancellationToken)
                    .ConfigureAwait(false);
                for (var componentIndex = 0; componentIndex < components.Count; componentIndex++)
                {
                    if (bindings.Sum(value => value.Tools.Count) >= MaximumTools) break;
                    var binding = await PrepareComponentAsync(profile.OwnerUserId, runId,
                        connector.DeviceId, project, record, catalog.DisplayName,
                        components[componentIndex], pluginIndex, componentIndex,
                        MaximumTools - bindings.Sum(value => value.Tools.Count),
                        cancellationToken).ConfigureAwait(false);
                    bindings.Add(binding);
                }
            }

            return bindings.Count == 0
                ? null
                : new AgentPluginRunSession(sessions, artifacts, profile.OwnerUserId,
                    connector.DeviceId, bindings, approvals);
        }
        catch
        {
            await StopBindingsAsync(bindings).ConfigureAwait(false);
            throw;
        }
    }

    private async Task<PluginBinding> PrepareComponentAsync(
        string ownerUserId,
        string runId,
        string deviceId,
        ProjectScope project,
        InstalledPluginRecord record,
        string displayName,
        string componentKey,
        int pluginIndex,
        int componentIndex,
        int maximumTools,
        CancellationToken cancellationToken)
    {
        var adapterSessionId = Guid.NewGuid().ToString("D").ToLowerInvariant();
        var launch = await manifestLoader.PrepareAsync(record, componentKey, null,
            adapterSessionId, project.Root, record.DeclaredPermissions.ToHashSet(StringComparer.Ordinal),
            ownerUserId, deviceId, project.WorkspaceId, project.ProjectId, project.ProjectName,
            cancellationToken).ConfigureAwait(false);
        var client = clientFactory.Create(launch);
        var registered = false;
        try
        {
            await client.StartAsync(cancellationToken).ConfigureAwait(false);
            var initialized = await client.InitializeAsync(cancellationToken).ConfigureAwait(false);
            var identity = new PluginRuntimeIdentity(runId, record.PluginId, record.ReleaseId,
                record.Version, record.ArtifactSha256, launch.ComponentKey, adapterSessionId,
                project.WorkspaceId, project.ProjectId);
            var published = ValidTools(initialized.Tools);
            if (published.Count == 0)
                throw new PluginRuntimeException("Agent Plugin MCP did not publish valid tools.");
            await sessions.InsertAsync(identity, client, published.Select(value => value.Raw).ToArray(),
                record.DeclaredPermissions.ToHashSet(StringComparer.Ordinal),
                launch.Server.RequiresExclusiveExecution, launch.InstallationPath,
                launch.ArtifactPath, launch.VisualSessionPath, launch.DisplayName).ConfigureAwait(false);
            registered = true;

            var tools = published.Take(maximumTools).Select((tool, toolIndex) =>
            {
                var exposedName = ExposedName(pluginIndex, componentIndex, toolIndex, tool.Name);
                var description = $"Plugin {displayName} / {componentKey} / {tool.Name}: {tool.Description}";
                if (description.Length > 1_000) description = description[..1_000];
                return new PluginTool(exposedName, tool.Name,
                    new AgentToolDefinition(exposedName, description, tool.InputSchema),
                    PluginToolPolicy.Parse(tool.Raw));
            }).ToArray();
            return new PluginBinding(identity, adapterSessionId, launch.ArtifactPath,
                project.WorkspaceId, project.ProjectId, initialized.Instructions, tools);
        }
        catch
        {
            if (registered)
            {
                await sessions.CancelAsync(adapterSessionId, null,
                    project.WorkspaceId, project.ProjectId).ConfigureAwait(false);
            }
            else
            {
                await client.TerminateAsync().ConfigureAwait(false);
                await client.DisposeAsync().ConfigureAwait(false);
            }
            throw;
        }
    }

    private async Task<ProjectScope> ResolveProjectAsync(
        string ownerUserId,
        AgentRoom room,
        CancellationToken cancellationToken)
    {
        if (room.Kind != AgentConversationKind.ProjectTeam)
            return ProjectScope.Empty;
        var record = await projects.GetAsync(ownerUserId, room.ProjectId, cancellationToken)
            .ConfigureAwait(false);
        if (record is null || record.Status != LocalProjectStatus.Active)
            throw new PluginRuntimeException("Agent team project is unavailable for Plugin execution.");
        var workspace = runtime.Find(record.Draft.WorkspaceId)
            ?? throw new PluginRuntimeException("Agent team workspace is not paired on this device.");
        var relativeRoot = string.IsNullOrEmpty(record.Draft.RelativeRoot)
            ? "."
            : record.Draft.RelativeRoot;
        var root = new WorkspacePathGuard(workspace.AbsoluteRoot).ResolveExisting(relativeRoot);
        if (!Directory.Exists(root))
            throw new PluginRuntimeException("Agent team project directory is unavailable.");
        return new ProjectScope(root, workspace.Id, record.Id, record.Draft.Name);
    }

    internal static IReadOnlyList<string> AllowedPluginIds(
        AgentProfile profile,
        AgentRoomMember member,
        IReadOnlyList<string>? selectedPluginIds = null)
    {
        var allowlist = member.Draft.Plugins.ToHashSet(StringComparer.Ordinal);
        var allowed = profile.Draft.Plugins
            .Where(value => allowlist.Count == 0 || allowlist.Contains(value))
            .Distinct(StringComparer.Ordinal)
            .Take(20)
            .ToArray();
        if (selectedPluginIds is null) return allowed;
        var allowedSet = allowed.ToHashSet(StringComparer.Ordinal);
        if (selectedPluginIds.Any(value => !allowedSet.Contains(value)))
            throw new PluginRuntimeException(
                "Todo Plugin snapshot contains a Plugin no longer allowed for this Agent.");
        return selectedPluginIds.Distinct(StringComparer.Ordinal).Take(20).ToArray();
    }

    private static IReadOnlyList<PublishedTool> ValidTools(IReadOnlyList<JsonElement> tools)
    {
        var output = new List<PublishedTool>();
        var names = new HashSet<string>(StringComparer.Ordinal);
        foreach (var tool in tools)
        {
            if (tool.ValueKind != JsonValueKind.Object ||
                !tool.TryGetProperty("name", out var nameValue) ||
                nameValue.ValueKind != JsonValueKind.String ||
                string.IsNullOrWhiteSpace(nameValue.GetString()) ||
                !names.Add(nameValue.GetString()!) ||
                !tool.TryGetProperty("inputSchema", out var schema) ||
                schema.ValueKind != JsonValueKind.Object)
            {
                continue;
            }
            var description = tool.TryGetProperty("description", out var descriptionValue) &&
                descriptionValue.ValueKind == JsonValueKind.String
                    ? descriptionValue.GetString() ?? string.Empty
                    : string.Empty;
            output.Add(new PublishedTool(nameValue.GetString()!, description,
                schema.Clone(), tool.Clone()));
        }
        return output;
    }

    private static string ExposedName(
        int pluginIndex,
        int componentIndex,
        int toolIndex,
        string original)
    {
        var normalized = new string(original.Select(value =>
            char.IsAsciiLetterOrDigit(value) || value is '_' or '-' ? value : '_').ToArray());
        if (string.IsNullOrWhiteSpace(normalized)) normalized = "tool";
        var prefix = $"mcp_{pluginIndex}_{componentIndex}_{toolIndex}_";
        return prefix + normalized[..Math.Min(normalized.Length, 64 - prefix.Length)];
    }

    private async Task StopBindingsAsync(IReadOnlyList<PluginBinding> bindings)
    {
        foreach (var binding in bindings.Reverse())
        {
            try
            {
                await sessions.CancelAsync(binding.AdapterSessionId, null,
                    binding.WorkspaceId, binding.ProjectId).ConfigureAwait(false);
            }
            catch
            {
            }
        }
    }

    private sealed record PublishedTool(
        string Name,
        string Description,
        JsonElement InputSchema,
        JsonElement Raw);

    internal sealed record PluginTool(
        string ExposedName,
        string OriginalName,
        AgentToolDefinition Definition,
        PluginToolPolicy Policy);

    internal sealed record PluginBinding(
        PluginRuntimeIdentity Identity,
        string AdapterSessionId,
        string ArtifactDirectory,
        string? WorkspaceId,
        string? ProjectId,
        string? Instructions,
        IReadOnlyList<PluginTool> Tools);

    private sealed record ProjectScope(
        string? Root,
        string? WorkspaceId,
        string? ProjectId,
        string? ProjectName)
    {
        public static ProjectScope Empty { get; } = new(null, null, null, null);
    }
}

internal sealed class AgentPluginRunSession(
    PluginRuntimeSessionStore sessions,
    PluginArtifactRegistry artifacts,
    string ownerUserId,
    string deviceId,
    IReadOnlyList<AgentPluginToolRuntime.PluginBinding> bindings,
    CommandApprovalCoordinator? approvals = null) : IAsyncDisposable
{
    private readonly Dictionary<string, (AgentPluginToolRuntime.PluginBinding Binding,
        AgentPluginToolRuntime.PluginTool Tool)> _tools = bindings
        .SelectMany(binding => binding.Tools.Select(tool => (binding, tool)))
        .ToDictionary(value => value.tool.ExposedName,
            value => (value.binding, value.tool), StringComparer.Ordinal);
    private int _disposed;

    public IReadOnlyList<AgentToolDefinition> Definitions =>
        _tools.Values.Select(value => value.Tool.Definition).ToArray();

    public string Instructions => string.Join("\n\n", bindings
        .Where(value => !string.IsNullOrWhiteSpace(value.Instructions))
        .Select(value => $"Plugin {value.Identity.PluginId}/{value.Identity.ComponentKey}:\n{value.Instructions}"));

    public bool CanExecute(string name) => _tools.ContainsKey(name);

    public async Task<string> ExecuteAsync(
        AgentToolCall call,
        CancellationToken cancellationToken)
    {
        if (!_tools.TryGetValue(call.Name, out var target))
            throw new AgentTeamException(AgentTeamError.InvalidField,
                $"Unknown Agent Plugin tool: {call.Name}");
        JsonElement arguments;
        try
        {
            using var document = JsonDocument.Parse(call.Arguments);
            if (document.RootElement.ValueKind != JsonValueKind.Object)
                throw new JsonException();
            arguments = document.RootElement.Clone();
        }
        catch (JsonException exception)
        {
            throw new AgentTeamException(AgentTeamError.InvalidField,
                "Agent Plugin tool arguments are not a JSON object.", exception);
        }

        var granted = sessions.Permissions(target.Binding.AdapterSessionId);
        var required = target.Tool.Policy.RequiredPermissions(arguments);
        if (required.Any(permission => !granted.Contains(permission)))
            throw new PluginRuntimeException(
                "Agent Plugin tool requested a local permission that was not granted.");
        if (target.Tool.Policy.ApprovalMode == "per_call")
        {
            if (approvals is null)
                throw new PluginRuntimeException("Agent Plugin approval service is unavailable.");
            var summary = SafeArgumentSummary(target.Tool.OriginalName, arguments);
            var outcome = await approvals.RequestAsync(new CommandApprovalRequest(
                call.Id, ownerUserId, deviceId, target.Binding.WorkspaceId ?? "device",
                $"Plugin · {target.Tool.OriginalName}", [summary],
                sessions.WorkingDirectory(target.Binding.AdapterSessionId),
                target.Binding.Identity.ComponentKey.Contains("browser",
                    StringComparison.OrdinalIgnoreCase)
                    ? "plugin_browser_agent_team"
                    : "plugin_agent_team",
                $"agent-plugin:{target.Binding.AdapterSessionId}"),
                new ConnectorApprovalRisk(target.Tool.Policy.RiskLevel,
                    $"Agent Plugin requested a local operation: {summary}"),
                cancellationToken).ConfigureAwait(false);
            if (!outcome.Approved)
                throw new PluginRuntimeException("User did not approve this Agent Plugin operation.");
        }

        var result = await sessions.CallAsync(target.Binding.AdapterSessionId,
            call.Id, target.Tool.OriginalName, arguments, target.Tool.Policy.Timeout,
            cancellationToken).ConfigureAwait(false);
        result = await artifacts.RegisterAsync(target.Binding.Identity, ownerUserId, deviceId,
            target.Binding.ArtifactDirectory, target.Tool.OriginalName, result,
            cancellationToken).ConfigureAwait(false);
        return result.GetRawText();
    }

    private static string SafeArgumentSummary(string toolName, JsonElement arguments)
    {
        var keys = string.Join(", ", arguments.EnumerateObject()
            .Select(property => property.Name).Order(StringComparer.Ordinal));
        var canonical = CanonicalJson.Serialize(arguments);
        var digest = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(canonical)))
            .ToLowerInvariant()[..12];
        return $"{toolName}; fields: {keys}; digest: {digest}";
    }

    public async ValueTask DisposeAsync()
    {
        if (Interlocked.Exchange(ref _disposed, 1) != 0) return;
        foreach (var binding in bindings.Reverse())
        {
            try
            {
                await sessions.CancelAsync(binding.AdapterSessionId, null,
                    binding.WorkspaceId, binding.ProjectId).ConfigureAwait(false);
            }
            catch
            {
            }
        }
    }
}
