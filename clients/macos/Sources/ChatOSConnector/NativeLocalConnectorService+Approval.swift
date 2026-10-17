import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation

extension NativeLocalConnectorService {
    enum ApprovalSessionReuse: Sendable, Equatable {
        /// Reuse only when the presented operation and arguments are identical.
        case exactRequest
        /// Reuse the stable scope declared by a host-owned capability boundary.
        case declaredScope
    }

    func approvalDecision(
        requestID: String,
        command: String,
        arguments: [String],
        cwd: URL,
        projectRoot: URL,
        source: String,
        risk: NativeApprovalRisk,
        purpose: String? = nil,
        requestedPermissionsDescription: String? = nil,
        approvalScopeKey: String? = nil,
        approvalSessionReuse: ApprovalSessionReuse = .exactRequest,
        evaluationArguments: [String]? = nil,
        workspaceID: String? = nil
    ) async -> NativeApprovalDecision {
        let scopedApprovalKey = approvalScopeKey.map {
            approvalSessionScope(
                base: $0,
                command: command,
                arguments: arguments,
                cwd: cwd,
                projectRoot: projectRoot,
                workspaceID: workspaceID,
                purpose: purpose,
                requestedPermissionsDescription: requestedPermissionsDescription,
                reuse: approvalSessionReuse
            )
        }
        if let scopedApprovalKey, sessionApprovalAllowlist.contains(scopedApprovalKey) {
            let reason = "用户已允许当前本机会话执行此类操作。"
            publishApprovalEvent(.init(
                requestID: requestID,
                command: ([command] + arguments).joined(separator: " "),
                purpose: purpose,
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
                purpose: purpose,
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
                purpose: purpose,
                approvalScopeKey: scopedApprovalKey
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
                    purpose: purpose,
                    approvalScopeKey: scopedApprovalKey
                )
            }
            guard let runtime = localApprovalAgentRuntime,
                  runtime.model.configID == modelID else {
                return await requestUserApproval(
                    requestID: requestID,
                    command: command,
                    arguments: arguments,
                    cwd: cwd,
                    source: source,
                    risk: risk,
                    reason: localApprovalAgentUnavailableReason,
                    purpose: purpose,
                    approvalScopeKey: scopedApprovalKey
                )
            }
            do {
                let runID = UUID()
                let runtimeScope = "approval:\(runID.uuidString)"
                let contextProvider: AgentMemoryContextProvider?
                if let workspaceID, let approvalMemoryProviderFactory {
                    contextProvider = try await approvalMemoryProviderFactory(
                        runtime.ownerUserID, workspaceID, runID, runtimeScope
                    )
                } else {
                    contextProvider = nil
                }
                let decision = await NativeApprovalAgent().evaluate(
                    request: .init(
                        command: command,
                        arguments: evaluationArguments ?? arguments,
                        cwd: displayPath(cwd, relativeTo: projectRoot),
                        source: source,
                        projectRoot: projectRoot,
                        riskLevel: risk.level,
                        riskReason: risk.reason,
                        purpose: purpose,
                        requestedPermissionsDescription: requestedPermissionsDescription
                    ),
                    model: runtime.model,
                    systemPrompt: runtime.systemPrompt,
                    thinkingLevel: runtime.thinkingLevel,
                    runID: runID,
                    runtimeScope: runtimeScope,
                    contextProvider: contextProvider
                )
                return await finalizeAutomaticApproval(
                    decision,
                    requestID: requestID,
                    command: command,
                    arguments: arguments,
                    cwd: cwd,
                    source: source,
                    risk: risk,
                    purpose: purpose,
                    approvalScopeKey: scopedApprovalKey
                )
            } catch {
                return await requestUserApproval(
                    requestID: requestID,
                    command: command,
                    arguments: arguments,
                    cwd: cwd,
                    source: source,
                    risk: risk,
                    reason: "本机审批 Agent 不可用：\(error.localizedDescription)",
                    purpose: purpose,
                    approvalScopeKey: scopedApprovalKey
                )
            }
        }
    }

    private func finalizeAutomaticApproval(
        _ decision: NativeApprovalDecision,
        requestID: String,
        command: String,
        arguments: [String],
        cwd: URL,
        source: String,
        risk: NativeApprovalRisk,
        purpose: String?,
        approvalScopeKey: String?
    ) async -> NativeApprovalDecision {
        if case let .askUser(reason) = decision {
            return await requestUserApproval(
                requestID: requestID,
                command: command,
                arguments: arguments,
                cwd: cwd,
                source: source,
                risk: risk,
                reason: reason,
                purpose: purpose,
                approvalScopeKey: approvalScopeKey
            )
        }
        switch decision {
        case let .approve(reason, rememberAllow):
            if rememberAllow, let approvalScopeKey {
                sessionApprovalAllowlist.insert(approvalScopeKey)
            }
            publishApprovalEvent(.init(
                requestID: requestID,
                command: ([command] + arguments).joined(separator: " "),
                purpose: purpose,
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
                purpose: purpose,
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
    }

    func approvalSessionScope(
        base: String,
        command: String,
        arguments: [String],
        cwd: URL,
        projectRoot: URL,
        workspaceID: String?,
        purpose: String? = nil,
        requestedPermissionsDescription: String?,
        reuse: ApprovalSessionReuse
    ) -> String {
        var values = [
            state.user?.id ?? "",
            state.deviceID ?? "",
            workspaceID ?? "",
            projectRoot.standardizedFileURL.resolvingSymlinksInPath().path,
            cwd.standardizedFileURL.resolvingSymlinksInPath().path,
            base,
            purpose ?? "",
            requestedPermissionsDescription ?? "",
        ]
        if reuse == .exactRequest {
            values.append(command)
            values.append(contentsOf: arguments)
        }
        let canonical = values.map { "\($0.utf8.count):\($0)" }.joined()
        let digest = SHA256.hash(data: Data(canonical.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return "v2:\(digest)"
    }

    private func requestUserApproval(
        requestID: String,
        command: String,
        arguments: [String],
        cwd: URL,
        source: String,
        risk: NativeApprovalRisk,
        reason: String?,
        purpose: String?,
        approvalScopeKey: String?
    ) async -> NativeApprovalDecision {
        let id = UUID().uuidString
        pendingApprovals.append(.init(
            id: id,
            requestID: requestID,
            command: ([command] + arguments).joined(separator: " "),
            purpose: purpose,
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
            throw NativeWorkspaceFilesystemError.unsafePath
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

    func sendRelayResponse(
        _ response: NativeRelayResponse,
        socket: URLSessionWebSocketTask
    ) async throws {
        let data = try JSONEncoder().encode(response)
        guard let text = String(data: data, encoding: .utf8) else {
            throw NativeRelayTransportError.invalidResponse
        }
        try await socket.send(.string(text))
    }
}
