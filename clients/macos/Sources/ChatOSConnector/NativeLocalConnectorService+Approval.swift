import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    func approvalDecision(
        requestID: String,
        command: String,
        arguments: [String],
        cwd: URL,
        projectRoot: URL,
        source: String,
        risk: NativeApprovalRisk,
        requestedPermissionsDescription: String? = nil,
        approvalScopeKey: String? = nil,
        workspaceID: String? = nil
    ) async -> NativeApprovalDecision {
        if let approvalScopeKey, sessionApprovalAllowlist.contains(approvalScopeKey) {
            let reason = "用户已允许当前本机会话执行此类操作。"
            publishApprovalEvent(.init(
                requestID: requestID,
                command: ([command] + arguments).joined(separator: " "),
                cwd: cwd.path,
                source: source,
                risk: risk.level,
                decision: "approved",
                reason: reason,
                mode: state.approvalMode,
                reviewer: .session
            ))
            return .approve(reason: reason, rememberAllow: true)
        }
        switch state.approvalMode {
        case .fullControl:
            let reason = "当前策略无需逐次审批。"
            publishApprovalEvent(.init(
                requestID: requestID,
                command: ([command] + arguments).joined(separator: " "),
                cwd: cwd.path,
                source: source,
                risk: risk.level,
                decision: "approved",
                reason: reason,
                mode: .fullControl,
                reviewer: .policy
            ))
            return .approve(reason: reason, rememberAllow: false)
        case .requestApproval:
            return await requestUserApproval(
                requestID: requestID,
                command: command,
                arguments: arguments,
                cwd: cwd,
                source: source,
                risk: risk,
                reason: risk.reason,
                approvalScopeKey: approvalScopeKey
            )
        case .autoApproval:
            guard let modelID = state.commandApprovalModelConfigID else {
                return await requestUserApproval(
                    requestID: requestID,
                    command: command,
                    arguments: arguments,
                    cwd: cwd,
                    source: source,
                    risk: risk,
                    reason: "本机审批 Agent 尚未配置模型。",
                    approvalScopeKey: approvalScopeKey
                )
            }
            do {
                let token = try requireAccessToken()
                async let model = gateway.modelConfig(token: token, id: modelID, includeSecret: true)
                async let promptBundle = gateway.agentPromptBundle(token: token)
                async let capability = gateway.agentCapability(
                    token: token,
                    agentKey: NativeApprovalAgent.agentKey
                )
                guard let tenantID = state.user?.id,
                      let workspaceID,
                      let approvalMemoryProviderFactory else {
                    throw AgentContextError.unavailable
                }
                let runID = UUID()
                let runtimeScope = "approval:\(runID.uuidString)"
                async let contextProvider = approvalMemoryProviderFactory(
                    tenantID, workspaceID, runID, runtimeScope
                )
                let resolvedModel = try await model
                let systemPrompt = try NativeApprovalAgent.resolveManagedSystemPrompt(
                    model: resolvedModel,
                    bundle: try await promptBundle,
                    capability: try await capability,
                    ownerUserID: tenantID
                )
                let decision = await NativeApprovalAgent().evaluate(
                    request: .init(
                        command: command,
                        arguments: arguments,
                        cwd: displayPath(cwd, relativeTo: projectRoot),
                        source: source,
                        projectRoot: projectRoot,
                        riskLevel: risk.level,
                        riskReason: risk.reason,
                        requestedPermissionsDescription: requestedPermissionsDescription
                    ),
                    model: resolvedModel,
                    systemPrompt: systemPrompt,
                    thinkingLevel: state.commandApprovalThinkingLevel,
                    runID: runID,
                    runtimeScope: runtimeScope,
                    contextProvider: try await contextProvider
                )
                if case let .askUser(reason) = decision {
                    return await requestUserApproval(
                        requestID: requestID,
                        command: command,
                        arguments: arguments,
                        cwd: cwd,
                        source: source,
                        risk: risk,
                        reason: reason,
                        approvalScopeKey: approvalScopeKey
                    )
                }
                if case .approve(_, true) = decision, let approvalScopeKey {
                    sessionApprovalAllowlist.insert(approvalScopeKey)
                }
                switch decision {
                case let .approve(reason, _):
                    publishApprovalEvent(.init(
                        requestID: requestID,
                        command: ([command] + arguments).joined(separator: " "),
                        cwd: cwd.path,
                        source: source,
                        risk: risk.level,
                        decision: "approved",
                        reason: reason,
                        mode: .autoApproval,
                        reviewer: .ai
                    ))
                case let .deny(reason):
                    publishApprovalEvent(.init(
                        requestID: requestID,
                        command: ([command] + arguments).joined(separator: " "),
                        cwd: cwd.path,
                        source: source,
                        risk: risk.level,
                        decision: "denied",
                        reason: reason,
                        mode: .autoApproval,
                        reviewer: .ai
                    ))
                case .askUser:
                    break
                }
                return decision
            } catch {
                return await requestUserApproval(
                    requestID: requestID,
                    command: command,
                    arguments: arguments,
                    cwd: cwd,
                    source: source,
                    risk: risk,
                    reason: "本机审批 Agent 不可用：\(error.localizedDescription)",
                    approvalScopeKey: approvalScopeKey
                )
            }
        }
    }

    private func requestUserApproval(
        requestID: String,
        command: String,
        arguments: [String],
        cwd: URL,
        source: String,
        risk: NativeApprovalRisk,
        reason: String?,
        approvalScopeKey: String?
    ) async -> NativeApprovalDecision {
        let id = UUID().uuidString
        pendingApprovals.append(.init(
            id: id,
            requestID: requestID,
            command: ([command] + arguments).joined(separator: " "),
            cwd: cwd.path,
            source: source,
            risk: risk.level,
            reason: reason,
            createdAt: ISO8601DateFormatter().string(from: Date()),
            availableDecisions: ["accept", "acceptForSession", "decline"]
        ))
        publishApprovalSnapshot()
        if let approvalScopeKey {
            pendingApprovalScopeKeys[id] = approvalScopeKey
        }
        return await withCheckedContinuation { continuation in
            pendingApprovalContinuations[id] = continuation
        }
    }

    func resolveDirectory(
        _ rawPath: String,
        relativeTo base: URL,
        workspace: LocalConnectorWorkspace
    ) throws -> URL {
        let workspaceRoot = URL(fileURLWithPath: workspace.absoluteRoot)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        let candidate: URL
        if rawPath.hasPrefix("/") {
            candidate = URL(fileURLWithPath: rawPath)
        } else {
            candidate = base.appendingPathComponent(rawPath)
        }
        let resolved = candidate.standardizedFileURL.resolvingSymlinksInPath()
        let prefix = workspaceRoot.path.hasSuffix("/") ? workspaceRoot.path : workspaceRoot.path + "/"
        var isDirectory: ObjCBool = false
        guard resolved.path == workspaceRoot.path || resolved.path.hasPrefix(prefix),
              FileManager.default.fileExists(atPath: resolved.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw NativeTerminalRelayError.unsafeDirectory
        }
        return resolved
    }

    private func displayPath(_ url: URL, relativeTo root: URL) -> String {
        guard url.path != root.path else { return "." }
        let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
        return url.path.hasPrefix(prefix) ? String(url.path.dropFirst(prefix.count)) : url.path
    }

    func appendApprovalHistory(
        command: String,
        arguments: [String],
        cwd: String,
        source: String,
        decision: String,
        risk: NativeApprovalRisk,
        reason: String
    ) {
        state.approvalHistory.insert(.init(
            id: UUID().uuidString,
            command: ([command] + arguments).joined(separator: " "),
            cwd: cwd,
            source: source,
            mode: state.approvalMode,
            decision: decision,
            risk: risk.level,
            reason: reason,
            createdAt: ISO8601DateFormatter().string(from: Date())
        ), at: 0)
        state.approvalHistory = Array(state.approvalHistory.prefix(1_000))
        try? stateStore.save(state)
    }

    func appendCommandHistory(
        result: LocalConnectorTerminalResult,
        display: String,
        workspace: LocalConnectorWorkspace,
        source: String
    ) {
        state.commandHistory.insert(.init(
            id: UUID().uuidString,
            source: source,
            workspaceAlias: workspace.alias,
            cwd: result.cwd,
            display: display,
            status: result.success ? "completed" : "failed",
            exitCode: result.exitCode,
            stdoutPreview: String(result.stdout.prefix(2_000)),
            stderrPreview: String(result.stderr.prefix(2_000)),
            error: result.error,
            startedAt: ISO8601DateFormatter().string(from: Date())
        ), at: 0)
        state.commandHistory = Array(state.commandHistory.prefix(1_000))
        try? stateStore.save(state)
    }

    func terminalResponse(
        requestID: String,
        command: String,
        arguments: [String],
        cwd: String,
        result: LocalConnectorTerminalResult?,
        error: String?,
        approvalDecision: String
    ) -> NativeRelayResponse {
        .init(
            type: "terminal_response",
            requestID: requestID,
            status: 200,
            body: .object([
                "command": .string(command),
                "args": .array(arguments.map(NativeJSONValue.string)),
                "cwd": .string(cwd),
                "success": .bool(result?.success ?? false),
                "exit_code": result?.exitCode.map { .number(Double($0)) } ?? .null,
                "timed_out": .bool(result?.timedOut ?? false),
                "stdout": .string(result?.stdout ?? ""),
                "stderr": .string(result?.stderr ?? ""),
                "error": error.map(NativeJSONValue.string) ?? .null,
                "approval_decision": .string(approvalDecision),
            ])
        )
    }

    func sendRelayResponse(
        _ response: NativeRelayResponse,
        socket: URLSessionWebSocketTask
    ) async throws {
        let data = try JSONEncoder().encode(response)
        guard let text = String(data: data, encoding: .utf8) else {
            throw NativeTerminalRelayError.invalidResponse
        }
        try await socket.send(.string(text))
    }
}
