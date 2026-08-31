using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Plugins;

internal sealed class PluginRelayHandler(
    IInstalledPluginStore installedPlugins,
    ILocalPluginManagementService pluginManagement,
    PluginManifestLoader manifestLoader,
    IPluginMcpClientFactory clientFactory,
    PluginRuntimeSessionStore sessions,
    ConnectorRuntimeContext runtime,
    ILocalProjectPathResolver projectPaths,
    CommandApprovalCoordinator approvals,
    TimeProvider? timeProvider = null,
    PluginArtifactRegistry? artifactRegistry = null) : IRelayRequestHandler
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;
    private readonly PluginArtifactRegistry _artifactRegistry = artifactRegistry ?? new PluginArtifactRegistry();

    public bool CanHandle(string requestType) => requestType is
        "plugin_prepare_request" or "plugin_execute_request" or "plugin_cancel_request";

    public string ResponseType(string requestType) => requestType switch
    {
        "plugin_execute_request" => "plugin_execute_response",
        "plugin_cancel_request" => "plugin_cancel_response",
        _ => "plugin_prepare_response",
    };

    public async Task<RelayHandlerResult> HandleAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        try
        {
            return request.Type switch
            {
                "plugin_prepare_request" => await PrepareAsync(request, cancellationToken).ConfigureAwait(false),
                "plugin_execute_request" => await ExecuteAsync(request, cancellationToken).ConfigureAwait(false),
                _ => await CancelAsync(request).ConfigureAwait(false),
            };
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (RelayRequestException)
        {
            throw;
        }
        catch (TimeoutException exception)
        {
            throw new RelayRequestException(408, exception.Message);
        }
        catch (PluginRuntimeException exception)
        {
            var status = exception.Message.Contains("permission", StringComparison.OrdinalIgnoreCase)
                ? 403
                : exception.Message.Contains("timed out", StringComparison.OrdinalIgnoreCase)
                    ? 408
                    : 400;
            throw new RelayRequestException(status, exception.Message);
        }
    }

    private async Task<RelayHandlerResult> PrepareAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        var body = RequireObject(request.Body);
        var runId = RequiredString(body, "run_id");
        var pluginId = RequiredString(body, "plugin_id");
        var releaseId = RequiredString(body, "release_id");
        var artifactSha256 = RequiredString(body, "artifact_sha256").ToLowerInvariant();
        var componentKey = RequiredString(body, "component_key");
        var serverKey = OptionalString(body, "server_key");
        var permissionSnapshot = StringArray(body, "permission_snapshot", required: true)
            .ToHashSet(StringComparer.Ordinal);
        var allowlist = StringArray(body, "tool_allowlist", required: false).ToHashSet(StringComparer.Ordinal);
        var blocklist = StringArray(body, "tool_blocklist", required: false).ToHashSet(StringComparer.Ordinal);
        var scope = ResolveScope(request, permissionSnapshot);

        var enabled = (await pluginManagement.ListAsync(cancellationToken).ConfigureAwait(false))
            .FirstOrDefault(plugin => string.Equals(plugin.PluginId, pluginId, StringComparison.Ordinal));
        var record = await installedPlugins.GetAsync(pluginId, cancellationToken).ConfigureAwait(false);
        if (enabled is null || !enabled.Enabled || record is null ||
            !string.Equals(record.ReleaseId, releaseId, StringComparison.Ordinal) ||
            !string.Equals(record.ArtifactSha256, artifactSha256, StringComparison.OrdinalIgnoreCase))
        {
            throw new PluginRuntimeException("Plugin is not installed, is disabled, or its Release does not match.");
        }

        var adapterSessionId = Guid.NewGuid().ToString("D").ToLowerInvariant();
        var launch = await manifestLoader.PrepareAsync(
            record,
            componentKey,
            serverKey,
            adapterSessionId,
            scope.ProjectRoot,
            permissionSnapshot,
            runtime.Snapshot.State?.User.Id
                ?? throw new PluginRuntimeException("Plugin Connector owner is unavailable."),
            runtime.Snapshot.State?.DeviceId
                ?? throw new PluginRuntimeException("Plugin Connector device is unavailable."),
            cancellationToken).ConfigureAwait(false);
        var client = clientFactory.Create(launch);
        try
        {
            await client.StartAsync(cancellationToken).ConfigureAwait(false);
            var initialized = await client.InitializeAsync(cancellationToken).ConfigureAwait(false);
            var seenNames = new HashSet<string>(StringComparer.Ordinal);
            var tools = initialized.Tools
                .Where(tool =>
                {
                    if (tool.ValueKind != JsonValueKind.Object ||
                        !tool.TryGetProperty("name", out var nameValue) ||
                        nameValue.ValueKind != JsonValueKind.String)
                    {
                        return false;
                    }

                    var name = nameValue.GetString()?.Trim();
                    return !string.IsNullOrEmpty(name) &&
                        seenNames.Add(name) &&
                        (allowlist.Count == 0 || allowlist.Contains(name)) &&
                        !blocklist.Contains(name);
                })
                .OrderBy(tool => tool.GetProperty("name").GetString(), StringComparer.Ordinal)
                .Select(tool => tool.Clone())
                .ToArray();
            if (tools.Length is < 1 or > 200)
            {
                throw new PluginRuntimeException("Plugin MCP tool count is invalid.");
            }

            var toolsValue = JsonSerializer.SerializeToElement(tools, JsonOptions);
            var instructionsValue = initialized.Instructions is null
                ? JsonSerializer.SerializeToElement<object?>(null, JsonOptions)
                : JsonSerializer.SerializeToElement(initialized.Instructions, JsonOptions);
            var toolHash = CanonicalHash(toolsValue);
            var instructionsHash = CanonicalHash(instructionsValue);
            var snapshotHash = CanonicalHash(JsonSerializer.SerializeToElement(new
            {
                identity = $"{pluginId}:{releaseId}:{launch.ComponentKey}",
                transport = launch.Transport,
                tools = toolHash,
                instructions = instructionsHash,
                credential_snapshot_sha256 = launch.CredentialBinding?.SnapshotSha256,
                oauth_snapshot_sha256 = launch.OAuthBinding?.SnapshotSha256,
            }, JsonOptions));
            var sessionHash = CanonicalHash(JsonSerializer.SerializeToElement(new
            {
                run_id = runId,
                adapter_session_id = adapterSessionId,
                snapshot_sha256 = snapshotHash,
            }, JsonOptions));
            var identity = new PluginRuntimeIdentity(
                runId,
                pluginId,
                releaseId,
                record.Version,
                record.ArtifactSha256,
                launch.ComponentKey,
                adapterSessionId,
                scope.WorkspaceId);
            await sessions.InsertAsync(
                identity,
                client,
                tools,
                permissionSnapshot,
                launch.Server.RequiresExclusiveExecution,
                launch.InstallationPath,
                launch.ArtifactPath,
                launch.VisualSessionPath,
                launch.DisplayName).ConfigureAwait(false);
            var expiresAt = _timeProvider.GetUtcNow().AddDays(7).ToUnixTimeSeconds();
            return RelayHandlerResult.Ok(JsonSerializer.SerializeToElement(new
            {
                run_id = runId,
                plugin_id = pluginId,
                release_id = releaseId,
                version = record.Version,
                artifact_sha256 = record.ArtifactSha256,
                component_key = launch.ComponentKey,
                mcp = new
                {
                    plugin_id = pluginId,
                    release_id = releaseId,
                    version = record.Version,
                    artifact_sha256 = record.ArtifactSha256,
                    component_key = launch.ComponentKey,
                    transport = launch.Transport,
                    credential_snapshot_sha256 = launch.CredentialBinding?.SnapshotSha256,
                    oauth_connection_id = launch.OAuthBinding?.ConnectionId,
                    oauth_snapshot_sha256 = launch.OAuthBinding?.SnapshotSha256,
                    server_instructions = instructionsValue,
                    server_instructions_sha256 = instructionsHash,
                    tools = toolsValue,
                    tool_snapshot_sha256 = toolHash,
                    snapshot_sha256 = snapshotHash,
                },
                operations = new[] { "mcp_tools_call", "mcp_health_check" },
                adapter_session_id = adapterSessionId,
                session_sha256 = sessionHash,
                expires_at = expiresAt,
            }, JsonOptions));
        }
        catch
        {
            await client.TerminateAsync().ConfigureAwait(false);
            await client.DisposeAsync().ConfigureAwait(false);
            TryDeleteDirectory(launch.VisualSessionPath);
            throw;
        }
    }

    private async Task<RelayHandlerResult> ExecuteAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        var body = RequireObject(request.Body);
        var pluginId = RequiredString(body, "plugin_id");
        var releaseId = RequiredString(body, "release_id");
        var artifactSha256 = RequiredString(body, "artifact_sha256");
        var componentKey = RequiredString(body, "component_key");
        var adapterSessionId = RequiredString(body, "adapter_session_id");
        var invocationId = RequiredString(body, "invocation_id");
        var operation = RequiredString(body, "operation");
        var toolName = RequiredString(body, "tool_name");
        if (operation != "mcp_tools_call")
        {
            throw new PluginRuntimeException("Plugin operation is not supported.");
        }

        var scope = ResolveScope(request, null);
        var identity = sessions.Validate(
            adapterSessionId,
            pluginId,
            releaseId,
            artifactSha256,
            componentKey,
            scope.WorkspaceId);
        if (OptionalString(body, "conversation_id") is { } conversationId)
        {
            sessions.BindVisualOwner(adapterSessionId, new PluginVisualSessionOwner(
                conversationId,
                OptionalString(body, "conversation_turn_id"),
                OptionalString(body, "source_user_message_id"),
                OptionalString(body, "task_id"),
                OptionalString(body, "task_run_id"),
                OptionalString(body, "task_title")));
        }
        var definition = sessions.ToolDefinition(adapterSessionId, toolName);
        var arguments = body.TryGetProperty("arguments", out var argumentValue)
            ? argumentValue.Clone()
            : JsonSerializer.SerializeToElement(new { }, JsonOptions);
        var policy = ToolPolicy.Parse(definition, componentKey, toolName);
        var granted = sessions.Permissions(adapterSessionId);
        var required = policy.RequiredPermissions(arguments);
        if (required.Any(permission => !granted.Contains(permission)))
        {
            throw new PluginRuntimeException("Plugin tool requested a local permission that was not granted.");
        }

        if (policy.ApprovalMode == "per_call")
        {
            var state = runtime.Snapshot.State
                ?? throw new PluginRuntimeException("Plugin Connector identity is unavailable.");
            var summary = SafeArgumentSummary(toolName, arguments);
            var outcome = await approvals.RequestAsync(
                new CommandApprovalRequest(
                    request.RequestId,
                    state.User.Id,
                    state.DeviceId,
                    scope.WorkspaceId ?? "device",
                    $"Plugin · {toolName}",
                    [summary],
                    sessions.WorkingDirectory(adapterSessionId),
                    componentKey.Contains("browser", StringComparison.OrdinalIgnoreCase)
                        ? "plugin_browser"
                        : "plugin_computer_use",
                    $"plugin:{adapterSessionId}"),
                new ConnectorApprovalRisk(policy.RiskLevel, $"Plugin requested a local operation: {summary}"),
                cancellationToken).ConfigureAwait(false);
            if (!outcome.Approved)
            {
                throw new PluginRuntimeException("User did not approve this Plugin operation.");
            }
        }

        var result = await sessions.CallAsync(
            adapterSessionId,
            invocationId,
            toolName,
            arguments,
            policy.Timeout,
            cancellationToken).ConfigureAwait(false);
        var connectorState = runtime.Snapshot.State
            ?? throw new PluginRuntimeException("Plugin Connector identity is unavailable.");
        result = await _artifactRegistry.RegisterAsync(
            identity,
            connectorState.User.Id,
            connectorState.DeviceId,
            sessions.ArtifactDirectory(adapterSessionId),
            toolName,
            result,
            cancellationToken).ConfigureAwait(false);
        return RelayHandlerResult.Ok(JsonSerializer.SerializeToElement(new
        {
            plugin_id = identity.PluginId,
            release_id = identity.ReleaseId,
            version = identity.Version,
            artifact_sha256 = identity.ArtifactSha256,
            component_key = identity.ComponentKey,
            invocation_id = invocationId,
            tool_name = toolName,
            adapter_session_id = adapterSessionId,
            operation,
            result,
        }, JsonOptions));
    }

    private async Task<RelayHandlerResult> CancelAsync(RelayRequest request)
    {
        var body = RequireObject(request.Body);
        var runId = RequiredString(body, "run_id");
        var adapterSessionId = RequiredString(body, "adapter_session_id");
        var invocationId = OptionalString(body, "invocation_id");
        var scope = ResolveScope(request, null);
        var status = await sessions.CancelAsync(adapterSessionId, invocationId, scope.WorkspaceId)
            .ConfigureAwait(false);
        return RelayHandlerResult.Ok(JsonSerializer.SerializeToElement(new
        {
            run_id = runId,
            adapter_session_id = adapterSessionId,
            invocation_id = invocationId ?? string.Empty,
            status,
        }, JsonOptions));
    }

    private Scope ResolveScope(RelayRequest request, IReadOnlySet<string>? permissions)
    {
        var workspaceId = request.WorkspaceId.Trim();
        var rawProjectPath = request.Header("x-local-connector-project-root")
            ?? request.Header("x-local-connector-cwd");
        if (workspaceId.Length == 0)
        {
            if (rawProjectPath is not null)
            {
                throw new PluginRuntimeException("Device-level Plugin requests cannot include a project path.");
            }

            if (permissions is not null && permissions.Any(permission =>
                    permission.StartsWith("workspace.", StringComparison.OrdinalIgnoreCase)))
            {
                throw new PluginRuntimeException("Device-level Plugin requests cannot request workspace permissions.");
            }

            return new Scope(null, null);
        }

        var workspace = runtime.Find(workspaceId)
            ?? throw new PluginRuntimeException("Plugin workspace is not registered on this device.");
        if (rawProjectPath is null)
        {
            return new Scope(workspace.Id, workspace.AbsoluteRoot);
        }

        var resolved = projectPaths.Resolve(rawProjectPath);
        if (!string.Equals(resolved.Workspace.Id, workspace.Id, StringComparison.Ordinal))
        {
            throw new PluginRuntimeException("Plugin project path does not match the Relay workspace.");
        }

        return new Scope(workspace.Id, resolved.AbsolutePath);
    }

    private static JsonElement RequireObject(JsonElement value) =>
        value.ValueKind == JsonValueKind.Object
            ? value
            : throw new PluginRuntimeException("Plugin Relay body must be an object.");

    private static string RequiredString(JsonElement body, string property)
    {
        var value = OptionalString(body, property);
        return value ?? throw new PluginRuntimeException($"Plugin Relay is missing {property}.");
    }

    private static string? OptionalString(JsonElement body, string property) =>
        body.TryGetProperty(property, out var value) && value.ValueKind == JsonValueKind.String
            ? value.GetString()?.Trim() is { Length: > 0 } text ? text : null
            : null;

    private static IReadOnlyList<string> StringArray(JsonElement body, string property, bool required)
    {
        if (!body.TryGetProperty(property, out var value))
        {
            return required
                ? throw new PluginRuntimeException($"Plugin Relay is missing {property}.")
                : Array.Empty<string>();
        }

        if (value.ValueKind != JsonValueKind.Array)
        {
            throw new PluginRuntimeException($"Plugin Relay {property} must be an array.");
        }

        var result = new List<string>();
        foreach (var item in value.EnumerateArray())
        {
            var text = item.ValueKind == JsonValueKind.String ? item.GetString()?.Trim() : null;
            if (string.IsNullOrEmpty(text))
            {
                throw new PluginRuntimeException($"Plugin Relay {property} contains an invalid value.");
            }

            result.Add(text);
        }

        return result;
    }

    private static string CanonicalHash(JsonElement value) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(CanonicalJson.Serialize(value))))
            .ToLowerInvariant();

    private static string SafeArgumentSummary(string toolName, JsonElement arguments)
    {
        var keys = arguments.ValueKind == JsonValueKind.Object
            ? string.Join(", ", arguments.EnumerateObject().Select(property => property.Name).Order())
            : "arguments";
        var digest = CanonicalHash(arguments)[..12];
        return $"{toolName}; fields: {keys}; digest: {digest}";
    }

    private static void TryDeleteDirectory(string path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        try
        {
            if (Directory.Exists(path))
            {
                Directory.Delete(path, recursive: true);
            }
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    private sealed record Scope(string? WorkspaceId, string? ProjectRoot);

    private sealed record ToolPolicy(
        string ApprovalMode,
        ConnectorApprovalRiskLevel RiskLevel,
        TimeSpan Timeout,
        IReadOnlySet<string> BasePermissions,
        IReadOnlyList<PermissionRule> Rules)
    {
        public static ToolPolicy Parse(JsonElement tool, string componentKey, string toolName)
        {
            var metadata = tool.TryGetProperty("_meta", out var meta) && meta.ValueKind == JsonValueKind.Object
                ? meta
                : default;
            var approval = ReadString(metadata, "chatos/approvalMode") == "per_call" ? "per_call" : "none";
            var risk = ReadString(metadata, "chatos/riskLevel") switch
            {
                "medium" => ConnectorApprovalRiskLevel.Medium,
                "high" or "critical" => ConnectorApprovalRiskLevel.High,
                _ => ConnectorApprovalRiskLevel.Low,
            };
            var declaredTimeout = ReadInt(metadata, "chatos/timeoutMs") ?? 7_200_000;
            var bounded = Math.Clamp(declaredTimeout, 300, 7_200_000);
            if (bounded < 7_200_000)
            {
                bounded = Math.Min(7_200_000, bounded + Math.Min(10_000, Math.Max(2_000, bounded / 2)));
            }

            var permissions = ReadStrings(metadata, "chatos/requiredPermissions").ToHashSet(StringComparer.Ordinal);
            var rules = new List<PermissionRule>();
            if (metadata.ValueKind == JsonValueKind.Object &&
                metadata.TryGetProperty("chatos/permissionRules", out var ruleValues) &&
                ruleValues.ValueKind == JsonValueKind.Array)
            {
                foreach (var rule in ruleValues.EnumerateArray())
                {
                    if (rule.ValueKind != JsonValueKind.Object ||
                        ReadString(rule, "argumentPointer") is not { } pointer)
                    {
                        continue;
                    }

                    rules.Add(new PermissionRule(
                        pointer,
                        rule.TryGetProperty("equals", out var expected)
                            ? expected.Clone()
                            : JsonSerializer.SerializeToElement<object?>(null),
                        rule.TryGetProperty("matchWhenMissing", out var missing) && missing.ValueKind == JsonValueKind.True,
                        ReadStrings(rule, "requiredPermissions").ToHashSet(StringComparer.Ordinal)));
                }
            }

            _ = componentKey;
            _ = toolName;
            return new ToolPolicy(approval, risk, TimeSpan.FromMilliseconds(bounded), permissions, rules);
        }

        public IReadOnlySet<string> RequiredPermissions(JsonElement arguments)
        {
            var result = BasePermissions.ToHashSet(StringComparer.Ordinal);
            foreach (var rule in Rules)
            {
                var value = JsonPointer(arguments, rule.Pointer);
                if ((value is null && rule.MatchWhenMissing) ||
                    (value is not null && CanonicalJson.Serialize(value.Value) == CanonicalJson.Serialize(rule.Expected)))
                {
                    result.UnionWith(rule.Permissions);
                }
            }

            return result;
        }

        private static string? ReadString(JsonElement value, string property) =>
            value.ValueKind == JsonValueKind.Object &&
            value.TryGetProperty(property, out var child) &&
            child.ValueKind == JsonValueKind.String
                ? child.GetString()
                : null;

        private static int? ReadInt(JsonElement value, string property) =>
            value.ValueKind == JsonValueKind.Object &&
            value.TryGetProperty(property, out var child) &&
            child.TryGetInt32(out var number)
                ? number
                : null;

        private static IReadOnlyList<string> ReadStrings(JsonElement value, string property) =>
            value.ValueKind == JsonValueKind.Object &&
            value.TryGetProperty(property, out var child) &&
            child.ValueKind == JsonValueKind.Array
                ? child.EnumerateArray()
                    .Where(item => item.ValueKind == JsonValueKind.String)
                    .Select(item => item.GetString())
                    .Where(item => !string.IsNullOrWhiteSpace(item))
                    .Select(item => item!.Trim())
                    .ToArray()
                : Array.Empty<string>();

        private static JsonElement? JsonPointer(JsonElement root, string pointer)
        {
            if (!pointer.StartsWith("/", StringComparison.Ordinal))
            {
                return null;
            }

            var current = root;
            foreach (var raw in pointer[1..].Split('/'))
            {
                var key = raw.Replace("~1", "/", StringComparison.Ordinal)
                    .Replace("~0", "~", StringComparison.Ordinal);
                if (current.ValueKind != JsonValueKind.Object || !current.TryGetProperty(key, out current))
                {
                    return null;
                }
            }

            return current.Clone();
        }
    }

    private sealed record PermissionRule(
        string Pointer,
        JsonElement Expected,
        bool MatchWhenMissing,
        IReadOnlySet<string> Permissions);
}
