using System.Net;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.Relay;

namespace ChatOS.Connector.Gateway;

public sealed class ConnectorGatewayHttpClient : IConnectorGatewayClient
{
    internal const string HttpClientName = "ChatOS.LocalConnectorGateway";
    internal const string ClientVersion = "1.0.0-windows";
    internal const long MaximumPluginArtifactBytes = 256L * 1024 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    private readonly IHttpClientFactory _httpClientFactory;

    public ConnectorGatewayHttpClient(IHttpClientFactory httpClientFactory)
    {
        _httpClientFactory = httpClientFactory;
    }

    public async Task<ConnectorGatewayLogin> ExchangeTicketAsync(
        Uri gatewayBaseUri,
        string ticket,
        string deviceName,
        CancellationToken cancellationToken = default)
    {
        var response = await SendAsync<GatewayLoginDto>(
            gatewayBaseUri,
            HttpMethod.Post,
            "/api/auth/local-connector-ticket/exchange",
            null,
            new GatewayTicketExchangeRequest(ticket, deviceName, ClientVersion),
            cancellationToken).ConfigureAwait(false);
        return new ConnectorGatewayLogin(
            response.Token,
            new ConnectorGatewayUser(
                response.User.Id,
                response.User.Username,
                response.User.DisplayName,
                response.User.Role ?? "user"));
    }

    public async Task<ConnectorGatewayDevice> CreateDeviceAsync(
        Uri gatewayBaseUri,
        string token,
        string displayName,
        string publicKey,
        CancellationToken cancellationToken = default) =>
        Map(await SendAsync<GatewayDeviceDto>(
            gatewayBaseUri,
            HttpMethod.Post,
            "/api/local-connectors/devices",
            token,
            new GatewayCreateDeviceRequest(displayName, publicKey, ClientVersion, "Windows"),
            cancellationToken).ConfigureAwait(false));

    public async Task<ConnectorGatewayDevice?> GetDeviceAsync(
        Uri gatewayBaseUri,
        string token,
        string deviceId,
        CancellationToken cancellationToken = default)
    {
        try
        {
            return Map(await SendAsync<GatewayDeviceDto>(
                gatewayBaseUri,
                HttpMethod.Get,
                $"/api/local-connectors/devices/{Uri.EscapeDataString(deviceId)}",
                token,
                null,
                cancellationToken).ConfigureAwait(false));
        }
        catch (ConnectorGatewayException exception) when (exception.StatusCode is HttpStatusCode.NotFound)
        {
            return null;
        }
    }

    public async Task DisconnectDeviceAsync(
        Uri gatewayBaseUri,
        string token,
        string deviceId,
        CancellationToken cancellationToken = default)
    {
        try
        {
            await SendWithoutResponseAsync(
                gatewayBaseUri,
                HttpMethod.Post,
                $"/api/local-connectors/devices/{Uri.EscapeDataString(deviceId)}/disconnect",
                token,
                new { },
                cancellationToken).ConfigureAwait(false);
        }
        catch (ConnectorGatewayException exception) when (exception.StatusCode is HttpStatusCode.NotFound)
        {
        }
    }

    public async Task<ConnectorControlledNetworkReadiness> GetControlledNetworkReadinessAsync(
        Uri gatewayBaseUri,
        string token,
        string deviceId,
        CancellationToken cancellationToken = default)
    {
        var response = await SendAsync<GatewayControlledNetworkReadinessDto>(
            gatewayBaseUri,
            HttpMethod.Get,
            $"/api/local-connectors/devices/{Uri.EscapeDataString(deviceId)}/controlled-network/readiness",
            token,
            null,
            cancellationToken).ConfigureAwait(false);
        return new ConnectorControlledNetworkReadiness(
            response.Available,
            response.State,
            response.PermissionProfile,
            response.AllowedHostCount);
    }

    private async Task SendWithoutResponseAsync(
        Uri gatewayBaseUri,
        HttpMethod method,
        string path,
        string? token,
        object? body,
        CancellationToken cancellationToken)
    {
        using var request = BuildRequest(gatewayBaseUri, method, path, token, body);
        using var response = await _httpClientFactory
            .CreateClient(HttpClientName)
            .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            var payload = await response.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false);
            throw new ConnectorGatewayException(response.StatusCode, ErrorMessage(payload, response.StatusCode));
        }
    }

    public async Task<IReadOnlyList<ConnectorGatewayWorkspace>> ListWorkspacesAsync(
        Uri gatewayBaseUri,
        string token,
        CancellationToken cancellationToken = default) =>
        (await SendAsync<IReadOnlyList<GatewayWorkspaceDto>>(
            gatewayBaseUri,
            HttpMethod.Get,
            "/api/local-connectors/workspaces",
            token,
            null,
            cancellationToken).ConfigureAwait(false)).Select(Map).ToArray();

    public async Task<ConnectorGatewayWorkspace> CreateWorkspaceAsync(
        Uri gatewayBaseUri,
        string token,
        string deviceId,
        string alias,
        string fingerprint,
        CancellationToken cancellationToken = default) =>
        Map(await SendAsync<GatewayWorkspaceDto>(
            gatewayBaseUri,
            HttpMethod.Post,
            "/api/local-connectors/workspaces",
            token,
            new GatewayCreateWorkspaceRequest(
                deviceId,
                alias,
                alias,
                fingerprint,
                ["mcp", "terminal", "sandbox"]),
            cancellationToken).ConfigureAwait(false));

    public async Task<ConnectorGatewayWorkspace> MoveWorkspaceAsync(
        Uri gatewayBaseUri,
        string token,
        string workspaceId,
        string deviceId,
        CancellationToken cancellationToken = default) =>
        Map(await SendAsync<GatewayWorkspaceDto>(
            gatewayBaseUri,
            HttpMethod.Put,
            $"/api/local-connectors/workspaces/{Uri.EscapeDataString(workspaceId)}",
            token,
            new GatewayMoveWorkspaceRequest(deviceId, "active"),
            cancellationToken).ConfigureAwait(false));

    public async Task<RemoteControlTrust> GetRemoteControlTrustAsync(
        Uri gatewayBaseUri,
        string token,
        CancellationToken cancellationToken = default)
    {
        var response = await SendAsync<GatewayManagedRuntimeDto>(
            gatewayBaseUri,
            HttpMethod.Get,
            "/api/local-connectors/config/runtime",
            token,
            null,
            cancellationToken).ConfigureAwait(false);
        return new RemoteControlTrust(
            response.RemoteControlTrust.RequireSignedMessages,
            response.RemoteControlTrust.SignatureMaxSkewSeconds,
            response.RemoteControlTrust.TrustedRelayPublicKeys);
    }

    public async Task<IReadOnlyList<ConnectorPluginSource>> ListPluginSourcesAsync(
        Uri gatewayBaseUri,
        string token,
        CancellationToken cancellationToken = default) =>
        (await SendAsync<GatewayPluginSourceListDto>(
            gatewayBaseUri,
            HttpMethod.Get,
            "/api/plugin-management/plugins/install-sources",
            token,
            null,
            cancellationToken).ConfigureAwait(false)).Items.Select(Map).ToArray();

    public async Task UpdatePluginPreferenceAsync(
        Uri gatewayBaseUri,
        string token,
        string pluginId,
        string deviceId,
        bool enabled,
        CancellationToken cancellationToken = default) =>
        await SendWithoutResponseAsync(
            gatewayBaseUri,
            HttpMethod.Put,
            $"/api/plugin-management/plugins/{Uri.EscapeDataString(pluginId)}/preference",
            token,
            new GatewayPluginPreferenceRequest(deviceId, enabled),
            cancellationToken).ConfigureAwait(false);

    public async Task DownloadPluginArtifactAsync(
        Uri gatewayBaseUri,
        string token,
        string pluginId,
        string releaseId,
        Stream destination,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(destination);
        if (!destination.CanWrite)
        {
            throw new ArgumentException("Plugin artifact destination must be writable.", nameof(destination));
        }

        var path = $"/api/plugin-management/plugins/{Uri.EscapeDataString(pluginId)}/releases/{Uri.EscapeDataString(releaseId)}/artifact";
        using var request = BuildRequest(gatewayBaseUri, HttpMethod.Get, path, token, null);
        request.Headers.Accept.Clear();
        request.Headers.Accept.ParseAdd("application/gzip, application/octet-stream");
        using var response = await _httpClientFactory
            .CreateClient(HttpClientName)
            .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            var payload = await response.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false);
            throw new ConnectorGatewayException(response.StatusCode, ErrorMessage(payload, response.StatusCode));
        }

        if (response.Content.Headers.ContentLength is > MaximumPluginArtifactBytes)
        {
            throw new ConnectorGatewayException(
                response.StatusCode,
                "Plugin artifact exceeds the 256 MB download limit.");
        }

        await using var source = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        var buffer = new byte[64 * 1024];
        long total = 0;
        while (true)
        {
            var read = await source.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (read == 0)
            {
                break;
            }

            total += read;
            if (total > MaximumPluginArtifactBytes)
            {
                throw new ConnectorGatewayException(
                    response.StatusCode,
                    "Plugin artifact exceeds the 256 MB download limit.");
            }

            await destination.WriteAsync(buffer.AsMemory(0, read), cancellationToken).ConfigureAwait(false);
        }
    }

    public async Task<IReadOnlyList<ConnectorGatewayModelConfig>> ListModelConfigsAsync(
        Uri gatewayBaseUri,
        string token,
        CancellationToken cancellationToken = default) =>
        (await SendAsync<IReadOnlyList<GatewayModelConfigDto>>(
            gatewayBaseUri,
            HttpMethod.Get,
            "/api/model-configs",
            token,
            null,
            cancellationToken).ConfigureAwait(false)).Select(Map).ToArray();

    public async Task<ConnectorGatewayModelConfig> GetModelConfigAsync(
        Uri gatewayBaseUri,
        string token,
        string modelConfigId,
        bool includeSecret,
        CancellationToken cancellationToken = default) =>
        Map(await SendAsync<GatewayModelConfigDto>(
            gatewayBaseUri,
            HttpMethod.Get,
            $"/api/model-configs/{Uri.EscapeDataString(modelConfigId)}?include_secret={includeSecret.ToString().ToLowerInvariant()}",
            token,
            null,
            cancellationToken).ConfigureAwait(false));

    public async Task<ConnectorAgentPromptBundle> GetAgentPromptBundleAsync(
        Uri gatewayBaseUri,
        string token,
        CancellationToken cancellationToken = default)
    {
        var bundle = await SendAsync<GatewayAgentPromptBundleDto>(
            gatewayBaseUri,
            HttpMethod.Get,
            "/api/plugin-management/agent-prompts/bundle",
            token,
            null,
            cancellationToken).ConfigureAwait(false);
        return new ConnectorAgentPromptBundle(
            bundle.BundleVersion,
            bundle.UpdatedAt,
            bundle.Prompts.Select(value => new ConnectorAgentPrompt(
                value.AgentKey,
                value.Vendor,
                value.Content,
                value.Revision,
                value.Checksum,
                value.PublishedAt)).ToArray());
    }

    public async Task<ConnectorAgentCapability> GetAgentCapabilityAsync(
        Uri gatewayBaseUri,
        string token,
        string agentKey,
        CancellationToken cancellationToken = default)
    {
        var capability = await SendAsync<GatewayAgentCapabilityDto>(
            gatewayBaseUri,
            HttpMethod.Get,
            $"/api/plugin-management/agent-capabilities/{Uri.EscapeDataString(agentKey)}",
            token,
            null,
            cancellationToken).ConfigureAwait(false);
        return new ConnectorAgentCapability(
            capability.AgentKey,
            capability.OwnerUserId,
            capability.PolicyRevision,
            capability.AgentEnabled);
    }

    private async Task<T> SendAsync<T>(
        Uri gatewayBaseUri,
        HttpMethod method,
        string path,
        string? token,
        object? body,
        CancellationToken cancellationToken)
    {
        using var request = BuildRequest(gatewayBaseUri, method, path, token, body);

        using var response = await _httpClientFactory
            .CreateClient(HttpClientName)
            .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        var payload = await response.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            throw new ConnectorGatewayException(response.StatusCode, ErrorMessage(payload, response.StatusCode));
        }

        try
        {
            return JsonSerializer.Deserialize<T>(payload, JsonOptions)
                ?? throw new JsonException("Gateway response body is empty.");
        }
        catch (JsonException exception)
        {
            throw new ConnectorGatewayException(
                response.StatusCode,
                "Connector gateway returned an invalid response.",
                exception);
        }
    }

    private static HttpRequestMessage BuildRequest(
        Uri gatewayBaseUri,
        HttpMethod method,
        string path,
        string? token,
        object? body)
    {
        var request = new HttpRequestMessage(method, Endpoint(gatewayBaseUri, path));
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        request.Headers.TryAddWithoutValidation("X-Chatos-Client-Surface", "local-connector-windows");
        request.Headers.TryAddWithoutValidation("X-ChatOS-Client", "windows-native");
        if (!string.IsNullOrWhiteSpace(token))
        {
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", token);
        }

        if (body is not null)
        {
            request.Content = JsonContent.Create(body, body.GetType(), options: JsonOptions);
        }

        return request;
    }

    private static Uri Endpoint(Uri baseUri, string path)
    {
        if (!baseUri.IsAbsoluteUri || baseUri.Scheme is not ("http" or "https"))
        {
            throw new ArgumentException("Connector gateway must be an absolute HTTP(S) URL.", nameof(baseUri));
        }

        return new Uri(baseUri.AbsoluteUri.TrimEnd('/') + path, UriKind.Absolute);
    }

    private static string ErrorMessage(string payload, HttpStatusCode statusCode)
    {
        try
        {
            using var document = JsonDocument.Parse(payload);
            foreach (var property in new[] { "message", "error", "detail" })
            {
                if (document.RootElement.TryGetProperty(property, out var value) &&
                    value.ValueKind is JsonValueKind.String &&
                    !string.IsNullOrWhiteSpace(value.GetString()))
                {
                    return value.GetString()!;
                }
            }
        }
        catch (JsonException)
        {
        }

        return $"Connector gateway request failed with status {(int)statusCode}.";
    }

    private static ConnectorGatewayDevice Map(GatewayDeviceDto value) => new(
        value.Id,
        value.OwnerUserId,
        value.DisplayName,
        value.PublicKey,
        value.Status);

    private static ConnectorGatewayWorkspace Map(GatewayWorkspaceDto value) => new(
        value.Id,
        value.DeviceId,
        value.LocalPathAlias,
        value.LocalPathFingerprint);

    private static ConnectorPluginSource Map(GatewayPluginSourceDto value) => new(
        new ConnectorPluginCatalog(
            value.Catalog.Id,
            value.Catalog.DisplayName,
            value.Catalog.Name,
            value.Catalog.Description,
            value.Catalog.Publisher?.Name,
            value.Catalog.Interface?.Category,
            value.Catalog.Interface?.DeveloperName),
        new ConnectorPluginRelease(
            value.Release.Id,
            value.Release.Version,
            value.Release.ArtifactSha256,
            value.Release.NpmPackage is null
                ? null
                : new ConnectorPluginNpmPackage(
                    value.Release.NpmPackage.Name,
                    value.Release.NpmPackage.Version,
                    value.Release.NpmPackage.Integrity),
            value.Release.SupportedPlatforms ?? Array.Empty<string>()),
        value.Preference is null ? null : new ConnectorPluginPreference(value.Preference.Enabled));

    private static ConnectorGatewayModelConfig Map(GatewayModelConfigDto value) => new(
        value.Id,
        string.IsNullOrWhiteSpace(value.Name) ? value.Model : value.Name,
        value.Provider,
        value.PromptVendor,
        value.Model,
        value.BaseUrl,
        value.ApiKey,
        value.Enabled,
        value.SupportsResponses,
        value.ThinkingLevel,
        value.Temperature,
        value.MaxOutputTokens);

    private sealed record GatewayLoginDto
    {
        [JsonPropertyName("token")]
        public required string Token { get; init; }

        [JsonPropertyName("user")]
        public required GatewayUserDto User { get; init; }
    }

    private sealed record GatewayUserDto
    {
        [JsonPropertyName("id")]
        public required string Id { get; init; }

        [JsonPropertyName("username")]
        public required string Username { get; init; }

        [JsonPropertyName("display_name")]
        public string? DisplayName { get; init; }

        [JsonPropertyName("role")]
        public string? Role { get; init; }
    }

    private sealed record GatewayDeviceDto
    {
        [JsonPropertyName("id")]
        public required string Id { get; init; }

        [JsonPropertyName("owner_user_id")]
        public string? OwnerUserId { get; init; }

        [JsonPropertyName("display_name")]
        public required string DisplayName { get; init; }

        [JsonPropertyName("public_key")]
        public required string PublicKey { get; init; }

        [JsonPropertyName("status")]
        public required string Status { get; init; }
    }

    private sealed record GatewayControlledNetworkReadinessDto
    {
        [JsonPropertyName("available")]
        public bool Available { get; init; }

        [JsonPropertyName("state")]
        public required string State { get; init; }

        [JsonPropertyName("permission_profile")]
        public string? PermissionProfile { get; init; }

        [JsonPropertyName("allowed_host_count")]
        public int AllowedHostCount { get; init; }
    }

    private sealed record GatewayWorkspaceDto
    {
        [JsonPropertyName("id")]
        public required string Id { get; init; }

        [JsonPropertyName("device_id")]
        public required string DeviceId { get; init; }

        [JsonPropertyName("local_path_alias")]
        public required string LocalPathAlias { get; init; }

        [JsonPropertyName("local_path_fingerprint")]
        public required string LocalPathFingerprint { get; init; }
    }

    private sealed record GatewayManagedRuntimeDto
    {
        [JsonPropertyName("remote_control_trust")]
        public required GatewayTrustDto RemoteControlTrust { get; init; }
    }

    private sealed record GatewayTrustDto
    {
        [JsonPropertyName("require_signed_messages")]
        public required bool RequireSignedMessages { get; init; }

        [JsonPropertyName("signature_max_skew_seconds")]
        public required int SignatureMaxSkewSeconds { get; init; }

        [JsonPropertyName("trusted_relay_public_keys")]
        public required IReadOnlyDictionary<string, string> TrustedRelayPublicKeys { get; init; }
    }

    private sealed record GatewayPluginSourceListDto
    {
        [JsonPropertyName("items")]
        public IReadOnlyList<GatewayPluginSourceDto> Items { get; init; } = Array.Empty<GatewayPluginSourceDto>();
    }

    private sealed record GatewayPluginSourceDto
    {
        [JsonPropertyName("catalog")]
        public required GatewayPluginCatalogDto Catalog { get; init; }

        [JsonPropertyName("release")]
        public required GatewayPluginReleaseDto Release { get; init; }

        [JsonPropertyName("preference")]
        public GatewayPluginPreferenceDto? Preference { get; init; }
    }

    private sealed record GatewayPluginCatalogDto
    {
        [JsonPropertyName("id")]
        public required string Id { get; init; }

        [JsonPropertyName("display_name")]
        public string? DisplayName { get; init; }

        [JsonPropertyName("name")]
        public string? Name { get; init; }

        [JsonPropertyName("description")]
        public string? Description { get; init; }

        [JsonPropertyName("publisher")]
        public GatewayPluginPublisherDto? Publisher { get; init; }

        [JsonPropertyName("interface")]
        public GatewayPluginInterfaceDto? Interface { get; init; }
    }

    private sealed record GatewayPluginPublisherDto
    {
        [JsonPropertyName("name")]
        public string? Name { get; init; }
    }

    private sealed record GatewayPluginInterfaceDto
    {
        [JsonPropertyName("category")]
        public string? Category { get; init; }

        [JsonPropertyName("developer_name")]
        public string? DeveloperName { get; init; }
    }

    private sealed record GatewayPluginReleaseDto
    {
        [JsonPropertyName("id")]
        public required string Id { get; init; }

        [JsonPropertyName("version")]
        public string? Version { get; init; }

        [JsonPropertyName("artifact_sha256")]
        public string? ArtifactSha256 { get; init; }

        [JsonPropertyName("npm_package")]
        public GatewayPluginNpmPackageDto? NpmPackage { get; init; }

        [JsonPropertyName("supported_platforms")]
        public IReadOnlyList<string>? SupportedPlatforms { get; init; }
    }

    private sealed record GatewayPluginNpmPackageDto
    {
        [JsonPropertyName("name")]
        public required string Name { get; init; }

        [JsonPropertyName("version")]
        public required string Version { get; init; }

        [JsonPropertyName("integrity")]
        public required string Integrity { get; init; }
    }

    private sealed record GatewayPluginPreferenceDto
    {
        [JsonPropertyName("enabled")]
        public required bool Enabled { get; init; }
    }

    private sealed record GatewayModelConfigDto
    {
        [JsonPropertyName("id")]
        public required string Id { get; init; }
        [JsonPropertyName("name")]
        public string? Name { get; init; }
        [JsonPropertyName("provider")]
        public required string Provider { get; init; }
        [JsonPropertyName("prompt_vendor")]
        public string? PromptVendor { get; init; }
        [JsonPropertyName("model")]
        public required string Model { get; init; }
        [JsonPropertyName("base_url")]
        public string? BaseUrl { get; init; }
        [JsonPropertyName("api_key")]
        public string? ApiKey { get; init; }
        [JsonPropertyName("enabled")]
        public bool Enabled { get; init; } = true;
        [JsonPropertyName("supports_responses")]
        public bool SupportsResponses { get; init; }
        [JsonPropertyName("thinking_level")]
        public string? ThinkingLevel { get; init; }
        [JsonPropertyName("temperature")]
        public double? Temperature { get; init; }
        [JsonPropertyName("max_output_tokens")]
        public int? MaxOutputTokens { get; init; }
    }

    private sealed record GatewayAgentPromptBundleDto
    {
        [JsonPropertyName("bundle_version")]
        public long BundleVersion { get; init; }
        [JsonPropertyName("updated_at")]
        public DateTimeOffset UpdatedAt { get; init; }
        [JsonPropertyName("prompts")]
        public IReadOnlyList<GatewayAgentPromptDto> Prompts { get; init; } = [];
    }

    private sealed record GatewayAgentPromptDto
    {
        [JsonPropertyName("agent_key")]
        public required string AgentKey { get; init; }
        [JsonPropertyName("vendor")]
        public required string Vendor { get; init; }
        [JsonPropertyName("content")]
        public required string Content { get; init; }
        [JsonPropertyName("revision")]
        public long Revision { get; init; }
        [JsonPropertyName("checksum")]
        public required string Checksum { get; init; }
        [JsonPropertyName("published_at")]
        public DateTimeOffset PublishedAt { get; init; }
    }

    private sealed record GatewayAgentCapabilityDto
    {
        [JsonPropertyName("agent_key")]
        public required string AgentKey { get; init; }
        [JsonPropertyName("owner_user_id")]
        public required string OwnerUserId { get; init; }
        [JsonPropertyName("policy_revision")]
        public required string PolicyRevision { get; init; }
        [JsonPropertyName("agent_enabled")]
        public bool AgentEnabled { get; init; } = true;
    }
}

public sealed class ConnectorGatewayException : Exception
{
    public ConnectorGatewayException(
        HttpStatusCode statusCode,
        string message,
        Exception? innerException = null)
        : base(message, innerException)
    {
        StatusCode = statusCode;
    }

    public HttpStatusCode StatusCode { get; }
}
