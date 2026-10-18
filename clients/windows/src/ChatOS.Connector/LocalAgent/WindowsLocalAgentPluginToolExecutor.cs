using System.Text.Json;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.LocalAgent;

public interface IWindowsLocalAgentPluginToolExecutor
{
    Task<IReadOnlyList<WindowsLocalAgentPluginChoice>> ListChoicesAsync(
        CancellationToken cancellationToken);

    Task<JsonElement> ExecuteAsync(
        string ownerUserId,
        string runId,
        string callId,
        string toolName,
        JsonElement arguments,
        CancellationToken cancellationToken);

    void Reset();
}

internal sealed partial class WindowsLocalAgentPluginToolExecutor(
    WindowsLocalAgentProjectToolExecutor projectTools,
    IInstalledPluginStore installedPlugins,
    ILocalPluginManagementService pluginManagement,
    PluginManifestLoader manifestLoader,
    IPluginMcpClientFactory clientFactory,
    PluginRuntimeSessionStore runtimeSessions,
    PluginArtifactRegistry artifacts,
    IConnectorWorkspaceContext workspaces,
    CommandApprovalCoordinator approvals) : IWindowsLocalAgentPluginToolExecutor
{
    private const int MaximumPlugins = 20;
    private const int MaximumTools = 128;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private readonly object _gate = new();
    private readonly Dictionary<string, RunSession> _runs = new(StringComparer.Ordinal);

    public async Task<IReadOnlyList<WindowsLocalAgentPluginChoice>> ListChoicesAsync(
        CancellationToken cancellationToken)
    {
        var installed = (await installedPlugins.ListAsync(cancellationToken).ConfigureAwait(false))
            .ToDictionary(record => record.PluginId, StringComparer.Ordinal);
        var plugins = (await pluginManagement.ListAsync(cancellationToken).ConfigureAwait(false))
            .Where(plugin => plugin.Installed && plugin.Enabled)
            .OrderBy(plugin => plugin.DisplayName, StringComparer.OrdinalIgnoreCase)
            .ThenBy(plugin => plugin.PluginId, StringComparer.Ordinal)
            .ToArray();
        var choices = new List<WindowsLocalAgentPluginChoice>();
        foreach (var plugin in plugins)
        {
            if (!installed.TryGetValue(plugin.PluginId, out var record)) continue;
            var installedKey = string.IsNullOrWhiteSpace(record.PluginKey)
                ? record.PluginId
                : record.PluginKey.Trim();
            if (!string.Equals(installedKey, plugin.PluginKey, StringComparison.Ordinal))
            {
                throw new InvalidOperationException(
                    "Installed Plugin identity does not match the enabled catalog.");
            }
            var components = await manifestLoader.ListMcpComponentChoicesAsync(
                record, cancellationToken).ConfigureAwait(false);
            choices.AddRange(components.Select(component => new WindowsLocalAgentPluginChoice(
                plugin.PluginKey,
                component.ComponentKey,
                record.PluginId,
                record.ReleaseId,
                record.Version,
                record.ArtifactSha256,
                component.DisplayName ?? plugin.DisplayName,
                component.Description ?? plugin.Description)));
        }
        return choices
            .OrderBy(choice => choice.DisplayName, StringComparer.OrdinalIgnoreCase)
            .ThenBy(choice => choice.PluginKey, StringComparer.Ordinal)
            .ThenBy(choice => choice.ComponentKey, StringComparer.Ordinal)
            .ToArray();
    }

    public async Task<JsonElement> ExecuteAsync(
        string ownerUserId,
        string runId,
        string callId,
        string toolName,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        if (!WindowsLocalAgentCapabilityCatalog.PluginToolNames.Contains(toolName) ||
            arguments.ValueKind != JsonValueKind.Object)
        {
            throw new InvalidOperationException("Unsupported Local Agent Plugin tool.");
        }
        var context = await projectTools.ResolveContextAsync(
            ownerUserId, runId, cancellationToken).ConfigureAwait(false);
        if (!context.Authorization.Allows(toolName))
        {
            throw new InvalidOperationException(
                "The Local Agent Task Plugin capability was not selected.");
        }
        var session = await SessionAsync(
            ownerUserId, runId, context, cancellationToken).ConfigureAwait(false);
        RefreshExpiration(session);
        return toolName switch
        {
            "capability_search" => Search(session, arguments),
            "capability_describe" => await DescribeAsync(
                session, arguments, cancellationToken).ConfigureAwait(false),
            "agent_skill_activate" => await ActivateSkillAsync(
                session, arguments, cancellationToken).ConfigureAwait(false),
            "agent_skill_read_resource" => await ReadSkillResourceAsync(
                session, arguments, cancellationToken).ConfigureAwait(false),
            "capability_invoke" => await InvokeAsync(
                session, callId, arguments, cancellationToken).ConfigureAwait(false),
            _ => throw new InvalidOperationException("Unsupported Local Agent Plugin tool."),
        };
    }

    public void Reset()
    {
        RunSession[] sessions;
        lock (_gate)
        {
            sessions = _runs.Values.ToArray();
            _runs.Clear();
        }
        foreach (var session in sessions)
        {
            _ = DisposeSessionAsync(session);
        }
    }

    private async Task<RunSession> SessionAsync(
        string ownerUserId,
        string runId,
        WindowsLocalAgentProjectContext context,
        CancellationToken cancellationToken)
    {
        lock (_gate)
        {
            if (_runs.TryGetValue(runId, out var existing))
            {
                Validate(existing, ownerUserId, context);
                return existing;
            }
        }

        var enabled = (await pluginManagement.ListAsync(cancellationToken).ConfigureAwait(false))
            .Where(value => value.Installed && value.Enabled)
            .ToDictionary(value => value.PluginId, StringComparer.Ordinal);
        var records = (await installedPlugins.ListAsync(cancellationToken).ConfigureAwait(false))
            .ToDictionary(value => value.PluginId, StringComparer.Ordinal);
        var selected = context.Authorization.IsLegacyUnrestricted
            ? (await ListChoicesAsync(cancellationToken).ConfigureAwait(false))
                .Take(MaximumPlugins)
                .ToArray()
            : context.Authorization.PluginReleaseSnapshots.Values
                .OrderBy(value => value.PluginKey, StringComparer.Ordinal)
                .ThenBy(value => value.ComponentKey, StringComparer.Ordinal)
                .Take(MaximumPlugins)
                .Select(snapshot => ValidateFrozenChoice(snapshot, enabled, records))
                .ToArray();
        var options = selected.Select((choice, index) =>
        {
            if (!records.TryGetValue(choice.PluginId, out var record))
            {
                throw new InvalidOperationException(
                    "The Local Agent Task Plugin component is no longer installed.");
            }
            return new PluginOption($"plugin_{index + 1}", choice, record);
        }).ToArray();
        var deviceId = workspaces.DeviceId
            ?? throw new InvalidOperationException("The local Connector is not paired.");
        var created = new RunSession(
            ownerUserId,
            runId,
            context.ConversationId,
            context.Project.Id,
            context.ProjectRoot,
            context.Workspace.Id,
            deviceId,
            options);
        lock (_gate)
        {
            if (_runs.TryGetValue(runId, out var existing))
            {
                created.Expiration.Dispose();
                Validate(existing, ownerUserId, context);
                return existing;
            }
            _runs.Add(runId, created);
        }
        return created;
    }

    private static void Validate(
        RunSession session,
        string ownerUserId,
        WindowsLocalAgentProjectContext context)
    {
        if (session.OwnerUserId != ownerUserId ||
            session.ConversationId != context.ConversationId ||
            session.ProjectId != context.Project.Id ||
            session.ProjectRoot != context.ProjectRoot)
        {
            throw new InvalidOperationException("Invalid Local Agent Plugin Run context.");
        }
    }

    private static JsonElement Search(RunSession session, JsonElement arguments)
    {
        var query = WindowsLocalAgentProjectToolExecutor.RequiredString(arguments, "query");
        if (query.Length > 200)
        {
            throw new InvalidOperationException("The Plugin capability query is invalid.");
        }
        var terms = query.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries |
            StringSplitOptions.TrimEntries);
        var lexicalMatches = session.Options.Where(option =>
        {
            var searchable = $"{option.Choice.DisplayName} {option.Choice.Description} " +
                $"{option.Choice.PluginKey} {option.Choice.ComponentKey}";
            return terms.Any(term => searchable.Contains(term, StringComparison.OrdinalIgnoreCase));
        }).ToArray();
        // The Task already froze this run's trusted Plugin selections. A translated
        // query may have no literal overlap with an English manifest, so keep the
        // selected options discoverable instead of returning a misleading empty list.
        var visibleOptions = lexicalMatches.Length == 0 ? session.Options : lexicalMatches;
        var matches = visibleOptions.Take(12).Select(option => new
        {
            plugin_option = option.Token,
            name = option.Choice.DisplayName,
                description = WindowsLocalAgentPluginRuntimeSupport.Limit(
                    option.Choice.Description, 1_000),
        });
        return JsonSerializer.SerializeToElement(new { matches }, JsonOptions);
    }

    private async Task<JsonElement> DescribeAsync(
        RunSession session,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var option = Option(session, arguments);
        var plugin = await LoadAsync(session, option, cancellationToken).ConfigureAwait(false);
        var toolOption = OptionalString(arguments, "tool_option");
        var response = WindowsAgentCapabilityDisclosure.Response(
            new WindowsAgentCapabilityDescriptor(
                option.Token,
                option.Choice.DisplayName,
                option.Choice.PluginKey,
                option.Choice.ComponentKey,
                plugin.Tools.Select(tool => new WindowsAgentCapabilityToolDescriptor(
                    tool.Token,
                    tool.Name,
                    tool.Description,
                    tool.InputSchema,
                    tool.Policy.ApprovalMode,
                    (tool.SkillGate?.CatalogSkillNames ?? Array.Empty<string>())
                        .Select(name => SkillRef(option.Token, name))
                        .ToArray())).ToArray()),
            toolOption,
            OptionalString(arguments, "query"),
            Integer(arguments, "offset") ?? 0,
            Integer(arguments, "limit") ?? 8,
            Integer(arguments, "schema_offset"));
        if (toolOption is not null &&
            response.TryGetProperty("view", out var view) &&
            view.GetString() == "tool")
        {
            plugin.DescribedTools.Add(toolOption);
        }
        return response;
    }

    private async Task<JsonElement> InvokeAsync(
        RunSession session,
        string callId,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var option = Option(session, arguments);
        var plugin = await LoadAsync(session, option, cancellationToken).ConfigureAwait(false);
        var toolOption = WindowsLocalAgentProjectToolExecutor.RequiredString(
            arguments, "tool_option");
        var tool = plugin.Tools.FirstOrDefault(value => value.Token == toolOption)
            ?? throw new InvalidOperationException("The Plugin tool option is invalid or expired.");
        WindowsAgentCapabilityAuthorization.RequireDescribedTool(
            plugin.DescribedTools, toolOption);
        if (!arguments.TryGetProperty("arguments", out var toolArguments) ||
            toolArguments.ValueKind != JsonValueKind.Object)
        {
            throw new InvalidOperationException("The Plugin tool arguments are invalid.");
        }

        // Validate the frozen schema before Skill, permission, approval, or process work.
        // Unsupported third-party assertions fail closed instead of being ignored.
        WindowsExternalToolSchemaValidator.Validate(toolArguments, tool.InputSchema);

        if (tool.SkillGate is { } skillGate)
        {
            var missing = skillGate.RequiredSkillNames(toolArguments)
                .Where(value => !plugin.ActivatedSkills.Contains(value))
                .ToArray();
            if (missing.Length > 0)
            {
                throw new InvalidOperationException(
                    $"The Plugin tool requires activated Skills: {string.Join(", ", missing)}.");
            }
        }

        var granted = runtimeSessions.Permissions(tool.AdapterSessionId);
        var required = tool.Policy.RequiredPermissions(toolArguments);
        if (required.Any(permission => !granted.Contains(permission)))
        {
            throw new InvalidOperationException(
                "The Plugin tool requested a local permission that was not granted.");
        }
        if (tool.Policy.ApprovalMode == "per_call")
        {
            var summary = WindowsLocalAgentPluginRuntimeSupport.SafeArgumentSummary(
                tool.Name, toolArguments);
            var outcome = await approvals.RequestAsync(new CommandApprovalRequest(
                callId,
                session.OwnerUserId,
                session.DeviceId,
                session.WorkspaceId,
                $"Plugin · {tool.Name}",
                [summary],
                runtimeSessions.WorkingDirectory(tool.AdapterSessionId),
                "local_agent_task_execution",
                $"local-agent-plugin:{tool.AdapterSessionId}"),
                new ConnectorApprovalRisk(
                    tool.Policy.RiskLevel,
                    $"A local task requested a Plugin operation: {summary}"),
                cancellationToken).ConfigureAwait(false);
            if (!outcome.Approved)
            {
                throw new InvalidOperationException("The Plugin operation was not approved.");
            }
        }

        var invocationId = string.IsNullOrWhiteSpace(callId)
            ? Guid.NewGuid().ToString("N")
            : callId;
        var result = await runtimeSessions.CallAsync(
            tool.AdapterSessionId,
            invocationId,
            tool.Name,
            toolArguments,
            tool.Policy.Timeout,
            cancellationToken).ConfigureAwait(false);
        result = await artifacts.RegisterAsync(
            tool.Identity,
            session.OwnerUserId,
            session.DeviceId,
            session.ProjectRoot,
            runtimeSessions.Permissions(tool.AdapterSessionId),
            runtimeSessions.ArtifactDirectory(tool.AdapterSessionId),
            tool.Name,
            result,
            cancellationToken).ConfigureAwait(false);
        return JsonSerializer.SerializeToElement(new
        {
            content = result,
            is_error = false,
            made_progress = true,
        }, JsonOptions);
    }

    private async Task<LoadedPlugin> LoadAsync(
        RunSession session,
        PluginOption option,
        CancellationToken cancellationToken)
    {
        if (session.Loaded.TryGetValue(option.Token, out var existing)) return existing;

        var tools = new List<PluginTool>();
        var adapterSessions = new List<string>();
        try
        {
            var componentKey = option.Choice.ComponentKey;
            var adapterSessionId = Guid.NewGuid().ToString("D").ToLowerInvariant();
            var launch = await manifestLoader.PrepareAsync(
                option.Record,
                componentKey,
                null,
                adapterSessionId,
                session.ProjectRoot,
                option.Record.DeclaredPermissions.ToHashSet(StringComparer.Ordinal),
                session.OwnerUserId,
                session.DeviceId,
                session.WorkspaceId,
                session.ProjectId,
                option.Choice.DisplayName,
                cancellationToken).ConfigureAwait(false);
            var client = clientFactory.Create(launch);
            var registered = false;
            try
            {
                await client.StartAsync(cancellationToken).ConfigureAwait(false);
                var initialized = await client.InitializeAsync(cancellationToken)
                    .ConfigureAwait(false);
                var definitions = WindowsLocalAgentPluginRuntimeSupport.ValidTools(
                    initialized.Tools);
                var identity = new PluginRuntimeIdentity(
                    session.RunId,
                    option.Record.PluginId,
                    option.Record.ReleaseId,
                    option.Record.Version,
                    option.Record.ArtifactSha256,
                    launch.ComponentKey,
                    adapterSessionId,
                    session.WorkspaceId,
                    session.ProjectId);
                await runtimeSessions.InsertAsync(
                    identity,
                    client,
                    definitions,
                    option.Record.DeclaredPermissions.ToHashSet(StringComparer.Ordinal),
                    launch.Server.RequiresExclusiveExecution,
                    launch.InstallationPath,
                    launch.ArtifactPath,
                    launch.VisualSessionPath,
                    launch.DisplayName).ConfigureAwait(false);
                registered = true;
                adapterSessions.Add(adapterSessionId);
                foreach (var definition in definitions)
                {
                    if (tools.Count >= MaximumTools) break;
                    var name = definition.GetProperty("name").GetString()!;
                    tools.Add(new PluginTool(
                        $"tool_{tools.Count + 1}",
                        name,
                        WindowsLocalAgentPluginRuntimeSupport.Description(definition),
                        definition.GetProperty("inputSchema").Clone(),
                        PluginToolPolicy.Parse(definition),
                        WindowsLocalAgentPluginRuntimeSupport.SkillGate(definition),
                        adapterSessionId,
                        identity));
                }
            }
            catch
            {
                if (registered)
                {
                    await runtimeSessions.CancelAsync(
                        adapterSessionId, null, session.WorkspaceId, session.ProjectId)
                        .ConfigureAwait(false);
                }
                else
                {
                    await client.TerminateAsync().ConfigureAwait(false);
                    await client.DisposeAsync().ConfigureAwait(false);
                }
                throw;
            }
            if (tools.Count == 0)
            {
                throw new InvalidOperationException(
                    "The Plugin did not publish valid local tools.");
            }
            var loaded = new LoadedPlugin(tools, adapterSessions);
            session.Loaded.Add(option.Token, loaded);
            return loaded;
        }
        catch
        {
            foreach (var adapterSessionId in adapterSessions.AsEnumerable().Reverse())
            {
                await runtimeSessions.CancelAsync(
                    adapterSessionId, null, session.WorkspaceId, session.ProjectId)
                    .ConfigureAwait(false);
            }
            throw;
        }
    }

    internal static WindowsLocalAgentPluginChoice ValidateFrozenChoice(
        WindowsLocalAgentPluginReleaseSnapshot snapshot,
        IReadOnlyDictionary<string, LocalConnectorPlugin> enabled,
        IReadOnlyDictionary<string, InstalledPluginRecord> records)
    {
        if (!enabled.TryGetValue(snapshot.PluginId, out var plugin) ||
            !records.TryGetValue(snapshot.PluginId, out var record))
        {
            throw new InvalidOperationException(
                "The Local Agent Task Plugin component is no longer installed or enabled.");
        }
        var installedKey = string.IsNullOrWhiteSpace(record.PluginKey)
            ? record.PluginId
            : record.PluginKey.Trim();
        if (!string.Equals(plugin.PluginKey, snapshot.PluginKey, StringComparison.Ordinal) ||
            !string.Equals(installedKey, snapshot.PluginKey, StringComparison.Ordinal) ||
            !string.Equals(record.ReleaseId, snapshot.ReleaseId, StringComparison.Ordinal) ||
            !string.Equals(record.Version, snapshot.Version, StringComparison.Ordinal) ||
            !string.Equals(record.ArtifactSha256, snapshot.ArtifactSha256, StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "The Plugin Release frozen by this Task has changed; create a new Task.");
        }
        return new WindowsLocalAgentPluginChoice(
            snapshot.PluginKey,
            snapshot.ComponentKey,
            snapshot.PluginId,
            snapshot.ReleaseId,
            snapshot.Version,
            snapshot.ArtifactSha256,
            plugin.DisplayName,
            plugin.Description);
    }

    private static PluginOption Option(RunSession session, JsonElement arguments)
    {
        var token = WindowsLocalAgentProjectToolExecutor.RequiredString(
            arguments, "plugin_option");
        return session.Options.FirstOrDefault(value => value.Token == token)
            ?? throw new InvalidOperationException("The Plugin option is invalid or expired.");
    }

    private static string? OptionalString(JsonElement value, string name)
    {
        if (!value.TryGetProperty(name, out var property) ||
            property.ValueKind == JsonValueKind.Null) return null;
        if (property.ValueKind != JsonValueKind.String)
        {
            throw new WindowsAgentCapabilityDisclosureException();
        }
        return property.GetString();
    }

    private static string SkillRef(string pluginOption, string skillName) =>
        $"plugin-skill:{pluginOption}:{skillName}";

    private void RefreshExpiration(RunSession session)
    {
        session.Expiration.Cancel();
        session.Expiration.Dispose();
        session.Expiration = new CancellationTokenSource();
        var source = session.Expiration;
        var token = source.Token;
        _ = Task.Run(async () =>
        {
            try
            {
                await Task.Delay(TimeSpan.FromMinutes(5), token).ConfigureAwait(false);
                await ExpireAsync(session, source).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (token.IsCancellationRequested)
            {
            }
        }, CancellationToken.None);
    }

    private async Task ExpireAsync(RunSession session, CancellationTokenSource source)
    {
        lock (_gate)
        {
            if (!_runs.TryGetValue(session.RunId, out var current) ||
                !ReferenceEquals(current, session) ||
                !ReferenceEquals(session.Expiration, source)) return;
            _runs.Remove(session.RunId);
        }
        await DisposeSessionAsync(session).ConfigureAwait(false);
    }

    private async Task DisposeSessionAsync(RunSession session)
    {
        if (Interlocked.Exchange(ref session.DisposeStarted, 1) != 0) return;
        session.Expiration.Cancel();
        session.Expiration.Dispose();
        foreach (var adapterSessionId in session.Loaded.Values
            .SelectMany(value => value.AdapterSessionIds).Reverse())
        {
            try
            {
                await runtimeSessions.CancelAsync(
                    adapterSessionId, null, session.WorkspaceId, session.ProjectId)
                    .ConfigureAwait(false);
            }
            catch
            {
            }
        }
    }

}
