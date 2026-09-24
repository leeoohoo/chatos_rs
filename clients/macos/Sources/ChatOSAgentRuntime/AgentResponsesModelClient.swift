import Foundation

/// Official OpenAI Responses adapter for native Agents.
///
/// The adapter uses OpenAI's stateless input-array chaining. Every successful
/// response carries its exact `response.output` into the next request, including
/// encrypted reasoning and compaction items.
public actor AgentResponsesModelClient: AgentModelClient {
    public typealias Transport = @Sendable (URLRequest) async throws -> (Data, Int)
    public typealias StreamTransport = @Sendable (URLRequest) async throws -> AgentHTTPStreamResponse

    public nonisolated let usesServerSideCompaction = true

    private static let compactionThreshold = 200_000
    private let endpoint: URL
    private let model: String
    private let apiKey: String
    private let thinking: String?
    private let maximumOutputTokens: Int?
    private let temperature: Double?
    private let promptCacheKey: String
    private let transport: Transport
    private let streamTransport: StreamTransport
    fileprivate struct ParsedResponse {
        let id: String
        let message: AgentMessage
    }

    public init(
        baseURL: URL, model: String, apiKey: String, thinking: String? = nil,
        maximumOutputTokens: Int? = nil, temperature: Double? = nil,
        promptCacheKey: String? = nil,
        transport: @escaping Transport = { request in
            let (data, response) = try await URLSession.shared.data(for: request)
            guard let response = response as? HTTPURLResponse else {
                throw AgentRuntimeError.invalidResponse
            }
            return (data, response.statusCode)
        },
        streamTransport: StreamTransport? = nil
    ) throws {
        guard ["http", "https"].contains(baseURL.scheme?.lowercased() ?? ""),
              baseURL.host != nil, baseURL.user == nil, baseURL.password == nil,
              !apiKey.isEmpty, !model.isEmpty else {
            throw AgentRuntimeError.invalidResponse
        }
        var endpoint = baseURL
        if endpoint.path.hasSuffix("/chat/completions") {
            endpoint.deleteLastPathComponent()
            endpoint.deleteLastPathComponent()
        } else if endpoint.path.hasSuffix("/responses") {
            endpoint.deleteLastPathComponent()
        }
        endpoint.appendPathComponent("responses")
        self.endpoint = endpoint
        self.model = model
        self.apiKey = apiKey
        self.thinking = thinking
        self.maximumOutputTokens = maximumOutputTokens
        self.temperature = temperature
        self.promptCacheKey = promptCacheKey?.trimmingCharacters(in: .whitespacesAndNewlines)
            .nonEmpty ?? "chatos-native:\(model)"
        self.transport = transport
        self.streamTransport = streamTransport ?? Self.urlSessionStream
    }

    public nonisolated static func shouldUse(
        provider: String?, baseURL: URL
    ) -> Bool {
        _ = provider
        return ["http", "https"].contains(baseURL.scheme?.lowercased() ?? "")
            && baseURL.host != nil && baseURL.user == nil && baseURL.password == nil
    }

    public func complete(
        messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval
    ) async throws -> AgentMessage {
        let request = try request(messages: messages, tools: tools, timeout: timeout, stream: false)
        let (data, status) = try await transport(request)
        try validate(status: status, errorBody: data)
        return try Self.decodeResponse(data).message
    }

    public func stream(
        messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval,
        onEvent: @escaping @Sendable (AgentModelStreamEvent) async -> Void
    ) async throws -> AgentMessage {
        let response = try await streamTransport(
            try request(messages: messages, tools: tools, timeout: timeout, stream: true)
        )
        await onEvent(.responseCreated)
        if !(200..<300).contains(response.statusCode) {
            var body = Data()
            for try await chunk in response.body {
                await onEvent(.activity(bytes: chunk.count))
                guard body.count + chunk.count <= 2 * 1_024 * 1_024 else {
                    throw AgentRuntimeError.invalidResponse
                }
                body.append(chunk)
            }
            try validate(status: response.statusCode, errorBody: body)
        }
        if let contentType = response.headers["content-type"]?.lowercased(),
           !contentType.contains("text/event-stream") {
            throw AgentRuntimeError.responsesStreamUnexpectedContentType
        }

        var parser = ResponsesSSEParser()
        for try await chunk in response.body {
            try Task.checkCancellation()
            await onEvent(.activity(bytes: chunk.count))
            for event in try parser.append(chunk) { await onEvent(event) }
        }
        let parsed = try parser.finish()
        await onEvent(.completed)
        return parsed.message
    }

    private func request(
        messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval,
        stream: Bool
    ) throws -> URLRequest {
        var payload: [String: Any] = [
            "model": model,
            "input": try Self.inputItems(messages),
            "tools": try tools.map(Self.responseTool),
            "tool_choice": "auto",
            "stream": stream,
            "store": false,
            "prompt_cache_key": promptCacheKey,
            "context_management": [[
                "type": "compaction",
                "compact_threshold": Self.compactionThreshold,
            ]],
        ]
        if let maximumOutputTokens { payload["max_output_tokens"] = maximumOutputTokens }
        if let temperature { payload["temperature"] = temperature }
        if let thinking = thinking?.trimmingCharacters(in: .whitespacesAndNewlines),
           !thinking.isEmpty, thinking.lowercased() != "auto" {
            payload["reasoning"] = ["effort": thinking.lowercased()]
            payload["include"] = ["reasoning.encrypted_content"]
        }

        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.timeoutInterval = timeout
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(stream ? "text/event-stream" : "application/json", forHTTPHeaderField: "Accept")
        request.httpBody = try JSONSerialization.data(withJSONObject: payload)
        return request
    }

    private static func inputItems(_ messages: [AgentMessage]) throws -> [[String: Any]] {
        var items: [[String: Any]] = []
        for message in messages {
            switch message.role {
            case .system, .user:
                if message.attachmentItems.isEmpty {
                    items.append(["role": message.role.rawValue, "content": message.content])
                } else {
                    items.append([
                        "role": message.role.rawValue,
                        "content": try inputContent(message),
                    ])
                }
            case .assistant:
                if let raw = message.responseOutputJSON {
                    guard let output = try JSONSerialization.jsonObject(with: raw) as? [[String: Any]] else {
                        throw AgentRuntimeError.invalidResponse
                    }
                    items.append(contentsOf: output)
                } else if !message.content.isEmpty {
                    items.append(["role": "assistant", "content": message.content])
                    items.append(contentsOf: message.toolCalls.map { call in
                        [
                            "type": "function_call",
                            "call_id": call.id,
                            "name": call.name,
                            "arguments": call.arguments,
                        ]
                    })
                } else {
                    items.append(contentsOf: message.toolCalls.map { call in
                        [
                            "type": "function_call",
                            "call_id": call.id,
                            "name": call.name,
                            "arguments": call.arguments,
                        ]
                    })
                }
            case .tool:
                guard let callID = message.toolCallID, !callID.isEmpty else {
                    throw AgentRuntimeError.invalidResponse
                }
                items.append([
                    "type": "function_call_output",
                    "call_id": callID,
                    "output": message.content,
                ])
            }
        }
        if let compactedAt = items.lastIndex(where: { $0["type"] as? String == "compaction" }),
           compactedAt > 0 {
            items.removeFirst(compactedAt)
        }
        guard !items.isEmpty else { throw AgentRuntimeError.invalidResponse }
        return items
    }

    private static func inputContent(_ message: AgentMessage) throws -> [[String: Any]] {
        var content: [[String: Any]] = []
        if !message.content.isEmpty {
            content.append(["type": "input_text", "text": message.content])
        }
        for attachment in message.attachmentItems {
            let data = try validatedAttachmentData(attachment)
            let dataURL = "data:\(attachment.mimeType);base64,\(data.base64EncodedString())"
            switch attachment.kind {
            case .image:
                content.append([
                    "type": "input_image",
                    "image_url": dataURL,
                    "detail": "auto",
                ])
            case .file where attachment.mimeType == "application/pdf":
                content.append([
                    "type": "input_file",
                    "filename": attachment.name,
                    "file_data": dataURL,
                ])
            case .file, .audio:
                if let text = boundedText(data) {
                    content.append([
                        "type": "input_text",
                        "text": "<attachment name=\"\(attachment.name)\" mime_type=\"\(attachment.mimeType)\">\n\(text)\n</attachment>",
                    ])
                } else {
                    content.append([
                        "type": "input_text",
                        "text": "[附件：\(attachment.name)，类型 \(attachment.mimeType)。需要时通过 Relay 的附件读取工具处理。]",
                    ])
                }
            }
        }
        return content
    }

    private static func validatedAttachmentData(_ attachment: AgentMessageAttachment) throws -> Data {
        guard attachment.localFileURL.isFileURL else { throw AgentRuntimeError.invalidAttachment }
        let data = try Data(contentsOf: attachment.localFileURL, options: [.mappedIfSafe])
        guard !data.isEmpty, data.count <= 20 * 1_024 * 1_024 else {
            throw AgentRuntimeError.invalidAttachment
        }
        return data
    }

    private static func boundedText(_ data: Data) -> String? {
        guard !data.prefix(8_000).contains(0),
              let text = String(data: data.prefix(512 * 1_024), encoding: .utf8) else { return nil }
        return data.count > 512 * 1_024
            ? text + "\n[附件内容已截断；可通过 Relay 分段读取]"
            : text
    }

    private static func responseTool(_ tool: AgentToolDefinition) throws -> [String: Any] {
        [
            "type": "function",
            "name": tool.name,
            "description": tool.description,
            "parameters": try JSONSerialization.jsonObject(with: tool.schema),
        ]
    }

    private static func decodeResponse(_ data: Data) throws -> ParsedResponse {
        guard data.count <= 2 * 1_024 * 1_024,
              let root = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw AgentRuntimeError.invalidResponse
        }
        return try decodeResponse(root)
    }

    fileprivate static func decodeResponse(_ root: [String: Any]) throws -> ParsedResponse {
        let status = root["status"] as? String
        if status == "incomplete" {
            let details = root["incomplete_details"] as? [String: Any]
            throw AgentRuntimeError.responsesIncomplete(
                safeProtocolToken(details?["reason"], fallback: "unknown")
            )
        }
        if status == "failed" {
            let error = root["error"] as? [String: Any]
            throw AgentRuntimeError.responsesFailed(
                safeProtocolToken(error?["code"], fallback: "unknown")
            )
        }
        guard let id = root["id"] as? String, !id.isEmpty,
              (status ?? "completed") == "completed",
              let output = root["output"] as? [[String: Any]] else {
            throw AgentRuntimeError.invalidResponsesEnvelope
        }
        var content = ""
        var calls: [AgentToolCall] = []
        for item in output {
            switch item["type"] as? String {
            case "message":
                for part in item["content"] as? [[String: Any]] ?? [] {
                    if ["output_text", "text"].contains(part["type"] as? String ?? ""),
                       let text = part["text"] as? String {
                        content += text
                    }
                }
            case "function_call":
                guard let callID = (item["call_id"] as? String) ?? (item["id"] as? String),
                      !callID.isEmpty,
                      let name = item["name"] as? String, !name.isEmpty,
                      let arguments = item["arguments"] as? String else {
                    throw AgentRuntimeError.invalidResponsesFunctionCall
                }
                calls.append(.init(id: callID, name: name, arguments: arguments))
            default:
                continue // reasoning and encrypted compaction items are provider-owned state.
            }
        }
        let outputJSON = try JSONSerialization.data(withJSONObject: output)
        let usage = decodeUsage(root["usage"] as? [String: Any])
        return .init(id: id, message: .init(
            role: .assistant, content: content, toolCalls: calls,
            responseOutputJSON: outputJSON, usage: usage
        ))
    }

    fileprivate static func safeProtocolToken(_ value: Any?, fallback: String) -> String {
        guard let value = value as? String, !value.isEmpty, value.count <= 80,
              value.unicodeScalars.allSatisfy({
                  CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "._-")).contains($0)
              }) else { return fallback }
        return value
    }

    private func validate(status: Int, errorBody data: Data) throws {
        guard !(200..<300).contains(status) else { return }
        if data.count <= 2 * 1_024 * 1_024,
           let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let error = root["error"] as? [String: Any] {
            let code = (error["code"] as? String ?? "").lowercased()
            let type = (error["type"] as? String ?? "").lowercased()
            if [code, type].contains(where: {
                $0 == "context_length_exceeded" || $0 == "context_window_exceeded"
            }) {
                throw AgentRuntimeError.contextOverflow
            }
        }
        throw AgentRuntimeError.provider(status)
    }

    private static func decodeUsage(_ usage: [String: Any]?) -> AgentUsage? {
        guard let usage else { return nil }
        let details = usage["input_tokens_details"] as? [String: Any]
        return AgentUsage(
            inputTokens: usage["input_tokens"] as? Int ?? 0,
            cachedTokens: details?["cached_tokens"] as? Int ?? 0,
            outputTokens: usage["output_tokens"] as? Int ?? 0,
            requests: 1
        )
    }

    private static func urlSessionStream(_ request: URLRequest) async throws -> AgentHTTPStreamResponse {
        let (bytes, response) = try await URLSession.shared.bytes(for: request)
        guard let response = response as? HTTPURLResponse else {
            throw AgentRuntimeError.invalidResponse
        }
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

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}

private struct ResponsesSSEParser {
    private var buffer = Data()
    private var terminal: AgentResponsesModelClient.ParsedResponse?
    private var parsedEvents = 0

    mutating func append(_ data: Data) throws -> [AgentModelStreamEvent] {
        guard buffer.count + data.count <= 2 * 1_024 * 1_024 else {
            throw AgentRuntimeError.invalidResponse
        }
        buffer.append(data)
        var output: [AgentModelStreamEvent] = []
        while let delimiter = delimiterRange() {
            let packet = buffer.subdata(in: 0..<delimiter.lowerBound)
            buffer.removeSubrange(0..<delimiter.upperBound)
            output += try consume(packet)
        }
        return output
    }

    mutating func finish() throws -> AgentResponsesModelClient.ParsedResponse {
        if !buffer.isEmpty {
            _ = try consume(buffer)
            buffer.removeAll()
        }
        guard let terminal else { throw AgentRuntimeError.responsesStreamMissingCompletion }
        return terminal
    }

    private mutating func consume(_ packet: Data) throws -> [AgentModelStreamEvent] {
        let values = String(decoding: packet, as: UTF8.self)
            .split(whereSeparator: \Character.isNewline)
            .compactMap { line -> String? in
                let value = line.trimmingCharacters(in: .whitespaces)
                guard value.hasPrefix("data:") else { return nil }
                return String(value.dropFirst(5)).trimmingCharacters(in: .whitespaces)
            }
        guard !values.isEmpty else { return [] }
        let dataText = values.joined(separator: "\n")
        if dataText == "[DONE]" { return [] }
        guard let data = dataText.data(using: .utf8),
              let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = root["type"] as? String else {
            throw AgentRuntimeError.invalidResponsesEnvelope
        }
        parsedEvents += 1
        guard parsedEvents <= 100_000 else { throw AgentRuntimeError.invalidResponse }
        switch type {
        case "response.output_text.delta":
            guard let delta = root["delta"] as? String else { return [] }
            return delta.isEmpty ? [] : [.textDelta(delta)]
        case "response.output_item.added":
            guard let item = root["item"] as? [String: Any],
                  item["type"] as? String == "function_call" else { return [] }
            let index = root["output_index"] as? Int ?? 0
            return [.toolCallDelta(
                index: index,
                id: (item["call_id"] as? String) ?? (item["id"] as? String),
                name: item["name"] as? String,
                argumentsDelta: item["arguments"] as? String ?? ""
            )]
        case "response.function_call_arguments.delta":
            return [.toolCallDelta(
                index: root["output_index"] as? Int ?? 0,
                id: nil,
                name: nil,
                argumentsDelta: root["delta"] as? String ?? ""
            )]
        case "response.completed":
            guard let response = root["response"] as? [String: Any] else {
                throw AgentRuntimeError.invalidResponsesEnvelope
            }
            terminal = try AgentResponsesModelClient.decodeResponse(response)
            return []
        case "response.incomplete":
            let response = root["response"] as? [String: Any]
            let details = response?["incomplete_details"] as? [String: Any]
            throw AgentRuntimeError.responsesIncomplete(
                AgentResponsesModelClient.safeProtocolToken(
                    details?["reason"], fallback: "unknown"
                )
            )
        case "response.failed":
            let response = root["response"] as? [String: Any]
            let error = response?["error"] as? [String: Any]
            throw AgentRuntimeError.responsesFailed(
                AgentResponsesModelClient.safeProtocolToken(error?["code"], fallback: "unknown")
            )
        case "error":
            throw AgentRuntimeError.responsesStreamError(
                AgentResponsesModelClient.safeProtocolToken(root["code"], fallback: "unknown")
            )
        default:
            return []
        }
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
