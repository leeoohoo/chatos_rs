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
            .Select(record => record.PluginId)
            .ToHashSet(StringComparer.Ordinal);
        return (await pluginManagement.ListAsync(cancellationToken).ConfigureAwait(false))
            .Where(plugin => plugin.Installed && plugin.Enabled && installed.Contains(plugin.PluginId))
            .OrderBy(plugin => plugin.DisplayName, StringComparer.OrdinalIgnoreCase)
            .ThenBy(plugin => plugin.PluginId, StringComparer.Ordinal)
            .Select(plugin => new WindowsLocalAgentPluginChoice(
                plugin.PluginKey,
                plugin.DisplayName,
                plugin.Description))
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
            "capability_skill_activate" => await ActivateSkillAsync(
                session, arguments, cancellationToken).ConfigureAwait(false),
            "capability_skill_read_resource" => await ReadSkillResourceAsync(
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
        var records = await installedPlugins.ListAsync(cancellationToken).ConfigureAwait(false);
        var selectedPluginKeys = context.Authorization.PluginKeys;
        var options = records
            .Where(record => enabled.ContainsKey(record.PluginId))
            .Where(record => context.Authorization.IsLegacyUnrestricted ||
                selectedPluginKeys.Contains(enabled[record.PluginId].PluginKey))
            .OrderBy(record => record.PluginId, StringComparer.Ordinal)
            .Take(MaximumPlugins)
            .Select((record, index) => new PluginOption(
                $"plugin_{index + 1}",
                enabled[record.PluginId],
                record))
            .ToArray();
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
        var matches = session.Options.Where(option =>
        {
            var searchable = $"{option.Catalog.DisplayName} {option.Catalog.Description}";
            return terms.Any(term => searchable.Contains(term, StringComparison.OrdinalIgnoreCase));
        }).Take(12).Select(option => new
        {
            plugin_option = option.Token,
            name = option.Catalog.DisplayName,
                description = WindowsLocalAgentPluginRuntimeSupport.Limit(
                    option.Catalog.Description, 1_000),
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
        return JsonSerializer.SerializeToElement(new
        {
            plugin_option = option.Token,
            name = option.Catalog.DisplayName,
            tools = plugin.Tools.Select(tool => new
            {
                tool_option = tool.Token,
                name = tool.Name,
                description = tool.Description,
                input_schema = tool.InputSchema,
                approval = tool.Policy.ApprovalMode,
                required_skills = tool.SkillGate?.CatalogSkillNames ?? Array.Empty<string>(),
            }),
            skills = plugin.Tools
                .SelectMany(tool =>
                    tool.SkillGate?.CatalogSkillNames ?? Array.Empty<string>())
                .Distinct(StringComparer.Ordinal)
                .Order(StringComparer.Ordinal)
                .Select(name => new { name, role = "leaf" }),
        }, JsonOptions);
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
        if (!arguments.TryGetProperty("arguments", out var toolArguments) ||
            toolArguments.ValueKind != JsonValueKind.Object)
        {
            throw new InvalidOperationException("The Plugin tool arguments are invalid.");
        }

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

        var componentKeys = await manifestLoader.ListMcpComponentsAsync(
            option.Record, cancellationToken).ConfigureAwait(false);
        var tools = new List<PluginTool>();
        var adapterSessions = new List<string>();
        try
        {
            foreach (var componentKey in componentKeys)
            {
                if (tools.Count >= MaximumTools) break;
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
                    option.Catalog.DisplayName,
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

    private static PluginOption Option(RunSession session, JsonElement arguments)
    {
        var token = WindowsLocalAgentProjectToolExecutor.RequiredString(
            arguments, "plugin_option");
        return session.Options.FirstOrDefault(value => value.Token == token)
            ?? throw new InvalidOperationException("The Plugin option is invalid or expired.");
    }

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
