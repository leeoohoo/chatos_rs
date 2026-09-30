using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalAgentBootstrapSnapshot(
    string OwnerUserId,
    IReadOnlyList<WindowsLocalAgentModelSnapshot> ModelSnapshots,
    IReadOnlyList<ConversationModelOption> ModelOptions,
    WindowsLocalAgentCapabilitySnapshot MainChatCapabilities);

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
    private readonly SemaphoreSlim _gate = new(1, 1);

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
        ChatOSApiClient api)
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
    }

    public WindowsLocalAgentBootstrapSnapshot? Current { get; private set; }

    public async Task<WindowsLocalAgentBootstrapSnapshot> BootstrapForOwnerAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(ownerUserId);
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
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
            Current = null;
            if (_host.ActiveOwnerUserId is { } activeOwner &&
                !string.Equals(activeOwner, ownerUserId, StringComparison.Ordinal))
            {
                await _host.StopAsync(cancellationToken).ConfigureAwait(false);
            }
            var configured = await _api.GetAsync<IReadOnlyList<WindowsModelConfigDto>>(
                "ai-model-configs",
                cancellationToken).ConfigureAwait(false);
            var environment = new Dictionary<string, string>(StringComparer.Ordinal);
            var snapshots = new List<WindowsLocalAgentModelSnapshot>();
            var options = new List<ConversationModelOption>();
            try
            {
                foreach (var summary in configured.Where(value =>
                    value.Enabled != false && value.TaskEnabled != false && value.HasApiKey != false))
                {
                    var model = await _api.GetAsync<WindowsModelConfigDto>(
                        $"ai-model-configs/{Uri.EscapeDataString(summary.Id)}?include_secret=true",
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
                    snapshots.Add(new WindowsLocalAgentModelSnapshot(
                        ownerUserId,
                        model.Id,
                        ModelRevision(model, baseUri),
                        $"env:{variable}",
                        baseUri.AbsoluteUri,
                        model.ResolvedModel,
                        model.Provider.Trim(),
                        model.SupportsResponses ?? false,
                        model.SupportsImages,
                        null,
                        model.Temperature,
                        model.MaxOutputTokens,
                        NonEmpty(model.ThinkingLevel),
                        false,
                        null,
                        null,
                        null));
                    options.Add(new ConversationModelOption(
                        model.Id,
                        NonEmpty(model.Name) ?? model.ResolvedModel,
                        model.ResolvedModel,
                        NonEmpty(model.ThinkingLevel)));
                }
                if (snapshots.Count == 0)
                {
                    throw new InvalidOperationException(
                        "No enabled Local Agent model with a credential is configured.");
                }
                await _host.RestartForOwnerAsync(
                    ownerUserId,
                    environment,
                    cancellationToken).ConfigureAwait(false);
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
            var mainCapabilities = new WindowsLocalAgentCapabilitySnapshot(
                ownerUserId,
                "main_chat",
                WindowsLocalAgentCapabilityCatalog.Revision,
                "Use local_attachment_read for attachment content and treat authorized_local_ref values as opaque. Use create_task or create_tasks_with_prerequisites only for user-requested durable work. Conversation, task, and execution state remain local.",
                [],
                WindowsLocalAgentCapabilityCatalog.MainChatTools);
            _ = await _controlPlane.PublishCapabilitiesAsync(
                mainCapabilities,
                cancellationToken).ConfigureAwait(false);
            _ = await _controlPlane.PublishCapabilitiesAsync(
                new WindowsLocalAgentCapabilitySnapshot(
                    ownerUserId,
                    "task_runner",
                    mainCapabilities.CapabilityPolicyRevision,
                    "Complete the durable local task objective and return a concrete result. Do not create nested tasks.",
                    [],
                    []),
                cancellationToken).ConfigureAwait(false);

            var result = new WindowsLocalAgentBootstrapSnapshot(
                ownerUserId,
                snapshots,
                options,
                mainCapabilities);
            _runtimeSettings.Configure(ownerUserId, result);
            _conversationCommands.Configure(ownerUserId, result);
            _conversationHistory.Configure(ownerUserId);
            _toolWorker.Configure(ownerUserId);
            _realtime.Configure(ownerUserId);
            _petActivities.Configure(ownerUserId);
            _askUser.Configure(ownerUserId);
            _taskGraph.Configure(ownerUserId);
            _workspace.Configure(ownerUserId);
            _projectConversations.Configure(ownerUserId);
            _notepad.Configure(ownerUserId);
            _remoteConnections.Configure(ownerUserId);
            Current = result;
            return result;
        }
        finally
        {
            _gate.Release();
        }
    }

    public void Reset()
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
        Current = null;
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

    private static string ModelRevision(WindowsModelConfigDto model, Uri baseUri)
    {
        var fields = new[]
        {
            model.Id,
            model.Provider.Trim(),
            model.ResolvedModel,
            baseUri.AbsoluteUri,
            NonEmpty(model.ThinkingLevel) ?? string.Empty,
            model.Temperature?.ToString("R", CultureInfo.InvariantCulture) ?? string.Empty,
            model.MaxOutputTokens?.ToString(CultureInfo.InvariantCulture) ?? string.Empty,
            (model.SupportsResponses ?? false).ToString(CultureInfo.InvariantCulture),
            (model.SupportsImages ?? false).ToString(CultureInfo.InvariantCulture),
        };
        var digest = SHA256.HashData(Encoding.UTF8.GetBytes(string.Join('\0', fields)));
        return $"sha256-{Convert.ToHexString(digest).ToLowerInvariant()}";
    }

    private static string? NonEmpty(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();

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

        [JsonPropertyName("has_api_key")]
        public bool? HasApiKey { get; init; }

        [JsonPropertyName("supports_responses")]
        public bool? SupportsResponses { get; init; }

        [JsonPropertyName("supports_images")]
        public bool? SupportsImages { get; init; }

        [JsonPropertyName("thinking_level")]
        public string? ThinkingLevel { get; init; }

        [JsonPropertyName("temperature")]
        public double? Temperature { get; init; }

        [JsonPropertyName("max_output_tokens")]
        public int? MaxOutputTokens { get; init; }

        public string ResolvedModel => NonEmpty(ModelName) ?? NonEmpty(Model) ?? string.Empty;
    }
}
