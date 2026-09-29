import Foundation

struct GatewayManagedRuntimeConfigDTO: Decodable, Sendable {
    var nativeAgentRuntimeSettings: GatewayNativeAgentRuntimeSettingsDTO?
    var remoteControlTrust: GatewayRemoteControlTrustDTO

    enum CodingKeys: String, CodingKey {
        case nativeAgentRuntimeSettings = "native_agent_runtime_settings"
        case remoteControlTrust = "remote_control_trust"
    }
}

struct GatewayNativeAgentRuntimeSettingsDTO: Decodable, Sendable {
    var maximumModelCalls: Int
    var maximumRequestRetries: Int
    var requestTimeoutSeconds: Int
    var runTimeoutSeconds: Int
    var maximumNoProgressRounds: Int
    var contextWindowTokens: Int
    var outputReserveTokens: Int

    enum CodingKeys: String, CodingKey {
        case maximumModelCalls = "maximum_model_calls"
        case maximumRequestRetries = "maximum_request_retries"
        case requestTimeoutSeconds = "request_timeout_seconds"
        case runTimeoutSeconds = "run_timeout_seconds"
        case maximumNoProgressRounds = "maximum_no_progress_rounds"
        case contextWindowTokens = "context_window_tokens"
        case outputReserveTokens = "output_reserve_tokens"
    }
}

struct GatewayRemoteControlTrustDTO: Decodable, Sendable {
    var requireSignedMessages: Bool
    var signatureMaxSkewSeconds: Int
    var trustedRelayPublicKeys: [String: String]

    enum CodingKeys: String, CodingKey {
        case requireSignedMessages = "require_signed_messages"
        case signatureMaxSkewSeconds = "signature_max_skew_seconds"
        case trustedRelayPublicKeys = "trusted_relay_public_keys"
    }
}
