import Foundation

/// Wire adapter shared by approval and story; callers resolve credentials and protocol explicitly.
public struct AgentChatModelClient: AgentModelClient {
    public typealias Transport = @Sendable (URLRequest) async throws -> (Data, Int)
    private let endpoint: URL
    private let model: String
    private let apiKey: String
    private let thinking: String?
    private let maximumOutputTokens: Int?
    private let temperature: Double?
    private let transport: Transport

    public init(baseURL: URL, model: String, apiKey: String, thinking: String? = nil, maximumOutputTokens: Int? = nil, temperature: Double? = nil,
                transport: @escaping Transport = { request in
                    let (data, response) = try await URLSession.shared.data(for: request)
                    guard let response = response as? HTTPURLResponse else { throw AgentRuntimeError.invalidResponse }
                    return (data, response.statusCode)
                }) throws {
        guard ["http", "https"].contains(baseURL.scheme?.lowercased() ?? ""), baseURL.host != nil,
              baseURL.user == nil, baseURL.password == nil, !apiKey.isEmpty, !model.isEmpty else { throw AgentRuntimeError.invalidResponse }
        var endpoint = baseURL
        if endpoint.path.hasSuffix("/responses") { endpoint.deleteLastPathComponent() }
        if !endpoint.path.hasSuffix("/chat/completions") { endpoint.appendPathComponent("chat/completions") }
        self.endpoint = endpoint; self.model = model; self.apiKey = apiKey
        self.thinking = thinking; self.maximumOutputTokens = maximumOutputTokens; self.transport = transport
        self.temperature = temperature
    }

    public func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage {
        var payload: [String: Any] = ["model": model, "stream": false,
            "messages": messages.map { message -> [String: Any] in
                var value: [String: Any] = ["role": message.role.rawValue, "content": message.content]
                if let id = message.toolCallID { value["tool_call_id"] = id }
                if !message.toolCalls.isEmpty {
                    value["tool_calls"] = message.toolCalls.map { ["id": $0.id, "type": "function", "function": ["name": $0.name, "arguments": $0.arguments]] as [String: Any] }
                }
                return value
            },
            "tools": try tools.map { tool in ["type": "function", "function": ["name": tool.name, "description": tool.description,
                "parameters": try JSONSerialization.jsonObject(with: tool.schema)]] as [String: Any] },
            "tool_choice": "auto",
        ]
        if let thinking, !thinking.isEmpty, thinking != "none" { payload["reasoning_effort"] = thinking }
        if let maximumOutputTokens { payload["max_tokens"] = maximumOutputTokens }
        if let temperature { payload["temperature"] = temperature }
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"; request.timeoutInterval = timeout
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: payload)
        let (data, status) = try await transport(request)
        guard (200..<300).contains(status) else {
            if [400, 413, 422].contains(status), data.count <= 65_536,
               let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let error = root["error"] as? [String: Any] {
                let overflowCodes: Set<String> = ["context_length_exceeded", "context_window_exceeded"]
                let code = error["code"] as? String ?? ""
                let type = error["type"] as? String ?? ""
                if overflowCodes.contains(code) || overflowCodes.contains(type) { throw AgentRuntimeError.contextOverflow }
            }
            throw AgentRuntimeError.provider(status)
        }
        guard data.count <= 2 * 1024 * 1024,
              let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let choice = (root["choices"] as? [[String: Any]])?.first,
              choice["finish_reason"] as? String != "length",
              let message = choice["message"] as? [String: Any] else { throw AgentRuntimeError.invalidResponse }
        var calls: [AgentToolCall] = []
        for item in message["tool_calls"] as? [[String: Any]] ?? [] {
            guard let id = item["id"] as? String, !id.isEmpty,
                  let function = item["function"] as? [String: Any], let name = function["name"] as? String,
                  let arguments = function["arguments"] as? String else { throw AgentRuntimeError.invalidResponse }
            calls.append(.init(id: id, name: name, arguments: arguments))
        }
        return .init(role: .assistant, content: message["content"] as? String ?? "", toolCalls: calls)
    }
}
