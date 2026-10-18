using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Runtime;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalAgentBootstrapSnapshot(
    string OwnerUserId,
    IReadOnlyList<WindowsLocalAgentModelSnapshot> ModelSnapshots,
    IReadOnlyList<ConversationModelOption> ModelOptions,
    IReadOnlyList<WindowsLocalAgentPluginChoice> PluginChoices,
    WindowsLocalAgentCapabilitySnapshot MainChatCapabilities,
    WindowsLocalAgentCapabilitySnapshot PluginInstallerCapabilities);

public sealed class WindowsLocalAgentBootstrapService
{
    private readonly ILocalAgentHostClient _host;
    private readonly WindowsLocalAgentControlPlaneClient _controlPlane;
    private readonly WindowsLocalAgentModelCredentialStore _credentials;
    private readonly WindowsLocalAgentConversationRuntimeSettingsService _runtimeSettings;
    private readonly WindowsLocalAgentConversationCommandService _conversationCommands;
    private readonly WindowsLocalAgentConversationHistoryService _conversationHistory;
    private readonly WindowsLocalAgentPlatformToolWorker _toolWorker;
    private readonly WindowsLocalAgentRealtimeClient _realtime;
    private readonly WindowsLocalAgentPetActivityService _petActivities;
    private readonly WindowsLocalAgentAskUserPromptService _askUser;
    private readonly WindowsLocalAgentMessageTaskGraphService _taskGraph;
    private readonly WindowsLocalAgentWorkspaceService _workspace;
    private readonly WindowsLocalAgentProjectConversationService _projectConversations;
    private readonly WindowsLocalAgentNotepadService _notepad;
    private readonly WindowsLocalAgentRemoteConnectionMetadataService _remoteConnections;
    private readonly ChatOSApiClient _api;
    private readonly IAuthTokenStore _authTokens;
    private readonly ConnectorRuntimeContext _connectorRuntime;
    private readonly IConnectorGatewayClient _gateway;
    private readonly SemaphoreSlim _gate = new(1, 1);
    private readonly object _recoveryGate = new();
    private readonly object _configurationStateGate = new();
    private CancellationTokenSource? _recovery;
    private ulong _configurationGeneration;

    public WindowsLocalAgentBootstrapService(
        ILocalAgentHostClient host,
        WindowsLocalAgentControlPlaneClient controlPlane,
        WindowsLocalAgentModelCredentialStore credentials,
        WindowsLocalAgentConversationRuntimeSettingsService runtimeSettings,
        WindowsLocalAgentConversationCommandService conversationCommands,
        WindowsLocalAgentConversationHistoryService conversationHistory,
        WindowsLocalAgentPlatformToolWorker toolWorker,
        WindowsLocalAgentRealtimeClient realtime,
        WindowsLocalAgentPetActivityService petActivities,
        WindowsLocalAgentAskUserPromptService askUser,
        WindowsLocalAgentMessageTaskGraphService taskGraph,
        WindowsLocalAgentWorkspaceService workspace,
        WindowsLocalAgentProjectConversationService projectConversations,
        WindowsLocalAgentNotepadService notepad,
        WindowsLocalAgentRemoteConnectionMetadataService remoteConnections,
        ChatOSApiClient api,
        IAuthTokenStore authTokens,
        ConnectorRuntimeContext connectorRuntime,
        IConnectorGatewayClient gateway)
    {
        _host = host;
        _controlPlane = controlPlane;
        _credentials = credentials;
        _runtimeSettings = runtimeSettings;
        _conversationCommands = conversationCommands;
        _conversationHistory = conversationHistory;
        _toolWorker = toolWorker;
        _realtime = realtime;
        _petActivities = petActivities;
        _askUser = askUser;
        _taskGraph = taskGraph;
        _workspace = workspace;
        _projectConversations = projectConversations;
        _notepad = notepad;
        _remoteConnections = remoteConnections;
        _api = api;
        _authTokens = authTokens;
        _connectorRuntime = connectorRuntime;
        _gateway = gateway;
        if (host is WindowsLocalAgentHostLifecycle lifecycle)
            lifecycle.UnexpectedExit += OnUnexpectedHostExit;
    }

    public WindowsLocalAgentBootstrapSnapshot? Current { get; private set; }

    public async Task PrepareLocalServicesForOwnerAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(ownerUserId);
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_host.ActiveOwnerUserId is { } activeOwner &&
                !string.Equals(activeOwner, ownerUserId, StringComparison.Ordinal))
            {
                await _host.StopAsync(cancellationToken).ConfigureAwait(false);
            }

            await _host.StartForBootstrapAsync(ownerUserId, cancellationToken).ConfigureAwait(false);
            ConfigureOwnerServices(ownerUserId);
        }
        finally
        {
            _gate.Release();
        }
    }

    public Task<WindowsLocalAgentBootstrapSnapshot> BootstrapForOwnerAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default) =>
        BootstrapForOwnerCoreAsync(
            ownerUserId,
            configureLocalServicesBeforeModelBootstrap: true,
            cancellationToken: cancellationToken);

    private async Task<WindowsLocalAgentBootstrapSnapshot> BootstrapForOwnerCoreAsync(
        string ownerUserId,
        bool configureLocalServicesBeforeModelBootstrap,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(ownerUserId);
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            ulong configurationGeneration;
            lock (_configurationStateGate)
            {
                configurationGeneration = _configurationGeneration;
                ResetConsumers();
                Current = null;
            }
            if (_host.ActiveOwnerUserId is { } activeOwner &&
                !string.Equals(activeOwner, ownerUserId, StringComparison.Ordinal))
            {
                await _host.StopAsync(cancellationToken).ConfigureAwait(false);
            }

            // Local storage features must remain available even when the cloud Connector
            // cannot become active (for example, when the same account is connected on a
            // different device). Start the native host and bind the account before any
            // Connector- or model-dependent bootstrap work. The host is restarted below
            // with model credentials once the complete Agent bootstrap succeeds.
            await _host.StartForBootstrapAsync(ownerUserId, cancellationToken).ConfigureAwait(false);
            if (configureLocalServicesBeforeModelBootstrap)
                ConfigureOwnerServices(ownerUserId);

            var connectorSession = await _connectorRuntime
                .SessionConfigurationAsync(cancellationToken).ConfigureAwait(false)
                ?? throw new InvalidOperationException("The local connector is not paired.");
            var managedRuntime = await _gateway.GetManagedRuntimeConfigAsync(
                connectorSession.GatewayBaseUri,
                connectorSession.AccessToken,
                cancellationToken).ConfigureAwait(false);
            var agentCapability = await _gateway.GetAgentCapabilityAsync(
                connectorSession.GatewayBaseUri,
                connectorSession.AccessToken,
                "local_agent_execution_agent",
                cancellationToken).ConfigureAwait(false);
            if (!agentCapability.AgentEnabled ||
                agentCapability.AgentKey != "local_agent_execution_agent" ||
                agentCapability.OwnerUserId != ownerUserId)
            {
                throw new InvalidOperationException(
                    "The Local Agent execution capability is unavailable for this account.");
            }
            var configured = await _api.GetUserServiceAsync<IReadOnlyList<WindowsModelConfigDto>>(
                "model-configs",
                cancellationToken).ConfigureAwait(false);
            var environment = new Dictionary<string, string>(StringComparer.Ordinal);
            var snapshots = new List<WindowsLocalAgentModelSnapshot>();
            var options = new List<ConversationModelOption>();
            var taskModelChoices = new List<WindowsLocalAgentMcpChoice>();
            try
            {
                // Keep the main conversation model catalog aligned with macOS: every
                // enabled model with credentials is available to Chat. `task_enabled`
                // only gates binding a model to background/local Tasks; applying it here
                // incorrectly hides models that are valid for ordinary conversations.
                foreach (var summary in configured.Where(value =>
                    value.Enabled != false && value.HasApiKey != false))
                {
                    var model = await _api.GetUserServiceAsync<WindowsModelConfigDto>(
                        $"model-configs/{Uri.EscapeDataString(summary.Id)}?include_secret=true",
                        cancellationToken).ConfigureAwait(false);
                    if (!TryValidateModel(model, out var baseUri, out var credential)) continue;

                    _credentials.Save(credential, ownerUserId, model.Id);
                    var variable = WindowsLocalAgentModelCredentialStore.EnvironmentVariable(model.Id);
                    if (environment.ContainsKey(variable))
                    {
                        throw new InvalidOperationException(
                            "Two model identifiers map to the same Local Agent credential variable.");
                    }
                    var stored = _credentials.Load(ownerUserId, model.Id)
                        ?? throw new InvalidOperationException(
                            "A Local Agent model credential could not be reloaded from Credential Manager.");
                    environment.Add(variable, stored);
                    var thinkingLevel = ModelThinkingLevel(
                        model.ThinkingLevel,
                        model.ResolvedModel);
                    var snapshot = new WindowsLocalAgentModelSnapshot(
                        ownerUserId,
                        model.Id,
                        string.Empty,
                        $"env:{variable}",
                        baseUri.AbsoluteUri,
                        model.ResolvedModel,
                        model.Provider.Trim(),
                        model.SupportsResponses ?? false,
                        model.SupportsImages,
                        null,
                        model.Temperature,
                        model.MaxOutputTokens,
                        thinkingLevel,
                        false,
                        null,
                        null,
                        null);
                    snapshots.Add(snapshot with
                    {
                        ModelConfigRevision = ModelRevision(
                            snapshot,
                            SupportsReasoning(model)),
                    });
                    options.Add(new ConversationModelOption(
                        model.Id,
                        NonEmpty(model.Name) ?? model.ResolvedModel,
                        model.ResolvedModel,
                        NonEmpty(model.ThinkingLevel),
                        model.TaskEnabled != false,
                        model.HasApiKey != false,
                        SupportsReasoning(model)
                            ? ThinkingLevels(model.Provider, model.ResolvedModel)
                            : Array.Empty<string>()));
                    if (model.TaskEnabled != false)
                    {
                        var displayName = NonEmpty(model.Name) ?? model.ResolvedModel;
                        var usage = NonEmpty(model.TaskUsageScenario) is { } scenario
                            ? $" - {scenario}"
                            : string.Empty;
                        taskModelChoices.Add(new WindowsLocalAgentMcpChoice(
                            model.Id,
                            $"{displayName} ({model.Provider.Trim()}/{model.ResolvedModel}){usage}"));
                    }
                }
                if (snapshots.Count == 0)
                {
                    throw new InvalidOperationException(
                        "No enabled Local Agent model with a credential is configured.");
                }
                var memoryAccessToken = (await _authTokens
                    .GetAccessTokenAsync(cancellationToken)
                    .ConfigureAwait(false))?.Trim();
                if (!string.IsNullOrEmpty(memoryAccessToken))
                {
                    environment["CHATOS_MEMORY_ACCESS_TOKEN"] = memoryAccessToken;
                }
                await _host.RefreshCredentialsForOwnerAsync(
                    ownerUserId,
                    environment,
                    cancellationToken).ConfigureAwait(false);
                if (!configureLocalServicesBeforeModelBootstrap)
                    ConfigureOwnerServices(ownerUserId);
            }
            finally
            {
                environment.Clear();
            }

            foreach (var snapshot in snapshots)
            {
                _ = await _controlPlane.PublishModelAsync(snapshot, cancellationToken)
                    .ConfigureAwait(false);
            }
            var installedPluginChoices = await _toolWorker.ListPluginChoicesAsync(cancellationToken)
                .ConfigureAwait(false);
            if ((agentCapability.Mcps ?? []).Any(value =>
                    value.Binding.Required && !value.Available) ||
                (agentCapability.Plugins ?? []).Any(value => value.Binding.Required &&
                    !value.Available && value.Status != "partially_available"))
                throw new InvalidOperationException(
                    "A required Local Agent capability is unavailable.");
            var requiredPluginKeys = (agentCapability.Plugins ?? [])
                .Where(value => value.Binding.Required &&
                    (value.Available || value.Status == "partially_available"))
                .Select(value => value.Catalog.PluginKey)
                .Distinct(StringComparer.Ordinal)
                .Order(StringComparer.Ordinal)
                .ToArray();
            var installedPluginKeys = installedPluginChoices.Select(value => value.PluginKey)
                .ToHashSet(StringComparer.Ordinal);
            if (requiredPluginKeys.Any(value => !installedPluginKeys.Contains(value)))
                throw new InvalidOperationException(
                    "A required Local Agent Plugin is not installed and enabled.");
            var selectablePluginKeys = (agentCapability.Plugins ?? [])
                .Where(value => !value.Binding.Required &&
                    (value.Available || value.Status == "partially_available"))
                .Select(value => value.Catalog.PluginKey)
                .ToHashSet(StringComparer.Ordinal);
            var pluginChoices = installedPluginChoices
                .Where(value => selectablePluginKeys.Contains(value.PluginKey))
                .ToArray();
            var policyPluginKeys = selectablePluginKeys
                .Concat(requiredPluginKeys)
                .ToHashSet(StringComparer.Ordinal);
            var policyPluginChoices = installedPluginChoices
                .Where(value => policyPluginKeys.Contains(value.PluginKey))
                .OrderBy(value => value.PluginKey, StringComparer.Ordinal)
                .ThenBy(value => value.ComponentKey, StringComparer.Ordinal)
                .ToArray();
            var requiredPluginComponents = policyPluginChoices
                .Where(value => requiredPluginKeys.Contains(value.PluginKey, StringComparer.Ordinal))
                .Select(value => new
                {
                    plugin_key = value.PluginKey,
                    component_key = value.ComponentKey,
                    reason = "required by Local Agent capability policy",
                })
                .ToArray();
            var pluginReleaseSnapshots = policyPluginChoices.Select(value => new
            {
                plugin_key = value.PluginKey,
                component_key = value.ComponentKey,
                plugin_id = value.PluginId,
                release_id = value.ReleaseId,
                version = value.Version,
                artifact_sha256 = value.ArtifactSha256,
            }).ToArray();
            var builtinChoices = SelectableBuiltinChoices(agentCapability.Mcps ?? []);
            var externalChoices = SelectableExternalChoices(agentCapability.Mcps ?? []);
            var requiredBuiltinKinds = (agentCapability.Mcps ?? [])
                .Where(value => value.Binding.Required && value.Available)
                .Select(value => value.Resource.Runtime.ResolvedBuiltinKind)
                .Where(value => value is not null)
                .Select(value => value!)
                .Distinct(StringComparer.Ordinal)
                .Order(StringComparer.Ordinal)
                .ToArray();
            var requiredExternalIds = (agentCapability.Mcps ?? [])
                .Where(value => value.Binding.Required && value.Available &&
                    value.Resource.Runtime.ResolvedBuiltinKind is null &&
                    !value.Resource.Id.StartsWith("system_mcp_", StringComparison.Ordinal) &&
                    value.Resource.Runtime.Kind.Equals("http", StringComparison.OrdinalIgnoreCase) &&
                    NonEmpty(value.Resource.Runtime.Url) is not null)
                .Select(value => value.Resource.Id)
                .Distinct(StringComparer.Ordinal)
                .Order(StringComparer.Ordinal)
                .ToArray();
            var externalIds = externalChoices.Select(value => value.Value)
                .Concat(requiredExternalIds)
                .ToHashSet(StringComparer.Ordinal);
            var externalTools = _toolWorker.Configure(
                ownerUserId,
                agentCapability.Mcps ?? [],
                externalIds);
            var mainSkillBundle = WindowsLocalAgentProductSkillBundle.BuildMainChat();
            var externalSkillBundle = WindowsLocalAgentExternalMcpSkillBundle.Build(externalTools);
            var taskPolicyJson = JsonSerializer.Serialize(new
            {
                max_iterations = managedRuntime.LocalTaskExecutionSettings.MaxIterations,
                allowed_model_config_ids = taskModelChoices
                    .Select(value => value.Value)
                    .Distinct(StringComparer.Ordinal)
                    .Order(StringComparer.Ordinal)
                    .ToArray(),
                enabled_builtin_kinds = requiredBuiltinKinds,
                external_mcp_config_ids = requiredExternalIds,
                plugin_components = requiredPluginComponents,
                plugin_release_snapshots = pluginReleaseSnapshots,
            });
            var mainTools = WindowsLocalAgentCapabilityCatalog.MainChatToolsFor(
                pluginChoices,
                builtinChoices,
                externalChoices,
                taskModelChoices);
            var taskTools = WindowsLocalAgentCapabilityCatalog.TaskExecutionToolsFor(externalTools);
            var mainCapabilities = new WindowsLocalAgentCapabilitySnapshot(
                ownerUserId,
                "main_chat",
                CapabilityRevision(
                    agentCapability.PolicyRevision,
                    policyPluginChoices,
                    builtinChoices,
                    externalChoices,
                    taskModelChoices,
                    externalTools,
                    mainSkillBundle.Revision,
                    externalSkillBundle?.Revision,
                    mainTools,
                    taskTools,
                    taskPolicyJson),
                mainSkillBundle.PolicyInstructions,
                [mainSkillBundle.CatalogInputItem],
                mainTools);
            _ = await _controlPlane.PublishCapabilitiesAsync(
                new WindowsLocalAgentCapabilitySnapshot(
                    ownerUserId,
                    "skill_bundle_internal",
                    mainCapabilities.CapabilityPolicyRevision,
                    mainSkillBundle.Json,
                    [],
                    []),
                cancellationToken).ConfigureAwait(false);
            if (externalSkillBundle is not null)
            {
                _ = await _controlPlane.PublishCapabilitiesAsync(
                    new WindowsLocalAgentCapabilitySnapshot(
                        ownerUserId,
                        "task_skill_bundle_internal",
                        mainCapabilities.CapabilityPolicyRevision,
                        externalSkillBundle.Json,
                        [],
                        []),
                    cancellationToken).ConfigureAwait(false);
            }
            _ = await _controlPlane.PublishCapabilitiesAsync(
                new WindowsLocalAgentCapabilitySnapshot(
                    ownerUserId,
                    "task_policy_internal",
                    mainCapabilities.CapabilityPolicyRevision,
                    taskPolicyJson,
                    [],
                    []),
                cancellationToken).ConfigureAwait(false);
            _ = await _controlPlane.PublishCapabilitiesAsync(
                new WindowsLocalAgentCapabilitySnapshot(
                    ownerUserId,
                    "task_execution",
                    mainCapabilities.CapabilityPolicyRevision,
                    "Complete the durable local task objective using only the project bound to its source conversation and enabled local Plugins. Project writes and terminal commands require Host approval; Plugin permissions and per-call approval are enforced by the native client. Do not create nested tasks.",
                    externalSkillBundle is null ? [] : [externalSkillBundle.CatalogInputItem],
                    taskTools),
                cancellationToken).ConfigureAwait(false);
            _ = await _controlPlane.PublishCapabilitiesAsync(
                mainCapabilities,
                cancellationToken).ConfigureAwait(false);
            var pluginInstallerCapabilities = new WindowsLocalAgentCapabilitySnapshot(
                ownerUserId,
                "main_chat",
                PluginInstallerCapabilityRevision(
                    mainCapabilities.CapabilityPolicyRevision),
                WindowsLocalAgentCapabilityCatalog.PluginInstallerInstructions,
                [],
                WindowsLocalAgentCapabilityCatalog.PluginInstallerTools);
            _ = await _controlPlane.PublishCapabilitiesAsync(
                pluginInstallerCapabilities,
                cancellationToken).ConfigureAwait(false);

            var result = new WindowsLocalAgentBootstrapSnapshot(
                ownerUserId,
                snapshots,
                options,
                pluginChoices,
                mainCapabilities,
                pluginInstallerCapabilities);
            cancellationToken.ThrowIfCancellationRequested();
            lock (_configurationStateGate)
            {
                if (configurationGeneration != _configurationGeneration)
                {
                    throw new OperationCanceledException(
                        "Local Agent bootstrap was superseded by an account reset.");
                }
                _runtimeSettings.Configure(ownerUserId, result);
                _conversationCommands.Configure(ownerUserId, result);
                Current = result;
            }
            return result;
        }
        finally
        {
            _gate.Release();
        }
    }

    public void Reset()
    {
        lock (_recoveryGate)
        {
            _recovery?.Cancel();
            _recovery = null;
        }
        lock (_configurationStateGate)
        {
            _configurationGeneration++;
            ResetConsumers();
            Current = null;
        }
    }

    private void ConfigureOwnerServices(string ownerUserId)
    {
        _conversationHistory.Configure(ownerUserId);
        _realtime.Configure(ownerUserId);
        _petActivities.Configure(ownerUserId);
        _askUser.Configure(ownerUserId);
        _taskGraph.Configure(ownerUserId);
        _workspace.Configure(ownerUserId);
        _projectConversations.Configure(ownerUserId);
        _notepad.Configure(ownerUserId);
        _remoteConnections.Configure(ownerUserId);
    }

    private void ResetConsumers()
    {
        _runtimeSettings.Reset();
        _conversationCommands.Reset();
        _conversationHistory.Reset();
        _toolWorker.Reset();
        _realtime.Reset();
        _petActivities.Reset();
        _askUser.Reset();
        _taskGraph.Reset();
        _workspace.Reset();
        _projectConversations.Reset();
        _notepad.Reset();
        _remoteConnections.Reset();
    }

    private void OnUnexpectedHostExit(object? sender, EventArgs args)
    {
        var owner = Current?.OwnerUserId;
        if (owner is null) return;
        CancellationTokenSource source;
        lock (_recoveryGate)
        {
            if (_recovery is not null) return;
            source = new CancellationTokenSource();
            _recovery = source;
        }
        _ = RecoverHostAsync(owner, source);
    }

    private async Task RecoverHostAsync(string ownerUserId, CancellationTokenSource source)
    {
        var delays = new[]
        {
            TimeSpan.FromMilliseconds(250),
            TimeSpan.FromSeconds(1),
            TimeSpan.FromSeconds(2),
            TimeSpan.FromSeconds(5),
            TimeSpan.FromSeconds(10),
        };
        try
        {
            foreach (var delay in delays)
            {
                await Task.Delay(delay, source.Token).ConfigureAwait(false);
                try
                {
                    await BootstrapForOwnerCoreAsync(
                        ownerUserId,
                        configureLocalServicesBeforeModelBootstrap: false,
                        cancellationToken: source.Token).ConfigureAwait(false);
                    return;
                }
                catch (OperationCanceledException) when (source.IsCancellationRequested)
                {
                    return;
                }
                catch
                {
                    // Retry with bounded backoff. Bootstrap reads credentials again instead of
                    // retaining model secrets in the lifecycle object.
                }
            }
        }
        catch (OperationCanceledException)
        {
        }
        finally
        {
            lock (_recoveryGate)
            {
                if (ReferenceEquals(_recovery, source)) _recovery = null;
            }
            source.Dispose();
        }
    }

    private static IReadOnlyList<WindowsLocalAgentMcpChoice> SelectableBuiltinChoices(
        IReadOnlyList<ConnectorResolvedMcp> mcps)
    {
        var candidates = mcps.Where(value =>
            !value.Binding.Required && value.Binding.Enabled && value.Resource.Enabled &&
            value.Resource.Runtime.ResolvedBuiltinKind is not null).ToArray();
        var available = candidates.Select(value => value.Resource.Runtime.ResolvedBuiltinKind!)
            .ToHashSet(StringComparer.Ordinal);
        return candidates
            .Where(value => value.Resource.Runtime.ResolvedBuiltinKind != "CodeMaintainerWrite" ||
                available.Contains("CodeMaintainerRead"))
            .Select(value => new WindowsLocalAgentMcpChoice(
                value.Resource.Runtime.ResolvedBuiltinKind!,
                McpChoiceTitle(value, value.Resource.Runtime.ResolvedBuiltinKind!)))
            .DistinctBy(value => value.Value, StringComparer.Ordinal)
            .OrderBy(value => value.Value, StringComparer.Ordinal)
            .ToArray();
    }

    private static IReadOnlyList<WindowsLocalAgentMcpChoice> SelectableExternalChoices(
        IReadOnlyList<ConnectorResolvedMcp> mcps) => mcps
            .Where(value => !value.Binding.Required && value.Binding.Enabled &&
                value.Resource.Enabled && value.Resource.Runtime.ResolvedBuiltinKind is null &&
                !value.Resource.Id.StartsWith("system_mcp_", StringComparison.Ordinal) &&
                value.Resource.Runtime.Kind.Equals("http", StringComparison.OrdinalIgnoreCase) &&
                NonEmpty(value.Resource.Runtime.Url) is not null)
            .Select(value => new WindowsLocalAgentMcpChoice(
                value.Resource.Id,
                McpChoiceTitle(value, value.Resource.Id)))
            .DistinctBy(value => value.Value, StringComparer.Ordinal)
            .OrderBy(value => value.Value, StringComparer.Ordinal)
            .ToArray();

    private static string McpChoiceTitle(ConnectorResolvedMcp item, string value)
    {
        var display = NonEmpty(item.Resource.DisplayName) ?? NonEmpty(item.Resource.Name) ?? value;
        var title = display == value ? value : $"{display} ({value})";
        if (NonEmpty(item.Resource.Description) is { } description) title += $" - {description}";
        var names = item.ToolSnapshot
            .Where(tool => tool.ValueKind == JsonValueKind.Object &&
                tool.TryGetProperty("name", out var name) &&
                name.ValueKind == JsonValueKind.String)
            .Select(tool => tool.GetProperty("name").GetString())
            .Where(name => !string.IsNullOrWhiteSpace(name))
            .Take(12)
            .ToArray();
        if (names.Length > 0) title += $" [tools: {string.Join(", ", names)}]";
        return title;
    }

    private static string CapabilityRevision(
        string policyRevision,
        IReadOnlyList<WindowsLocalAgentPluginChoice> plugins,
        IReadOnlyList<WindowsLocalAgentMcpChoice> builtinChoices,
        IReadOnlyList<WindowsLocalAgentMcpChoice> externalChoices,
        IReadOnlyList<WindowsLocalAgentMcpChoice> taskModelChoices,
        IReadOnlyList<WindowsLocalAgentExternalMcpTool> externalTools,
        string mainSkillBundleRevision,
        string? taskSkillBundleRevision,
        IReadOnlyList<JsonElement> mainTools,
        IReadOnlyList<JsonElement> taskTools,
        string taskPolicyJson)
    {
        var fields = new[] { policyRevision, WindowsLocalAgentCapabilityCatalog.Revision }
            .Concat(plugins
                .OrderBy(value => value.PluginKey, StringComparer.Ordinal)
                .ThenBy(value => value.ComponentKey, StringComparer.Ordinal)
                .SelectMany(value => new[]
                {
                    value.PluginKey,
                    value.ComponentKey,
                    value.PluginId,
                    value.ReleaseId,
                    value.Version,
                    value.ArtifactSha256,
                }))
            .Concat(builtinChoices.Select(value => value.Value).Order(StringComparer.Ordinal))
            .Concat(externalChoices.Select(value => value.Value).Order(StringComparer.Ordinal))
            .Concat(taskModelChoices
                .OrderBy(value => value.Value, StringComparer.Ordinal)
                .SelectMany(value => new[] { value.Value, value.Title }))
            .Concat(externalTools.OrderBy(value => value.PublicName, StringComparer.Ordinal)
                .Select(value => WindowsLocalAgentExternalMcpSkillBundle.CanonicalJson(value.ModelTool())))
            .Append(mainSkillBundleRevision)
            .Append(taskSkillBundleRevision ?? string.Empty)
            .Concat(mainTools.Select(WindowsLocalAgentExternalMcpSkillBundle.CanonicalJson))
            .Concat(taskTools.Select(WindowsLocalAgentExternalMcpSkillBundle.CanonicalJson))
            .Append(taskPolicyJson);
        var digest = SHA256.HashData(Encoding.UTF8.GetBytes(string.Join('\0', fields)));
        return $"local-agent-{Convert.ToHexString(digest).ToLowerInvariant()}";
    }

    internal static string PluginInstallerCapabilityRevision(string mainRevision)
    {
        var fields = new[]
        {
            "plugin-installer-chat-v1",
            mainRevision,
            WindowsLocalAgentCapabilityCatalog.PluginInstallerInstructions,
        }.Concat(WindowsLocalAgentCapabilityCatalog.PluginInstallerTools.Select(
            value => value.GetRawText()));
        var digest = SHA256.HashData(Encoding.UTF8.GetBytes(string.Join('\0', fields)));
        return $"plugin-installer-{Convert.ToHexString(digest).ToLowerInvariant()}";
    }

    private static bool TryValidateModel(
        WindowsModelConfigDto model,
        out Uri baseUri,
        out string credential)
    {
        baseUri = null!;
        credential = model.ApiKey?.Trim() ?? string.Empty;
        if (!Uri.TryCreate(model.BaseUrl?.Trim(), UriKind.Absolute, out var parsed) ||
            parsed.Scheme is not ("http" or "https") ||
            credential.Length == 0 ||
            model.ResolvedModel.Length == 0 ||
            model.Provider.Trim().Length == 0)
        {
            return false;
        }
        baseUri = parsed;
        return true;
    }

    internal static string ModelRevision(
        WindowsLocalAgentModelSnapshot snapshot,
        bool supportsReasoning)
    {
        var fields = new[]
        {
            snapshot.ModelConfigRef,
            snapshot.Provider,
            snapshot.Model,
            snapshot.BaseUrl,
            snapshot.ThinkingLevel ?? string.Empty,
            supportsReasoning.ToString(CultureInfo.InvariantCulture),
            snapshot.Temperature?.ToString("R", CultureInfo.InvariantCulture) ?? string.Empty,
            snapshot.MaxOutputTokens?.ToString(CultureInfo.InvariantCulture) ?? string.Empty,
            snapshot.SupportsResponses.ToString(CultureInfo.InvariantCulture),
            snapshot.SupportsImages switch
            {
                true => bool.TrueString,
                false => bool.FalseString,
                null => "<null>",
            },
        };
        var digest = SHA256.HashData(Encoding.UTF8.GetBytes(string.Join('\0', fields)));
        return $"sha256-{Convert.ToHexString(digest).ToLowerInvariant()}";
    }

    private static string? NonEmpty(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private static bool SupportsReasoning(WindowsModelConfigDto model) =>
        WindowsModelReasoningCapabilities.ForModel(model.ResolvedModel) is not null ||
        (model.SupportsReasoning ?? !string.IsNullOrWhiteSpace(model.ThinkingLevel));

    internal static string? ModelThinkingLevel(string? level, string model)
    {
        if (WindowsModelReasoningCapabilities.ForModel(model) is { } capability)
            return capability.Normalize(level) ?? capability.DefaultLevel;
        return NonEmpty(level);
    }

    private static IReadOnlyList<string> ThinkingLevels(string? provider, string model)
    {
        if (WindowsModelReasoningCapabilities.ForModel(model) is { } capability)
            return capability.Levels;
        var normalized = (provider ?? string.Empty).Trim().ToLowerInvariant().Replace('-', '_');
        return normalized switch
        {
            "gpt" or "openai" => new[] { "none", "minimal", "low", "medium", "high", "xhigh" },
            "deepseek" => new[] { "none", "low", "medium", "high", "max" },
            "kimi" or "kimik2" or "moonshot" => new[] { "none", "auto", "low", "medium", "high", "xhigh" },
            _ => new[] { "none", "low", "medium", "high", "xhigh" },
        };
    }

    private sealed record WindowsModelConfigDto
    {
        [JsonPropertyName("id")]
        public required string Id { get; init; }

        [JsonPropertyName("name")]
        public string? Name { get; init; }

        [JsonPropertyName("provider")]
        public required string Provider { get; init; }

        [JsonPropertyName("model")]
        public string? Model { get; init; }

        [JsonPropertyName("model_name")]
        public string? ModelName { get; init; }

        [JsonPropertyName("base_url")]
        public string? BaseUrl { get; init; }

        [JsonPropertyName("api_key")]
        public string? ApiKey { get; init; }

        [JsonPropertyName("enabled")]
        public bool? Enabled { get; init; }

        [JsonPropertyName("task_enabled")]
        public bool? TaskEnabled { get; init; }

        [JsonPropertyName("task_usage_scenario")]
        public string? TaskUsageScenario { get; init; }

        [JsonPropertyName("has_api_key")]
        public bool? HasApiKey { get; init; }

        [JsonPropertyName("supports_responses")]
        public bool? SupportsResponses { get; init; }

        [JsonPropertyName("supports_images")]
        public bool? SupportsImages { get; init; }

        [JsonPropertyName("thinking_level")]
        public string? ThinkingLevel { get; init; }

        [JsonPropertyName("supports_reasoning")]
        public bool? SupportsReasoning { get; init; }

        [JsonPropertyName("temperature")]
        public double? Temperature { get; init; }

        [JsonPropertyName("max_output_tokens")]
        public int? MaxOutputTokens { get; init; }

        public string ResolvedModel => NonEmpty(ModelName) ?? NonEmpty(Model) ?? string.Empty;
    }
}
