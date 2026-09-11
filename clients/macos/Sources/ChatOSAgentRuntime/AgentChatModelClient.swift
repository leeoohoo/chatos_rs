import Foundation

public struct AgentHTTPStreamResponse: Sendable {
    public let statusCode: Int
    public let headers: [String: String]
    public let body: AsyncThrowingStream<Data, Error>

    public init(statusCode: Int, headers: [String: String] = [:], body: AsyncThrowingStream<Data, Error>) {
        self.statusCode = statusCode; self.headers = headers; self.body = body
    }
}

/// OpenAI-compatible Chat Completions adapter shared by approval and story Agents.
/// Partial stream deltas never become a durable AgentMessage and cannot execute tools.
public struct AgentChatModelClient: AgentModelClient {
    public typealias Transport = @Sendable (URLRequest) async throws -> (Data, Int)
    public typealias StreamTransport = @Sendable (URLRequest) async throws -> AgentHTTPStreamResponse

    private let endpoint: URL
    private let model: String
    private let apiKey: String
    private let thinking: String?
    private let maximumOutputTokens: Int?
    private let temperature: Double?
    private let transport: Transport
    private let streamTransport: StreamTransport

    public init(
        baseURL: URL, model: String, apiKey: String, thinking: String? = nil,
        maximumOutputTokens: Int? = nil, temperature: Double? = nil,
        transport: @escaping Transport = { request in
            let (data, response) = try await URLSession.shared.data(for: request)
            guard let response = response as? HTTPURLResponse else { throw AgentRuntimeError.invalidResponse }
            return (data, response.statusCode)
        },
        streamTransport: StreamTransport? = nil
    ) throws {
        guard ["http", "https"].contains(baseURL.scheme?.lowercased() ?? ""), baseURL.host != nil,
              baseURL.user == nil, baseURL.password == nil, !apiKey.isEmpty, !model.isEmpty else {
            throw AgentRuntimeError.invalidResponse
        }
        var endpoint = baseURL
        if endpoint.path.hasSuffix("/responses") { endpoint.deleteLastPathComponent() }
        if !endpoint.path.hasSuffix("/chat/completions") { endpoint.appendPathComponent("chat/completions") }
        self.endpoint = endpoint; self.model = model; self.apiKey = apiKey
        self.thinking = thinking; self.maximumOutputTokens = maximumOutputTokens
        self.temperature = temperature; self.transport = transport
        self.streamTransport = streamTransport ?? Self.urlSessionStream
    }

    public func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage {
        let request = try request(messages: messages, tools: tools, timeout: timeout, stream: false)
        let (data, status) = try await transport(request)
        try Self.validate(status: status, errorBody: data)
        return try Self.decodeCompleteResponse(data)
    }

    public func stream(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval,
                       onEvent: @escaping @Sendable (AgentModelStreamEvent) async -> Void) async throws -> AgentMessage {
        let response = try await streamTransport(try request(messages: messages, tools: tools, timeout: timeout, stream: true))
        if !(200..<300).contains(response.statusCode) {
            var body = Data()
            for try await chunk in response.body {
                guard body.count + chunk.count <= 2 * 1_024 * 1_024 else { throw AgentRuntimeError.invalidResponse }
                body.append(chunk)
            }
            try Self.validate(status: response.statusCode, errorBody: body)
        }

        await onEvent(.responseCreated)
        var parser = ChatCompletionsSSEParser()
        for try await chunk in response.body {
            try Task.checkCancellation()
            for event in try parser.append(chunk) { await onEvent(event) }
        }
        let message = try parser.finish()
        await onEvent(.completed)
        return message
    }

    private func request(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval,
                         stream: Bool) throws -> URLRequest {
        var payload: [String: Any] = [
            "model": model, "stream": stream,
            "messages": messages.map { message -> [String: Any] in
                var value: [String: Any] = ["role": message.role.rawValue, "content": message.content]
                if let id = message.toolCallID { value["tool_call_id"] = id }
                if !message.toolCalls.isEmpty {
                    value["tool_calls"] = message.toolCalls.map { call in
                        ["id": call.id, "type": "function", "function": ["name": call.name, "arguments": call.arguments]]
                    }
                }
                return value
            },
            "tools": try tools.map { tool in
                ["type": "function", "function": ["name": tool.name, "description": tool.description,
                    "parameters": try JSONSerialization.jsonObject(with: tool.schema)]]
            },
            "tool_choice": "auto",
        ]
        if let thinking { payload["thinking_level"] = thinking }
        if let maximumOutputTokens { payload["max_tokens"] = maximumOutputTokens }
        if let temperature { payload["temperature"] = temperature }
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"; request.timeoutInterval = timeout
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(stream ? "text/event-stream" : "application/json", forHTTPHeaderField: "Accept")
        request.httpBody = try JSONSerialization.data(withJSONObject: payload)
        return request
    }

    private static func validate(status: Int, errorBody data: Data) throws {
        guard !(200..<300).contains(status) else { return }
        if data.count <= 2 * 1_024 * 1_024,
           let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let error = root["error"] as? [String: Any] {
            let code = (error["code"] as? String ?? "").lowercased()
            let type = (error["type"] as? String ?? "").lowercased()
            if [code, type].contains(where: { $0 == "context_length_exceeded" || $0 == "context_window_exceeded" }) {
                throw AgentRuntimeError.contextOverflow
            }
            if let detail = safeProviderFailureDetail(
                status: status,
                code: code,
                type: type,
                message: (error["message"] as? String ?? "").lowercased()
            ) {
                throw AgentRuntimeError.providerDetail(status, detail)
            }
        }
        throw AgentRuntimeError.provider(status)
    }

    /// Provider bodies are untrusted and can echo request data, so only expose a bounded set of
    /// recognized operational categories instead of displaying the raw upstream message.
    private static func safeProviderFailureDetail(status: Int, code: String, type: String, message: String) -> String? {
        let evidence = [code, type, message].joined(separator: " ")
        if evidence.contains("overloaded") || evidence.contains("server_is_overloaded") {
            if evidence.contains("auth_unavailable") || evidence.contains("no auth available") {
                return "中转服务暂时没有可用的上游授权；最后一个上游模型处于过载状态，请稍后重试。"
            }
            return "上游模型当前过载，请稍后重试。"
        }
        if evidence.contains("auth_unavailable") || evidence.contains("no auth available") {
            return "中转服务暂时没有可用的上游授权，请检查该模型的渠道状态。"
        }
        if status == 429 || evidence.contains("rate_limit") || evidence.contains("too many requests") {
            return "模型服务当前限流，请稍后重试。"
        }
        if evidence.contains("insufficient_quota") || evidence.contains("quota_exceeded") || evidence.contains("balance") {
            return "当前模型额度或余额不足。"
        }
        if status == 401 || evidence.contains("invalid_api_key") || evidence.contains("incorrect api key") {
            return "模型服务拒绝了当前凭证，请检查 Key 是否有效。"
        }
        if status == 502 || status == 503 || status == 504 {
            return "模型服务暂时不可用，请稍后重试。"
        }
        return nil
    }

    fileprivate static func decodeCompleteResponse(_ data: Data) throws -> AgentMessage {
        guard data.count <= 2 * 1_024 * 1_024,
              let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let choices = root["choices"] as? [[String: Any]], let choice = choices.first,
              choice["finish_reason"] as? String != "length",
              let message = choice["message"] as? [String: Any] else { throw AgentRuntimeError.invalidResponse }
        var calls: [AgentToolCall] = []
        for value in message["tool_calls"] as? [[String: Any]] ?? [] {
            guard let id = value["id"] as? String, !id.isEmpty,
                  let function = value["function"] as? [String: Any],
                  let name = function["name"] as? String, !name.isEmpty,
                  let arguments = function["arguments"] as? String else { throw AgentRuntimeError.invalidResponse }
            calls.append(.init(id: id, name: name, arguments: arguments))
        }
        return .init(role: .assistant, content: message["content"] as? String ?? "", toolCalls: calls)
    }

    private static func urlSessionStream(_ request: URLRequest) async throws -> AgentHTTPStreamResponse {
        let (bytes, response) = try await URLSession.shared.bytes(for: request)
        guard let response = response as? HTTPURLResponse else { throw AgentRuntimeError.invalidResponse }
        let headers = response.allHeaderFields.reduce(into: [String: String]()) { result, entry in
            result[String(describing: entry.key).lowercased()] = String(describing: entry.value)
        }
        let body = AsyncThrowingStream<Data, Error> { continuation in
            let task = Task {
                do {
                    var chunk = Data(); chunk.reserveCapacity(4_096)
                    for try await byte in bytes {
                        chunk.append(byte)
                        if byte == 10 || chunk.count >= 4_096 {
                            continuation.yield(chunk); chunk.removeAll(keepingCapacity: true)
                        }
                    }
                    if !chunk.isEmpty { continuation.yield(chunk) }
                    continuation.finish()
                } catch { continuation.finish(throwing: error) }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        return .init(statusCode: response.statusCode, headers: headers, body: body)
    }
}

private struct ChatCompletionsSSEParser {
    private struct CallParts { var id = ""; var name = ""; var arguments = "" }
    private var buffer = Data()
    private var content = ""
    private var calls: [Int: CallParts] = [:]
    private var finishReason: String?
    private var parsedEvents = 0
    private var accumulatedOutputBytes = 0

    mutating func append(_ data: Data) throws -> [AgentModelStreamEvent] {
        guard buffer.count + data.count <= 2 * 1_024 * 1_024 else { throw AgentRuntimeError.invalidResponse }
        buffer.append(data)
        var output: [AgentModelStreamEvent] = []
        while let delimiter = delimiterRange() {
            let packet = buffer.subdata(in: 0..<delimiter.lowerBound)
            buffer.removeSubrange(0..<delimiter.upperBound)
            output += try consume(packet)
        }
        return output
    }

    mutating func finish() throws -> AgentMessage {
        if !buffer.isEmpty {
            let tail = String(decoding: buffer, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
            if tail.hasPrefix("{") { return try AgentChatModelClient.decodeCompleteResponse(buffer) }
            _ = try consume(buffer); buffer.removeAll()
        }
        guard parsedEvents > 0, finishReason != nil, finishReason != "length" else {
            throw AgentRuntimeError.invalidResponse
        }
        let toolCalls = try calls.keys.sorted().map { index -> AgentToolCall in
            let value = calls[index]!
            guard !value.id.isEmpty, !value.name.isEmpty else { throw AgentRuntimeError.invalidResponse }
            return .init(id: value.id, name: value.name, arguments: value.arguments)
        }
        return .init(role: .assistant, content: content, toolCalls: toolCalls)
    }

    private mutating func consume(_ packet: Data) throws -> [AgentModelStreamEvent] {
        let values = String(decoding: packet, as: UTF8.self).split(whereSeparator: \Character.isNewline).compactMap { line -> String? in
            let value = line.trimmingCharacters(in: .whitespaces)
            guard value.hasPrefix("data:") else { return nil }
            return String(value.dropFirst(5)).trimmingCharacters(in: .whitespaces)
        }
        guard !values.isEmpty else { return [] }
        let dataText = values.joined(separator: "\n")
        if dataText == "[DONE]" { return [] }
        guard let data = dataText.data(using: .utf8),
              let root = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw AgentRuntimeError.invalidResponse
        }
        if root["error"] != nil { throw AgentRuntimeError.invalidResponse }
        parsedEvents += 1
        guard parsedEvents <= 100_000 else { throw AgentRuntimeError.invalidResponse }
        guard let choice = (root["choices"] as? [[String: Any]])?.first else { return [] }
        if let reason = choice["finish_reason"] as? String { finishReason = reason }
        guard let delta = choice["delta"] as? [String: Any] else { return [] }
        var output: [AgentModelStreamEvent] = []
        if let piece = delta["content"] as? String, !piece.isEmpty {
            accumulatedOutputBytes += piece.utf8.count
            guard accumulatedOutputBytes <= 2 * 1_024 * 1_024 else { throw AgentRuntimeError.invalidResponse }
            content += piece; output.append(.textDelta(piece))
        }
        for (fallback, raw) in (delta["tool_calls"] as? [[String: Any]] ?? []).enumerated() {
            let index = raw["index"] as? Int ?? fallback
            var value = calls[index] ?? CallParts()
            let id = raw["id"] as? String
            let function = raw["function"] as? [String: Any]
            let name = function?["name"] as? String
            let arguments = function?["arguments"] as? String ?? ""
            accumulatedOutputBytes += (id?.utf8.count ?? 0) + (name?.utf8.count ?? 0) + arguments.utf8.count
            guard accumulatedOutputBytes <= 2 * 1_024 * 1_024 else { throw AgentRuntimeError.invalidResponse }
            if let id { value.id += id }
            if let name { value.name += name }
            value.arguments += arguments; calls[index] = value
            output.append(.toolCallDelta(index: index, id: id, name: name, argumentsDelta: arguments))
        }
        return output
    }

    private func delimiterRange() -> Range<Data.Index>? {
        let lf = buffer.range(of: Data([10, 10]))
        let crlf = buffer.range(of: Data([13, 10, 13, 10]))
        switch (lf, crlf) {
        case let (.some(a), .some(b)): return a.lowerBound < b.lowerBound ? a : b
        case let (.some(a), .none): return a
        case let (.none, .some(b)): return b
        case (.none, .none): return nil
        }
    }
}
