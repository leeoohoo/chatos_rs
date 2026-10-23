import Foundation

/// A narrowly scoped, pipe-delivered model capability for one Agent Run. The configuration is
/// never persisted in checkpoints or passed through argv/environment; it only crosses the
/// inherited anonymous pipe after the parent has revalidated the account session.
public enum AgentWorkerModelTransportConfiguration: Codable, Equatable, Sendable {
    case responses(Responses)

    public struct Responses: Codable, Equatable, Sendable {
        public let baseURL: URL
        public let model: String
        public let apiKey: String
        public let thinking: String?
        public let maximumOutputTokens: Int?
        public let temperature: Double?
        public let promptCacheKey: String

        public init(
            baseURL: URL,
            model: String,
            apiKey: String,
            thinking: String?,
            maximumOutputTokens: Int?,
            temperature: Double?,
            promptCacheKey: String
        ) throws {
            guard ["http", "https"].contains(baseURL.scheme?.lowercased() ?? ""),
                  baseURL.host != nil,
                  baseURL.user == nil,
                  baseURL.password == nil,
                  !model.isEmpty,
                  !apiKey.isEmpty,
                  !promptCacheKey.isEmpty else {
                throw AgentRuntimeError.invalidResponse
            }
            self.baseURL = baseURL
            self.model = model
            self.apiKey = apiKey
            self.thinking = thinking
            self.maximumOutputTokens = maximumOutputTokens
            self.temperature = temperature
            self.promptCacheKey = promptCacheKey
        }
    }

    public var usesServerSideCompaction: Bool {
        switch self {
        case .responses: return true
        }
    }

    public func makeModelClient() throws -> any AgentModelClient {
        switch self {
        case let .responses(configuration):
            try AgentResponsesModelClient(
                baseURL: configuration.baseURL,
                model: configuration.model,
                apiKey: configuration.apiKey,
                thinking: configuration.thinking,
                maximumOutputTokens: configuration.maximumOutputTokens,
                temperature: configuration.temperature,
                promptCacheKey: configuration.promptCacheKey
            )
        }
    }
}
