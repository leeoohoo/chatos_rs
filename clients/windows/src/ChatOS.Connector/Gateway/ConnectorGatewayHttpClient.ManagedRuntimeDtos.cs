using System.Text.Json.Serialization;

namespace ChatOS.Connector.Gateway;

public sealed partial class ConnectorGatewayHttpClient
{
    private sealed record GatewayManagedRuntimeDto
    {
        [JsonPropertyName("native_agent_runtime_settings")]
        public GatewayNativeAgentRuntimeSettingsDto? NativeAgentRuntimeSettings { get; init; }

        [JsonPropertyName("local_task_execution_settings")]
        public GatewayLocalTaskExecutionSettingsDto? LocalTaskExecutionSettings { get; init; }

        [JsonPropertyName("remote_control_trust")]
        public required GatewayTrustDto RemoteControlTrust { get; init; }
    }

    private sealed record GatewayLocalTaskExecutionSettingsDto
    {
        [JsonPropertyName("max_iterations")]
        public required int MaxIterations { get; init; }

        public LocalTaskExecutionSettings ToDomain() => new(MaxIterations);
    }

    private sealed record GatewayNativeAgentRuntimeSettingsDto
    {
        [JsonPropertyName("maximum_model_calls")]
        public required int MaximumModelCalls { get; init; }

        [JsonPropertyName("maximum_request_retries")]
        public required int MaximumRequestRetries { get; init; }

        [JsonPropertyName("request_timeout_seconds")]
        public required int RequestTimeoutSeconds { get; init; }

        [JsonPropertyName("run_timeout_seconds")]
        public required int RunTimeoutSeconds { get; init; }

        [JsonPropertyName("maximum_no_progress_rounds")]
        public required int MaximumNoProgressRounds { get; init; }

        [JsonPropertyName("context_window_tokens")]
        public required int ContextWindowTokens { get; init; }

        [JsonPropertyName("output_reserve_tokens")]
        public required int OutputReserveTokens { get; init; }

        public NativeAgentRuntimeSettings ToDomain() => new(
            MaximumModelCalls,
            MaximumRequestRetries,
            RequestTimeoutSeconds,
            RunTimeoutSeconds,
            MaximumNoProgressRounds,
            ContextWindowTokens,
            OutputReserveTokens);
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
}
