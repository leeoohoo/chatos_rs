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
        guard ["gpt", "openai", "deepseek", "qwen", "ollama"].contains(config.provider?.lowercased() ?? "gpt") else { throw StoryError.unavailable }
        let model = try AgentChatModelClient(baseURL: url, model: config.model, apiKey: key,
            maximumOutputTokens: (policy.context ?? .init()).outputReserveTokens, transport: { request in
                guard let url = request.url else { throw ChatOSAPIError.invalidEndpoint }
                let response = try await transport.send(.init(url: url, method: "POST", headers: request.allHTTPHeaderFields ?? [:],
                    body: request.httpBody, timeoutInterval: request.timeoutInterval))
                return (response.body, response.statusCode)
            }, streamTransport: { request in
                guard let url = request.url else { throw ChatOSAPIError.invalidEndpoint }
                let response = try await transport.stream(.init(url: url, method: "POST",
                    headers: request.allHTTPHeaderFields ?? [:], body: request.httpBody,
                    timeoutInterval: request.timeoutInterval))
                return .init(statusCode: response.statusCode, headers: response.headers, body: response.body)
            })
        return StorySessionBoundModel(model: model, client: client, session: session)
    }

    public func plan(_ request: StoryPlanningRequest) async throws -> Data {
        let config: Config = try await client.request("/ai-model-configs/\(request.modelConfigID.urlPathEncoded)?include_secret=true")
        guard config.enabled, let key = config.apiKey, !key.isEmpty,
              let raw = config.baseURL, var url = URL(string: raw.trimmingCharacters(in: .whitespacesAndNewlines)),
              ["http", "https"].contains(url.scheme?.lowercased() ?? ""), url.host != nil,
              url.user == nil, url.password == nil else { throw StoryError.missingModel }
        // Protocol selection follows the provider configuration, never the model's marketing name.
        guard ["gpt", "openai", "deepseek", "qwen", "ollama"].contains(config.provider?.lowercased() ?? "gpt") else {
            throw StoryError.unavailable
        }
        if url.path.hasSuffix("/responses") { url.deleteLastPathComponent() }
        if !url.path.hasSuffix("/chat/completions") { url.appendPathComponent("chat/completions") }
        let schema = try JSONSerialization.jsonObject(with: request.schema)
        let body = try JSONSerialization.data(withJSONObject: [
            "model": config.model,
            "messages": [["role": "system", "content": request.systemPrompt], ["role": "user", "content": request.context]],
            "tools": [["type": "function", "function": ["name": request.toolName, "description": "保存当前规划步骤的结构化结果", "parameters": schema]]],
            "tool_choice": ["type": "function", "function": ["name": request.toolName]],
            "stream": false,
        ])
        let response = try await transport.send(.init(url: url, method: "POST",
            headers: ["Authorization": "Bearer \(key)", "Content-Type": "application/json"], body: body, timeoutInterval: 180))
        guard (200..<300).contains(response.statusCode) else { throw PlanningError.rejected(response.statusCode) }
        guard response.body.count <= 2 * 1024 * 1024,
              let root = try JSONSerialization.jsonObject(with: response.body) as? [String: Any],
              let choices = root["choices"] as? [[String: Any]],
              let choice = choices.first, choice["finish_reason"] as? String != "length",
              let message = choice["message"] as? [String: Any],
              let calls = message["tool_calls"] as? [[String: Any]], calls.count == 1,
              let function = calls[0]["function"] as? [String: Any],
              function["name"] as? String == request.toolName,
              let arguments = function["arguments"] as? String,
              let data = arguments.data(using: .utf8),
              (try? JSONSerialization.jsonObject(with: data)) is [String: Any] else { throw StoryError.invalidPlan }
        return data
    }
}

private struct StorySessionBoundModel: AgentModelClient {
    let model: AgentChatModelClient
    let client: ChatOSAPIClient
    let session: UUID
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
private enum PlanningError: LocalizedError {
    case rejected(Int)
    var errorDescription: String? {
        switch self {
        case .rejected(let status): "文本规划请求失败（HTTP \(status)），请检查模型权限、余额及工具调用支持。未自动重试。"
        }
    }
}
