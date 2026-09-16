import ChatOSCore
import ChatOSAgentRuntime
import Foundation

/// Uses the user's configured relay and secret; no separate story-model credentials.
public struct ChatOSStoryPlanningService: StoryPlanningServicing, AgentServiceProviding {
    private let client: ChatOSAPIClient
    private let transport: any HTTPTransport
    public init(client: ChatOSAPIClient, transport: any HTTPTransport = URLSessionHTTPTransport()) {
        self.client = client; self.transport = transport
    }

    public func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        try await ChatOSMemoryEngineService(client: client, scope: scope)
    }

    public func makeAgentModel(configID: String, policy: AgentRunPolicy) async throws -> any AgentModelClient {
        try policy.validate()
        let session = try await client.currentAuthenticationSessionID()
        let config: Config = try await client.request("/ai-model-configs/\(configID.urlPathEncoded)?include_secret=true",
                                                     expectedAuthenticationSessionID: session)
        guard config.enabled, let key = config.apiKey, !key.isEmpty,
              let raw = config.baseURL, let url = URL(string: raw.trimmingCharacters(in: .whitespacesAndNewlines)) else { throw StoryError.missingModel }
        let send: AgentResponsesModelClient.Transport = { request in
            guard let url = request.url else { throw ChatOSAPIError.invalidEndpoint }
            let response = try await transport.send(.init(url: url, method: "POST", headers: request.allHTTPHeaderFields ?? [:],
                body: request.httpBody, timeoutInterval: request.timeoutInterval))
            return (response.body, response.statusCode)
        }
        let stream: AgentResponsesModelClient.StreamTransport = { request in
            guard let url = request.url else { throw ChatOSAPIError.invalidEndpoint }
            let response = try await transport.stream(.init(url: url, method: "POST",
                headers: request.allHTTPHeaderFields ?? [:], body: request.httpBody,
                timeoutInterval: request.timeoutInterval))
            return .init(statusCode: response.statusCode, headers: response.headers, body: response.body)
        }
        let model: any AgentModelClient = try AgentResponsesModelClient(
            baseURL: url, model: config.model, apiKey: key,
            maximumOutputTokens: (policy.context ?? .init()).outputReserveTokens,
            promptCacheKey: "story-agent:\(configID)",
            transport: send, streamTransport: stream
        )
        return StorySessionBoundModel(model: model, client: client, session: session)
    }

    public func plan(_ request: StoryPlanningRequest) async throws -> Data {
        let config: Config = try await client.request("/ai-model-configs/\(request.modelConfigID.urlPathEncoded)?include_secret=true")
        guard config.enabled, let key = config.apiKey, !key.isEmpty,
              let raw = config.baseURL, let url = URL(string: raw.trimmingCharacters(in: .whitespacesAndNewlines)),
              ["http", "https"].contains(url.scheme?.lowercased() ?? ""), url.host != nil,
              url.user == nil, url.password == nil else { throw StoryError.missingModel }
        let send: AgentResponsesModelClient.Transport = { urlRequest in
            guard let requestURL = urlRequest.url else { throw ChatOSAPIError.invalidEndpoint }
            let response = try await transport.send(.init(
                url: requestURL, method: "POST", headers: urlRequest.allHTTPHeaderFields ?? [:],
                body: urlRequest.httpBody, timeoutInterval: urlRequest.timeoutInterval
            ))
            return (response.body, response.statusCode)
        }
        let model = try AgentResponsesModelClient(
            baseURL: url, model: config.model, apiKey: key,
            promptCacheKey: "story-plan:\(request.modelConfigID)", transport: send
        )
        let response = try await model.complete(
            messages: [
                .init(role: .system, content: request.systemPrompt),
                .init(role: .user, content: request.context),
            ],
            tools: [.init(
                name: request.toolName, description: "保存当前规划步骤的结构化结果",
                schema: request.schema, effect: .terminal
            )],
            timeout: 180
        )
        guard response.toolCalls.count == 1,
              response.toolCalls[0].name == request.toolName,
              let data = response.toolCalls[0].arguments.data(using: .utf8),
              (try? JSONSerialization.jsonObject(with: data)) is [String: Any] else { throw StoryError.invalidPlan }
        return data
    }
}

private struct StorySessionBoundModel: AgentModelClient {
    let model: any AgentModelClient
    let client: ChatOSAPIClient
    let session: UUID
    var usesServerSideCompaction: Bool { model.usesServerSideCompaction }
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage {
        guard try await client.currentAuthenticationSessionID() == session else { throw ChatOSAPIError.unauthorized }
        try Task.checkCancellation()
        let response = try await model.complete(messages: messages, tools: tools, timeout: timeout)
        guard try await client.currentAuthenticationSessionID() == session else { throw ChatOSAPIError.unauthorized }
        try Task.checkCancellation()
        return response
    }

    func stream(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval,
                onEvent: @escaping @Sendable (AgentModelStreamEvent) async -> Void) async throws -> AgentMessage {
        guard try await client.currentAuthenticationSessionID() == session else { throw ChatOSAPIError.unauthorized }
        try Task.checkCancellation()
        let response = try await model.stream(messages: messages, tools: tools, timeout: timeout, onEvent: onEvent)
        guard try await client.currentAuthenticationSessionID() == session else { throw ChatOSAPIError.unauthorized }
        try Task.checkCancellation()
        return response
    }
}

private struct Config: Decodable {
    var enabled: Bool
    var model: String
    var provider: String?
    var apiKey: String?
    var baseURL: String?
    enum CodingKeys: String, CodingKey { case enabled, model, provider; case apiKey = "api_key", baseURL = "base_url" }
}
