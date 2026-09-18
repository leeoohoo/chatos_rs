import ChatOSAgentRuntime
import ChatOSCore
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
    private let settingsStore: AgentSettingsStore

    init(settingsStore: AgentSettingsStore = .init()) { self.settingsStore = settingsStore }

    func evaluate(
        request: NativeApprovalAgentRequest,
        model: GatewayModelConfigDTO,
        thinkingLevel: String?,
        runID: UUID = UUID(),
        runtimeScope: String? = nil,
        contextProvider: AgentMemoryContextProvider? = nil
    ) async -> NativeApprovalDecision {
        do {
            let policy = try settingsStore.load().effective(.approval)
            guard model.enabled != false,
                  let apiKey = model.apiKey?.trimmedNonEmpty,
                  let baseURLText = model.baseURL?.trimmedNonEmpty,
                  let baseURL = URL(string: baseURLText), !model.model.isEmpty else {
                throw NativeApprovalAgentError.invalidModelConfiguration
            }
            let reserve = (policy.context ?? .init()).outputReserveTokens
            let maximumOutputTokens = min(max(1, model.maxOutputTokens ?? 1_200), reserve)
            let client: any AgentModelClient = try AgentResponsesModelClient(
                baseURL: baseURL, model: model.model, apiKey: apiKey,
                thinking: thinkingLevel, maximumOutputTokens: maximumOutputTokens,
                temperature: model.temperature ?? 0,
                promptCacheKey: "approval-agent:\(model.id)"
            )
            return await evaluate(
                request: request, modelClient: client, policy: policy,
                runID: runID, runtimeScope: runtimeScope, contextProvider: contextProvider
            )
        } catch {
            return .askUser(reason: "本机审批 Agent 不可用：\(error.localizedDescription)")
        }
    }

    /// Shared loop with a separate read-only registry. Production callers provide
    /// a Memory Engine context so prompts, tool calls, and tool results share the
    /// same durable audit contract as the story and server Agents.
    func evaluate(request: NativeApprovalAgentRequest, modelClient: any AgentModelClient,
                  policy: AgentRunPolicy, runID: UUID = UUID(), runtimeScope: String? = nil,
                  contextProvider: AgentMemoryContextProvider? = nil) async -> NativeApprovalDecision {
        do {
            let definitions = try Self.toolSchemas.map { schema -> AgentToolDefinition in
                guard let function = schema["function"] as? [String: Any],
                      let name = function["name"] as? String,
                      let description = function["description"] as? String,
                      let parameters = function["parameters"] as? [String: Any] else {
                    throw NativeApprovalAgentError.invalidToolArguments
                }
                return .init(name: name, description: description,
                    schema: try JSONSerialization.data(withJSONObject: parameters),
                    effect: name == "approval_decision" ? .terminal : .readOnly)
            }
            let scope = runtimeScope ?? "approval:\(runID.uuidString)"
            var checkpoint = AgentRunCheckpoint(scope: scope, messages: [
                .init(role: .system, content: LocalAgentPromptCatalog.render(.approvalSystem)),
                .init(role: .user, content: prompt(for: request)),
            ])
            checkpoint.id = runID
            if let contextProvider {
                checkpoint = try contextProvider.bind(checkpoint)
            }
            let result = try await AgentRuntime().run(checkpoint: checkpoint, scope: checkpoint.scope, policy: policy,
                model: modelClient, tools: definitions, execute: { call in
                    let arguments = try decodeArguments(call.arguments)
                    if call.name == "approval_decision" {
                        _ = try decision(from: arguments)
                        return .init(call.arguments)
                    }
                    let output = tools.execute(name: call.name, arguments: arguments, projectRoot: request.projectRoot)
                    return output.hasPrefix("工具执行失败：") ? .failure(output) : .init(output)
                }, contextProvider: contextProvider)
            guard result.status == .completed, let output = result.result else {
                return .askUser(reason: result.stopReason ?? "本机审批 Agent 未形成有效结论，已转交人工确认。")
            }
            return try decision(from: decodeArguments(output))
        } catch {
            return .askUser(reason: "本机审批 Agent 不可用：\(error.localizedDescription)")
        }
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
        LocalAgentPromptCatalog.render(
            .approvalUser,
            values: [
                "source": request.source,
                "cwd": request.cwd,
                "operation": ([request.command] + request.arguments).joined(separator: " "),
                "requested_permissions": request.requestedPermissionsDescription ?? "null",
                "risk_level": request.riskLevel,
                "risk_reason": request.riskReason ?? "无",
            ]
        )
    }

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
