import Foundation

enum NativeApprovalDecision: Sendable, Equatable {
    case approve(reason: String, rememberAllow: Bool)
    case deny(reason: String)
    case askUser(reason: String)
}

struct NativeApprovalAgentRequest: Sendable {
    var command: String
    var arguments: [String]
    var cwd: String
    var source: String
    var projectRoot: URL
    var riskLevel: String
    var riskReason: String?
    var requestedPermissionsDescription: String?
}

struct NativeApprovalAgent: Sendable {
    private let tools = NativeApprovalAgentTools()
    private let maximumIterations = 8

    func evaluate(
        request: NativeApprovalAgentRequest,
        model: GatewayModelConfigDTO,
        thinkingLevel: String?,
        maximumRetries: Int
    ) async -> NativeApprovalDecision {
        do {
            return try await run(
                request: request,
                model: model,
                thinkingLevel: thinkingLevel,
                maximumRetries: min(max(0, maximumRetries), 1)
            )
        } catch {
            return .askUser(reason: "本机审批 Agent 不可用：\(error.localizedDescription)")
        }
    }

    private func run(
        request: NativeApprovalAgentRequest,
        model: GatewayModelConfigDTO,
        thinkingLevel: String?,
        maximumRetries: Int
    ) async throws -> NativeApprovalDecision {
        guard let apiKey = model.apiKey?.trimmedNonEmpty,
              let baseURLText = model.baseURL?.trimmedNonEmpty,
              let baseURL = URL(string: baseURLText),
              !model.model.isEmpty else {
            throw NativeApprovalAgentError.invalidModelConfiguration
        }

        var messages: [[String: Any]] = [
            ["role": "system", "content": Self.systemPrompt],
            ["role": "user", "content": prompt(for: request)],
        ]
        var requestedDecisionRetry = false

        for _ in 0..<maximumIterations {
            let message = try await complete(
                baseURL: baseURL,
                apiKey: apiKey,
                model: model,
                thinkingLevel: thinkingLevel,
                maximumRetries: maximumRetries,
                messages: messages
            )
            messages.append(message)
            let toolCalls = message["tool_calls"] as? [[String: Any]] ?? []
            if toolCalls.isEmpty {
                if !requestedDecisionRetry {
                    requestedDecisionRetry = true
                    messages.append([
                        "role": "user",
                        "content": "你还没有调用 approval_decision。现在必须调用它，并且只能返回 approve、deny 或 ask_user。",
                    ])
                    continue
                }
                return .askUser(reason: "本机审批 Agent 没有形成有效的工具决策。")
            }

            for call in toolCalls {
                guard let callID = call["id"] as? String,
                      let function = call["function"] as? [String: Any],
                      let name = function["name"] as? String else {
                    continue
                }
                let argumentsText = function["arguments"] as? String ?? "{}"
                let arguments = try decodeArguments(argumentsText)
                if name == "approval_decision" {
                    return try decision(from: arguments)
                }
                let result = tools.execute(
                    name: name,
                    arguments: arguments,
                    projectRoot: request.projectRoot
                )
                messages.append([
                    "role": "tool",
                    "tool_call_id": callID,
                    "name": name,
                    "content": result,
                ])
            }
        }
        return .askUser(reason: "本机审批 Agent 超过最大检查轮次。")
    }

    private func complete(
        baseURL: URL,
        apiKey: String,
        model: GatewayModelConfigDTO,
        thinkingLevel: String?,
        maximumRetries: Int,
        messages: [[String: Any]]
    ) async throws -> [String: Any] {
        let endpoint = chatCompletionsURL(baseURL)
        var payload: [String: Any] = [
            "model": model.model,
            "messages": messages,
            "tools": Self.toolSchemas,
            "tool_choice": "auto",
            "temperature": model.temperature ?? 0,
            "max_tokens": model.maxOutputTokens ?? 1_200,
        ]
        if let thinkingLevel = thinkingLevel?.trimmedNonEmpty,
           thinkingLevel != "none" {
            payload["reasoning_effort"] = thinkingLevel
        }
        let body = try JSONSerialization.data(withJSONObject: payload)
        var lastError: Error?
        for attempt in 0...maximumRetries {
            do {
                var request = URLRequest(url: endpoint)
                request.httpMethod = "POST"
                request.timeoutInterval = 45
                request.httpBody = body
                request.setValue("application/json", forHTTPHeaderField: "Content-Type")
                request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
                let (data, response) = try await URLSession.shared.data(for: request)
                guard let http = response as? HTTPURLResponse else {
                    throw NativeApprovalAgentError.invalidResponse
                }
                guard (200..<300).contains(http.statusCode) else {
                    let detail = String(decoding: data, as: UTF8.self)
                    throw NativeApprovalAgentError.upstream(http.statusCode, detail)
                }
                guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                      let choices = root["choices"] as? [[String: Any]],
                      let message = choices.first?["message"] as? [String: Any] else {
                    throw NativeApprovalAgentError.invalidResponse
                }
                return message
            } catch {
                lastError = error
                if attempt < maximumRetries { continue }
            }
        }
        throw lastError ?? NativeApprovalAgentError.invalidResponse
    }

    private func chatCompletionsURL(_ baseURL: URL) -> URL {
        if baseURL.path.hasSuffix("/chat/completions") { return baseURL }
        return baseURL
            .appendingPathComponent("chat", isDirectory: true)
            .appendingPathComponent("completions")
    }

    private func decodeArguments(_ text: String) throws -> [String: Any] {
        guard let data = text.data(using: .utf8),
              let value = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw NativeApprovalAgentError.invalidToolArguments
        }
        return value
    }

    private func decision(from arguments: [String: Any]) throws -> NativeApprovalDecision {
        guard let rawDecision = arguments["decision"] as? String,
              let reason = (arguments["reason"] as? String)?.trimmedNonEmpty else {
            throw NativeApprovalAgentError.invalidDecision
        }
        switch rawDecision {
        case "approve":
            return .approve(reason: reason, rememberAllow: arguments["remember_allow"] as? Bool ?? false)
        case "deny":
            return .deny(reason: reason)
        case "ask_user":
            return .askUser(reason: reason)
        default:
            throw NativeApprovalAgentError.invalidDecision
        }
    }

    private func prompt(for request: NativeApprovalAgentRequest) -> String {
        """
        请审核下面这次本机操作。它可能是 shell 命令，也可能是 Browser CDP、Computer Use 或其他本机 Plugin 操作。必要时先使用只读工具检查项目，再调用 approval_decision。

        - source: \(request.source)
        - cwd: \(request.cwd)
        - operation: \(([request.command] + request.arguments).joined(separator: " "))
        - requested_permissions: \(request.requestedPermissionsDescription ?? "null")
        - static_risk_level: \(request.riskLevel)
        - static_risk_reason: \(request.riskReason ?? "无")

        规则：
        1. 只判断这一次请求，不要执行命令，也不要修改文件。
        2. 信息不足、路径不明确、请求范围过大或存在不可逆风险时，必须 ask_user。
        3. deny 用于明确恶意、越权或与用户目标冲突的操作。
        4. approve 只用于意图清晰、范围受控且与当前项目任务一致的操作。
        5. Browser CDP、Computer Use 和其他 Plugin 操作不是 shell 命令，不要因为项目中找不到同名文件而拒绝或追问。ChatOS 生成的 browser_session_id、tab_id、cdp_session_id 等不透明标识属于正常会话边界，应结合工具名、参数摘要和权限说明判断。
        """
    }

    private static let systemPrompt = """
    你是 ChatOS 运行在用户 Mac 上的本机操作审批 Agent，负责审核 shell 命令、Browser CDP、Computer Use 和其他本机 Plugin 操作。你只能使用提供的只读项目工具进行核对，最终必须调用 approval_decision。你不得把普通文字回答当作审批结论，不得执行命令、写文件或访问项目根目录之外的路径。无法可靠判断时必须 ask_user。
    """

    private static var toolSchemas: [[String: Any]] { [
        functionTool("read_file_raw", "读取项目内 UTF-8 文本文件。", [
            "type": "object", "properties": ["path": ["type": "string"]], "required": ["path"],
        ]),
        functionTool("read_file_range", "读取文本文件的指定行范围。", [
            "type": "object",
            "properties": [
                "path": ["type": "string"],
                "start_line": ["type": "integer", "minimum": 1],
                "end_line": ["type": "integer", "minimum": 1],
            ],
            "required": ["path", "start_line", "end_line"],
        ]),
        functionTool("list_dir", "列出项目内目录。", [
            "type": "object", "properties": ["path": ["type": "string"]], "required": ["path"],
        ]),
        functionTool("search_text", "在项目文本文件中搜索固定文本。", [
            "type": "object",
            "properties": [
                "query": ["type": "string"],
                "path": ["type": "string"],
            ],
            "required": ["query"],
        ]),
        functionTool("approval_decision", "提交唯一且最终的审批结论。", [
            "type": "object",
            "properties": [
                "decision": ["type": "string", "enum": ["approve", "deny", "ask_user"]],
                "reason": ["type": "string"],
                "remember_allow": ["type": "boolean"],
            ],
            "required": ["decision", "reason"],
        ]),
    ] }

    private static func functionTool(
        _ name: String,
        _ description: String,
        _ parameters: [String: Any]
    ) -> [String: Any] {
        [
            "type": "function",
            "function": [
                "name": name,
                "description": description,
                "parameters": parameters,
            ],
        ]
    }
}

private enum NativeApprovalAgentError: LocalizedError {
    case invalidModelConfiguration
    case invalidResponse
    case invalidToolArguments
    case invalidDecision
    case upstream(Int, String)

    var errorDescription: String? {
        switch self {
        case .invalidModelConfiguration: "审批模型配置缺少 Base URL、模型名或 API Key"
        case .invalidResponse: "审批模型返回格式无效"
        case .invalidToolArguments: "审批模型返回了无效工具参数"
        case .invalidDecision: "审批模型没有返回有效审批结论"
        case let .upstream(status, detail): "审批模型请求失败（HTTP \(status)）：\(detail.prefix(400))"
        }
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
