using System.Net;
using System.Net.Http.Headers;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Gateway;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalAgentExternalMcpTool(
    string ResourceId,
    string ServerName,
    string PublicName,
    string UpstreamName,
    string Description,
    JsonElement InputSchema,
    JsonElement? RemoteSkillGate,
    Uri Endpoint,
    IReadOnlyDictionary<string, string> Headers)
{
    public JsonElement ModelTool() => JsonSerializer.SerializeToElement(
        new Dictionary<string, object?>
        {
            ["type"] = "function",
            ["name"] = PublicName,
            ["description"] = Description,
            ["parameters"] = InputSchema,
            // Removed by the Rust planner before the schema reaches the model.
            ["x-chatos-external-mcp-id"] = ResourceId,
        });
}

public sealed class WindowsLocalAgentExternalMcpExecutor(
    ILocalAgentHostClient host,
    IHttpClientFactory httpClientFactory)
{
    internal const string HttpClientName = "ChatOS.LocalAgentExternalMcp";
    private const int MaximumResponseBytes = 16 * 1024 * 1024;
    private static readonly HashSet<string> ForbiddenHeaders = new(StringComparer.OrdinalIgnoreCase)
    {
        "accept", "connection", "content-length", "content-type", "host",
        "proxy-authenticate", "proxy-authorization", "te", "trailer",
        "transfer-encoding", "upgrade",
    };
    private readonly object _gate = new();
    private IReadOnlyDictionary<string, WindowsLocalAgentExternalMcpTool> _routes =
        new Dictionary<string, WindowsLocalAgentExternalMcpTool>(StringComparer.Ordinal);

    public IReadOnlyList<WindowsLocalAgentExternalMcpTool> Configure(
        IReadOnlyList<ConnectorResolvedMcp> mcps,
        IReadOnlySet<string> selectableIds)
    {
        var routes = new Dictionary<string, WindowsLocalAgentExternalMcpTool>(StringComparer.Ordinal);
        var usedNames = new HashSet<string>(StringComparer.Ordinal);
        foreach (var mcp in mcps.OrderBy(value => value.Resource.Id, StringComparer.Ordinal))
        {
            if (!selectableIds.Contains(mcp.Resource.Id) ||
                !Uri.TryCreate(mcp.Resource.Runtime.Url?.Trim(), UriKind.Absolute, out var endpoint))
                continue;
            ValidateEndpoint(endpoint);
            ValidateHeaders(mcp.Resource.Runtime.Headers);
            var server = NonEmpty(mcp.Resource.Runtime.ServerName) ??
                NonEmpty(mcp.Resource.Name) ?? mcp.Resource.Id;
            foreach (var snapshot in mcp.ToolSnapshot)
            {
                if (snapshot.ValueKind != JsonValueKind.Object ||
                    !snapshot.TryGetProperty("name", out var nameValue) ||
                    nameValue.ValueKind != JsonValueKind.String ||
                    NonEmpty(nameValue.GetString()) is not { } upstreamName)
                    continue;
                var description = snapshot.TryGetProperty("description", out var descriptionValue) &&
                    descriptionValue.ValueKind == JsonValueKind.String
                    ? descriptionValue.GetString() ?? string.Empty
                    : string.Empty;
                var schema = snapshot.TryGetProperty("inputSchema", out var inputSchema)
                    ? inputSchema.Clone()
                    : snapshot.TryGetProperty("input_schema", out var snakeSchema)
                        ? snakeSchema.Clone()
                        : JsonSerializer.SerializeToElement(new { type = "object" });
                var publicName = DisambiguatedName(
                    server, upstreamName, mcp.Resource.Id, usedNames);
                routes.Add(publicName, new(
                    mcp.Resource.Id,
                    server,
                    publicName,
                    upstreamName,
                    description,
                    schema,
                    RemoteSkillGate(snapshot),
                    endpoint,
                    mcp.Resource.Runtime.Headers));
            }
            var configured = routes.Values.Where(value => value.ResourceId == mcp.Resource.Id).ToArray();
            if (configured.Length > 200 ||
                configured.Select(value => value.UpstreamName).Distinct(StringComparer.Ordinal).Count()
                    != configured.Length)
            {
                throw new InvalidOperationException("The external MCP runtime configuration is invalid.");
            }
        }
        lock (_gate) _routes = routes;
        return routes.Values.OrderBy(value => value.PublicName, StringComparer.Ordinal).ToArray();
    }

    public void Reset()
    {
        lock (_gate)
            _routes = new Dictionary<string, WindowsLocalAgentExternalMcpTool>(StringComparer.Ordinal);
    }

    public IReadOnlySet<string> ToolNames()
    {
        lock (_gate) return _routes.Keys.ToHashSet(StringComparer.Ordinal);
    }

    internal async Task<JsonElement> ExecuteAsync(
        string ownerUserId,
        WindowsLocalToolInvocation invocation,
        CancellationToken cancellationToken)
    {
        WindowsLocalAgentExternalMcpTool route;
        lock (_gate)
        {
            if (!_routes.TryGetValue(invocation.ToolName, out route!))
                return Rejection("external_mcp_runtime_unavailable");
        }
        var runResult = await host.SendAsync<GetLocalRunCommand, GetLocalRunResult>(
            new("get_run", ownerUserId, invocation.RunId), cancellationToken).ConfigureAwait(false);
        if (runResult.Type != "run" || runResult.Run.OwnerUserId != ownerUserId ||
            !SelectedExternalMcpIds(runResult.Run.Input).Contains(route.ResourceId))
            throw new InvalidOperationException("This Task did not select the external MCP configuration.");

        // Gateway snapshots do not yet carry trusted server-owned business Skill documents.
        // A declared remote gate must therefore fail closed; the generic Product Skill cannot
        // be treated as a substitute for arbitrary remote instructions.
        if (route.RemoteSkillGate is not null)
            return Rejection("external_mcp_skill_instructions_unavailable");

        var bundleResult = await host.SendAsync<GetRunSkillBundleCommand, RunSkillBundleResult>(
            new("get_run_skill_bundle", ownerUserId, invocation.RunId), cancellationToken)
            .ConfigureAwait(false);
        if (bundleResult.Type != "run_skill_bundle" || bundleResult.RunId != invocation.RunId)
            throw new InvalidDataException("Local Agent Host returned an invalid Skill bundle result.");
        if (bundleResult.Bundle is { ValueKind: JsonValueKind.Object } bundle &&
            bundle.TryGetProperty("schema_version", out var schemaVersion) &&
            schemaVersion.TryGetInt32(out var version) && version >= 2)
        {
            if (!TryFindBinding(bundle, route, invocation.ToolName, out var binding) ||
                String(binding, "binding_id") !=
                    WindowsLocalAgentExternalMcpSkillBundle.BindingId(
                        RoutesForResource(route.ResourceId)))
            {
                return Rejection("external_mcp_snapshot_changed");
            }
            var activationResult = await host.SendAsync<
                GetRunSkillActivationsCommand, RunSkillActivationsResult>(
                new("get_run_skill_activations", ownerUserId, invocation.RunId), cancellationToken)
                .ConfigureAwait(false);
            if (activationResult.Type != "run_skill_activations" ||
                activationResult.RunId != invocation.RunId)
            {
                throw new InvalidDataException("Local Agent Host returned an invalid Skill activation result.");
            }
            var activated = activationResult.SkillRefs.ToHashSet(StringComparer.Ordinal);
            var required = new[]
                {
                    String(binding, "router_skill_ref"),
                    String(binding, "specialist_skill_ref"),
                }
                .Where(value => !string.IsNullOrWhiteSpace(value) && !activated.Contains(value!))
                .Select(value => value!)
                .Distinct(StringComparer.Ordinal)
                .Order(StringComparer.Ordinal)
                .ToArray();
            if (required.Length > 0)
            {
                return JsonSerializer.SerializeToElement(new
                {
                    is_error = true,
                    content = new
                    {
                        error = "skill_activation_required",
                        required_skill_refs = required,
                    },
                });
            }
        }

        // The schema is part of the frozen gateway tool snapshot. Reject invalid or
        // unsupported input before creating an HTTP request.
        WindowsExternalToolSchemaValidator.Validate(invocation.Arguments, route.InputSchema);

        using var request = new HttpRequestMessage(HttpMethod.Post, route.Endpoint);
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        foreach (var pair in route.Headers)
        {
            if (!request.Headers.TryAddWithoutValidation(pair.Key, pair.Value))
                request.Content?.Headers.TryAddWithoutValidation(pair.Key, pair.Value);
        }
        var body = JsonSerializer.SerializeToUtf8Bytes(new
        {
            jsonrpc = "2.0",
            id = string.IsNullOrWhiteSpace(invocation.CallId) ? Guid.NewGuid().ToString("D") : invocation.CallId,
            method = "tools/call",
            @params = new { name = route.UpstreamName, arguments = invocation.Arguments },
        });
        request.Content = new ByteArrayContent(body);
        request.Content.Headers.ContentType = new MediaTypeHeaderValue("application/json");
        // Content headers must be added after the content object exists.
        foreach (var pair in route.Headers)
        {
            if (!request.Headers.Contains(pair.Key))
                request.Content.Headers.TryAddWithoutValidation(pair.Key, pair.Value);
        }
        using var response = await httpClientFactory.CreateClient(HttpClientName)
            .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
            throw new InvalidOperationException("The external MCP request failed.");
        if (response.Content.Headers.ContentLength is > MaximumResponseBytes)
            throw new InvalidOperationException("The external MCP response exceeds the local limit.");
        await using var stream = await response.Content.ReadAsStreamAsync(cancellationToken)
            .ConfigureAwait(false);
        using var buffer = new MemoryStream();
        var chunk = new byte[64 * 1024];
        while (true)
        {
            var read = await stream.ReadAsync(chunk, cancellationToken).ConfigureAwait(false);
            if (read == 0) break;
            if (buffer.Length + read > MaximumResponseBytes)
                throw new InvalidOperationException("The external MCP response exceeds the local limit.");
            buffer.Write(chunk, 0, read);
        }
        using var document = JsonDocument.Parse(buffer.ToArray());
        if (document.RootElement.ValueKind != JsonValueKind.Object)
            throw new InvalidOperationException("The external MCP returned an invalid response.");
        if (document.RootElement.TryGetProperty("error", out var error) &&
            error.ValueKind is not JsonValueKind.Null and not JsonValueKind.Undefined)
        {
            return JsonSerializer.SerializeToElement(new
            {
                is_error = true,
                content = new { error = "external_mcp_rpc_error", detail = error.Clone() },
            });
        }
        if (!document.RootElement.TryGetProperty("result", out var result))
            throw new InvalidOperationException("The external MCP returned an invalid response.");
        return result.Clone();
    }

    private IReadOnlyList<WindowsLocalAgentExternalMcpTool> RoutesForResource(string resourceId)
    {
        lock (_gate)
            return _routes.Values.Where(value => value.ResourceId == resourceId)
                .OrderBy(value => value.PublicName, StringComparer.Ordinal).ToArray();
    }

    private static bool TryFindBinding(
        JsonElement bundle,
        WindowsLocalAgentExternalMcpTool route,
        string toolName,
        out JsonElement binding)
    {
        binding = default;
        if (!bundle.TryGetProperty("tool_skill_bindings", out var bindings) ||
            bindings.ValueKind != JsonValueKind.Array) return false;
        var providerId = WindowsLocalAgentExternalMcpSkillBundle.ProviderId(route.ResourceId);
        foreach (var candidate in bindings.EnumerateArray())
        {
            if (candidate.ValueKind != JsonValueKind.Object ||
                String(candidate, "provider_id") != providerId ||
                !candidate.TryGetProperty("tool_names", out var toolNames) ||
                toolNames.ValueKind != JsonValueKind.Array ||
                !toolNames.EnumerateArray().Any(value =>
                    value.ValueKind == JsonValueKind.String && value.GetString() == toolName))
                continue;
            binding = candidate.Clone();
            return true;
        }
        return false;
    }

    private static JsonElement Rejection(string code) => JsonSerializer.SerializeToElement(new
    {
        is_error = true,
        content = new { error = code },
    });

    private static IReadOnlySet<string> SelectedExternalMcpIds(JsonElement input)
    {
        if (input.ValueKind != JsonValueKind.Object ||
            !input.TryGetProperty("tool_options", out var options) ||
            options.ValueKind != JsonValueKind.Object ||
            !options.TryGetProperty("external_mcp_config_ids", out var ids) ||
            ids.ValueKind != JsonValueKind.Array) return new HashSet<string>(StringComparer.Ordinal);
        return ids.EnumerateArray()
            .Where(value => value.ValueKind == JsonValueKind.String)
            .Select(value => value.GetString())
            .Where(value => !string.IsNullOrWhiteSpace(value))
            .Select(value => value!.Trim())
            .ToHashSet(StringComparer.Ordinal);
    }

    private static JsonElement? RemoteSkillGate(JsonElement snapshot)
    {
        if (!snapshot.TryGetProperty("_meta", out var metadata) ||
            metadata.ValueKind != JsonValueKind.Object ||
            !metadata.TryGetProperty("chatos/skillGate", out var gate)) return null;
        return gate.Clone();
    }

    private static void ValidateEndpoint(Uri endpoint)
    {
        if (!endpoint.IsAbsoluteUri || !string.IsNullOrEmpty(endpoint.UserInfo) ||
            !string.IsNullOrEmpty(endpoint.Fragment) || string.IsNullOrWhiteSpace(endpoint.Host) ||
            !(endpoint.Scheme.Equals(Uri.UriSchemeHttps, StringComparison.OrdinalIgnoreCase) ||
              endpoint.Scheme.Equals(Uri.UriSchemeHttp, StringComparison.OrdinalIgnoreCase) &&
              IsLoopback(endpoint.Host)))
            throw new InvalidOperationException("The external MCP endpoint is invalid.");
    }

    private static bool IsLoopback(string host) =>
        host.Equals("localhost", StringComparison.OrdinalIgnoreCase) ||
        IPAddress.TryParse(host.Trim('[', ']'), out var address) && IPAddress.IsLoopback(address);

    private static void ValidateHeaders(IReadOnlyDictionary<string, string> headers)
    {
        if (headers.Count > 64 || headers.Sum(pair =>
                Encoding.UTF8.GetByteCount(pair.Key) + Encoding.UTF8.GetByteCount(pair.Value)) > 32 * 1024)
            throw new InvalidOperationException("The external MCP headers are invalid.");
        foreach (var pair in headers)
        {
            if (string.IsNullOrWhiteSpace(pair.Key) ||
                pair.Key.Any(character => !(char.IsAsciiLetterOrDigit(character) || character is '-' or '_')) ||
                pair.Value.Contains('\r') || pair.Value.Contains('\n') ||
                ForbiddenHeaders.Contains(pair.Key.Trim()))
                throw new InvalidOperationException("The external MCP headers are invalid.");
        }
    }

    private static string DisambiguatedName(
        string server, string tool, string resourceId, HashSet<string> used)
    {
        var name = $"external_mcp__{Segment(server, "server")}__{Segment(tool, "tool")}";
        if (name.Length > 64) name = name[..64];
        if (used.Add(name)) return name;
        var digest = Convert.ToHexString(SHA256.HashData(
            Encoding.UTF8.GetBytes($"{resourceId}\0{tool}")))[..8].ToLowerInvariant();
        name = string.Concat(name.AsSpan(0, Math.Min(name.Length, 55)), "_", digest);
        if (!used.Add(name)) throw new InvalidOperationException("The external MCP tool name is duplicated.");
        return name;
    }

    private static string Segment(string value, string fallback)
    {
        var output = new StringBuilder();
        var separator = false;
        foreach (var character in value.Trim())
        {
            if (char.IsAsciiLetterOrDigit(character) || character is '_' or '-')
            {
                output.Append(character);
                separator = false;
            }
            else if (!separator)
            {
                output.Append('_');
                separator = true;
            }
        }
        var normalized = output.ToString().Trim('_');
        return normalized.Length == 0 ? fallback : normalized;
    }

    private static string? NonEmpty(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private static string? String(JsonElement value, string name) =>
        value.TryGetProperty(name, out var property) && property.ValueKind == JsonValueKind.String
            ? property.GetString()
            : null;

    private sealed record GetRunSkillBundleCommand(string Type, string OwnerUserId, string RunId);
    private sealed record RunSkillBundleResult(string Type, string RunId, JsonElement? Bundle);
    private sealed record GetRunSkillActivationsCommand(string Type, string OwnerUserId, string RunId);
    private sealed record RunSkillActivationsResult(
        string Type,
        string RunId,
        IReadOnlyList<string> SkillRefs);
}
