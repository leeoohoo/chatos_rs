using System.Text.Json;
using System.Text.Json.Serialization;

namespace ChatOS.Connector.Gateway;

public sealed partial class ConnectorGatewayHttpClient
{
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
        [JsonPropertyName("mcps")]
        public IReadOnlyList<GatewayResolvedMcpDto> Mcps { get; init; } = [];
        [JsonPropertyName("plugins")]
        public IReadOnlyList<GatewayResolvedPluginDto> Plugins { get; init; } = [];
    }

    private sealed record GatewayResolvedMcpDto
    {
        [JsonPropertyName("resource")]
        public required GatewayMcpResourceDto Resource { get; init; }
        [JsonPropertyName("binding")]
        public required GatewayCapabilityBindingDto Binding { get; init; }
        [JsonPropertyName("available")]
        public bool Available { get; init; }
        [JsonPropertyName("status")]
        public required string Status { get; init; }
        [JsonPropertyName("tool_snapshot")]
        public IReadOnlyList<JsonElement> ToolSnapshot { get; init; } = [];
    }

    private sealed record GatewayMcpResourceDto
    {
        [JsonPropertyName("id")]
        public required string Id { get; init; }
        [JsonPropertyName("name")]
        public required string Name { get; init; }
        [JsonPropertyName("display_name")]
        public required string DisplayName { get; init; }
        [JsonPropertyName("description")]
        public string? Description { get; init; }
        [JsonPropertyName("enabled")]
        public bool Enabled { get; init; }
        [JsonPropertyName("runtime")]
        public required GatewayMcpRuntimeDto Runtime { get; init; }
    }

    private sealed record GatewayMcpRuntimeDto
    {
        [JsonPropertyName("kind")]
        public required string Kind { get; init; }
        [JsonPropertyName("builtin_kind")]
        public string? BuiltinKind { get; init; }
        [JsonPropertyName("server_name")]
        public string? ServerName { get; init; }
        [JsonPropertyName("url")]
        public string? Url { get; init; }
        [JsonPropertyName("headers")]
        public IReadOnlyDictionary<string, string> Headers { get; init; } =
            new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
    }

    private sealed record GatewayCapabilityBindingDto
    {
        [JsonPropertyName("enabled")]
        public bool Enabled { get; init; }
        [JsonPropertyName("required")]
        public bool Required { get; init; }
    }

    private sealed record GatewayResolvedPluginDto
    {
        [JsonPropertyName("catalog")]
        public required GatewayPluginCapabilityCatalogDto Catalog { get; init; }
        [JsonPropertyName("binding")]
        public required GatewayCapabilityBindingDto Binding { get; init; }
        [JsonPropertyName("available")]
        public bool Available { get; init; }
        [JsonPropertyName("status")]
        public required string Status { get; init; }
    }

    private sealed record GatewayPluginCapabilityCatalogDto
    {
        [JsonPropertyName("id")]
        public required string Id { get; init; }
        [JsonPropertyName("plugin_key")]
        public required string PluginKey { get; init; }
        [JsonPropertyName("display_name")]
        public required string DisplayName { get; init; }
        [JsonPropertyName("description")]
        public required string Description { get; init; }
    }
}
