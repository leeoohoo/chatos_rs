import CryptoKit
import Foundation

enum NativeManagedAgentPromptResolver {
    static let maximumPromptBytes = 256 * 1_024

    static func resolve(
        agentKey: String,
        model: GatewayModelConfigDTO,
        bundle: GatewayAgentPromptBundleDTO
    ) throws -> GatewayAgentPromptDTO {
        guard bundle.bundleVersion > 0 else {
            throw NativeManagedAgentPromptError.invalidBundle
        }
        let vendor = try normalizedVendor(
            explicitVendor: model.promptVendor,
            provider: model.provider
        )
        guard let prompt = bundle.prompts.first(where: {
            $0.agentKey == agentKey
                && $0.vendor.caseInsensitiveCompare(vendor) == .orderedSame
        }), prompt.revision > 0,
              !prompt.content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              prompt.content.lengthOfBytes(using: .utf8) <= maximumPromptBytes else {
            throw NativeManagedAgentPromptError.promptUnavailable(
                agentKey: agentKey,
                vendor: vendor
            )
        }
        let digest = SHA256.hash(data: Data(prompt.content.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        guard prompt.checksum.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            == "sha256:\(digest)" else {
            throw NativeManagedAgentPromptError.invalidChecksum(
                agentKey: agentKey,
                vendor: vendor
            )
        }
        return prompt
    }

    static func normalizedVendor(
        explicitVendor: String?,
        provider: String
    ) throws -> String {
        let explicitVendor = explicitVendor?.trimmingCharacters(in: .whitespacesAndNewlines)
        let candidate: String
        if let explicitVendor, !explicitVendor.isEmpty {
            candidate = explicitVendor.lowercased()
        } else {
            candidate = provider.trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased()
                .replacingOccurrences(of: "-", with: "_")
        }
        switch candidate {
        case "gpt", "openai": return "gpt"
        case "deepseek": return "deepseek"
        case "kimi", "moonshot": return "kimi"
        case "glm", "zhipu", "zai": return "glm"
        default: throw NativeManagedAgentPromptError.unsupportedVendor(candidate)
        }
    }
}

enum NativeManagedAgentPromptError: LocalizedError, Equatable {
    case invalidBundle
    case unsupportedVendor(String)
    case promptUnavailable(agentKey: String, vendor: String)
    case invalidChecksum(agentKey: String, vendor: String)

    var errorDescription: String? {
        switch self {
        case .invalidBundle:
            "The managed Agent Prompt bundle is invalid."
        case .unsupportedVendor(let vendor):
            "The managed Agent Prompt vendor is unsupported: \(vendor)."
        case .promptUnavailable(let agentKey, let vendor):
            "The managed Agent Prompt is unavailable: \(agentKey)@\(vendor)."
        case .invalidChecksum(let agentKey, let vendor):
            "The managed Agent Prompt checksum is invalid: \(agentKey)@\(vendor)."
        }
    }
}
