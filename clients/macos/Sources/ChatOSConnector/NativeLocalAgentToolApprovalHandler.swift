import ChatOSCore
import Foundation

protocol NativeLocalAgentToolApprovalHandling: Sendable {
    func resolveNextPending(ownerUserID: String) async throws -> Bool
}

protocol NativeLocalAgentDurableApprovalServicing: Sendable {
    func pendingApprovals(
        ownerUserID: String
    ) async throws -> [LocalAgentToolInvocationRecord]

    func decideApproval(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord,
        approve: Bool,
        decidedBy: String,
        reason: String
    ) async throws
}

private struct NativeLocalAgentDurableApprovalPort: NativeLocalAgentDurableApprovalServicing {
    let client: NativeLocalAgentToolClient

    func pendingApprovals(
        ownerUserID: String
    ) async throws -> [LocalAgentToolInvocationRecord] {
        try await client.pendingApprovals(ownerUserID: ownerUserID)
    }

    func decideApproval(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord,
        approve: Bool,
        decidedBy: String,
        reason: String
    ) async throws {
        _ = try await client.decideApproval(
            ownerUserID: ownerUserID,
            invocation: invocation,
            approve: approve,
            decidedBy: decidedBy,
            reason: reason
        )
    }
}

protocol NativeLocalAgentApprovalContextResolving: Sendable {
    func resolve(
        ownerUserID: String,
        runID: String
    ) async throws -> NativeLocalAgentTaskExecutionContext
}

extension NativeLocalAgentProjectContextResolver: NativeLocalAgentApprovalContextResolving {}

struct NativeLocalHumanApprovalRequest: Sendable {
    let requestID: String
    let command: String
    let arguments: [String]
    let cwd: URL
    let projectRoot: URL
    let source: String
    let risk: NativeApprovalRisk
    let purpose: String?
    let requestedPermissionsDescription: String
    let approvalScopeKey: String
    let workspaceID: String
    let evaluationArguments: [String]?
    let automaticApprovalEvidenceComplete: Bool
}

protocol NativeLocalHumanApprovalServicing: Sendable {
    func decideHumanApproval(
        _ request: NativeLocalHumanApprovalRequest
    ) async -> NativeApprovalDecision
}

extension NativeLocalConnectorService: NativeLocalHumanApprovalServicing {
    func decideHumanApproval(
        _ request: NativeLocalHumanApprovalRequest
    ) async -> NativeApprovalDecision {
        await approvalDecision(
            requestID: request.requestID,
            command: request.command,
            arguments: request.arguments,
            cwd: request.cwd,
            projectRoot: request.projectRoot,
            source: request.source,
            risk: request.risk,
            purpose: request.purpose,
            requestedPermissionsDescription: request.requestedPermissionsDescription,
            approvalScopeKey: request.approvalScopeKey,
            evaluationArguments: request.evaluationArguments,
            automaticApprovalEvidenceComplete: request.automaticApprovalEvidenceComplete,
            workspaceID: request.workspaceID
        )
    }
}

struct NativeLocalAgentToolApprovalHandler: NativeLocalAgentToolApprovalHandling, Sendable {
    private let durableApprovals: any NativeLocalAgentDurableApprovalServicing
    private let contextResolver: any NativeLocalAgentApprovalContextResolving
    private let humanApprovals: any NativeLocalHumanApprovalServicing
    private let editEvidenceProvider: (any NativeMCPCodeWriteApprovalEvidenceProviding)?
    private let decisionJournal: NativeLocalAgentApprovalDecisionJournal
    private let reviewerID: String

    init(
        host: any LocalAgentHostClientServicing,
        projects: NativeLocalProjectsService,
        connector: NativeLocalConnectorService,
        editEvidenceProvider: (any NativeMCPCodeWriteApprovalEvidenceProviding)? = nil,
        decisionJournalURL: URL? = nil,
        reviewerID: String = "macos-local-approval"
    ) {
        durableApprovals = NativeLocalAgentDurableApprovalPort(
            client: .init(host: host)
        )
        contextResolver = NativeLocalAgentProjectContextResolver(
            host: host,
            projects: projects,
            connector: connector
        )
        humanApprovals = connector
        self.editEvidenceProvider = editEvidenceProvider
        decisionJournal = .init(fileURL: decisionJournalURL)
        self.reviewerID = reviewerID
    }

    init(
        durableApprovals: any NativeLocalAgentDurableApprovalServicing,
        contextResolver: any NativeLocalAgentApprovalContextResolving,
        humanApprovals: any NativeLocalHumanApprovalServicing,
        editEvidenceProvider: (any NativeMCPCodeWriteApprovalEvidenceProviding)? = nil,
        decisionJournal: NativeLocalAgentApprovalDecisionJournal = .init(),
        reviewerID: String = "macos-local-approval"
    ) {
        self.durableApprovals = durableApprovals
        self.contextResolver = contextResolver
        self.humanApprovals = humanApprovals
        self.editEvidenceProvider = editEvidenceProvider
        self.decisionJournal = decisionJournal
        self.reviewerID = reviewerID
    }

    func resolveNextPending(ownerUserID: String) async throws -> Bool {
        let pending = try await durableApprovals.pendingApprovals(ownerUserID: ownerUserID)
        guard let invocation = pending.first(where: {
            NativeMCPCodeWriteStore.toolNames.contains($0.toolName)
                || NativeLocalAgentPlatformToolCatalog.taskExecutionTerminalToolNames.contains(
                    $0.toolName
                )
                || [
                    "remote_connection_controller_run_command",
                    "remote_connection_controller_upload_file",
                    "remote_connection_controller_upload_project_file",
                ].contains($0.toolName)
        }) else { return false }
        if let recorded = await decisionJournal.decision(
            ownerUserID: ownerUserID,
            invocation: invocation
        ) {
            try await commit(
                recorded,
                ownerUserID: ownerUserID,
                invocation: invocation,
                recordBeforeWriteback: false
            )
            return true
        }
        let context = try await contextResolver.resolve(
            ownerUserID: ownerUserID,
            runID: invocation.runID
        )
        let approvalScope = Self.approvalScope(for: context)
        guard let presentation = Self.presentation(invocation) else {
            try await commit(
                writebackDecision(
                    ownerUserID: ownerUserID,
                    invocation: invocation,
                    approve: false,
                    reason: "终端命令缺少有效的运行目的，请重新创建任务。"
                ),
                ownerUserID: ownerUserID,
                invocation: invocation
            )
            return true
        }
        let cwd: URL
        do {
            cwd = try Self.approvalWorkingDirectory(
                for: invocation,
                projectRoot: approvalScope.rootURL
            )
        } catch {
            try await commit(
                writebackDecision(
                    ownerUserID: ownerUserID,
                    invocation: invocation,
                    approve: false,
                    reason: "终端命令工作目录无效或超出当前项目范围。"
                ),
                ownerUserID: ownerUserID,
                invocation: invocation
            )
            return true
        }
        let editEvidence: NativeMCPCodeWriteApprovalEvidence?
        if invocation.toolName == "commit_edit_session" {
            if let editEvidenceProvider, case let .object(arguments) = invocation.arguments {
                do {
                    editEvidence = try await editEvidenceProvider.approvalEvidence(
                        arguments: arguments.mapValues(NativeJSONValue.init(local:)),
                        scope: .init(
                            workspaceID: context.workspaceScopeID,
                            sessionID: context.conversationID,
                            runID: invocation.runID
                        ),
                        projectRoot: approvalScope.rootURL,
                        ownedPaths: context.toolAuthorization.ownedPaths
                    )
                } catch {
                    editEvidence = Self.incompleteEditEvidence(
                        "宿主无法读取或重新校验本次暂存编辑：\(error.localizedDescription)"
                    )
                }
            } else {
                editEvidence = Self.incompleteEditEvidence(
                    "宿主未取得可验证的暂存编辑内容"
                )
            }
        } else {
            editEvidence = nil
        }
        let decision = await humanApprovals.decideHumanApproval(.init(
            requestID: invocation.invocationID,
            command: presentation.command,
            arguments: presentation.arguments,
            cwd: cwd,
            projectRoot: approvalScope.rootURL,
            source: "Task",
            risk: presentation.risk,
            purpose: editEvidence?.purpose ?? presentation.purpose,
            requestedPermissionsDescription: presentation.permission,
            approvalScopeKey: presentation.scope + ":\(context.conversationID)",
            workspaceID: approvalScope.workspaceID,
            evaluationArguments: editEvidence.map { [$0.evaluationSummary] }
                ?? presentation.evaluationArguments,
            automaticApprovalEvidenceComplete: editEvidence?.isComplete
                ?? presentation.automaticApprovalEvidenceComplete
        ))
        let approved: Bool
        let reason: String
        switch decision {
        case let .approve(value, _):
            approved = true
            reason = value
        case let .deny(value), let .askUser(value):
            approved = false
            reason = value
        }
        try await commit(
            writebackDecision(
                ownerUserID: ownerUserID,
                invocation: invocation,
                approve: approved,
                reason: String(reason.prefix(4_000))
            ),
            ownerUserID: ownerUserID,
            invocation: invocation
        )
        return true
    }

    private func writebackDecision(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord,
        approve: Bool,
        reason: String
    ) -> NativeLocalAgentApprovalWritebackDecision {
        .init(
            ownerUserID: ownerUserID,
            invocationID: invocation.invocationID,
            invocationVersion: invocation.version,
            approve: approve,
            decidedBy: reviewerID,
            reason: reason,
            recordedAtUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
        )
    }

    private func commit(
        _ decision: NativeLocalAgentApprovalWritebackDecision,
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord,
        recordBeforeWriteback: Bool = true
    ) async throws {
        // Record the Human/policy result before touching Host durable state. If the IPC write
        // fails, reconciliation replays this exact identity/version instead of asking again.
        if recordBeforeWriteback {
            try await decisionJournal.record(decision)
        }
        try await durableApprovals.decideApproval(
            ownerUserID: ownerUserID,
            invocation: invocation,
            approve: decision.approve,
            decidedBy: decision.decidedBy,
            reason: decision.reason
        )
        // Host is now authoritative. A cleanup failure only leaves a bounded stale record;
        // it cannot be applied to a different invocation version.
        try? await decisionJournal.remove(decision)
    }

    static func approvalScope(
        for context: NativeLocalAgentTaskExecutionContext
    ) -> (rootURL: URL, workspaceID: String) {
        // Project and direct-contact tasks both execute inside a host-bound root. Requiring a
        // project here strands contact tasks before their selected Plugin can ever run.
        (context.executionRootURL, context.workspaceScopeID)
    }

    static func approvalWorkingDirectory(
        for invocation: LocalAgentToolInvocationRecord,
        projectRoot: URL
    ) throws -> URL {
        guard invocation.toolName == "execute_command" else { return projectRoot }
        guard case let .object(arguments) = invocation.arguments else {
            throw NativeLocalAgentPlatformToolError.invalidField("path")
        }
        return try NativeLocalAgentExecutionDirectoryResolver.resolve(
            string(arguments["path"]) ?? ".",
            projectRoot: projectRoot
        )
    }

    private static func presentation(
        _ invocation: LocalAgentToolInvocationRecord
    ) -> (
        command: String,
        arguments: [String],
        risk: NativeApprovalRisk,
        purpose: String?,
        permission: String,
        scope: String,
        evaluationArguments: [String]?,
        automaticApprovalEvidenceComplete: Bool
    )? {
        if invocation.toolName == "execute_command" {
            guard case let .object(arguments) = invocation.arguments else { return nil }
            let command = NativeLocalAgentTerminalCommandResolver.resolve(arguments) ?? ""
            guard let purpose = NativeLocalAgentTerminalCommandResolver.purpose(arguments) else {
                return nil
            }
            let shellArguments = ["-lc", command]
            let evaluation = automaticApprovalEvidence(
                arguments: shellArguments,
                operation: "shell 命令"
            )
            return (
                "/bin/zsh",
                shellArguments,
                NativeApprovalRiskEvaluator.evaluate(
                    command: "/bin/zsh",
                    arguments: shellArguments
                ),
                purpose,
                "在当前项目中执行命令",
                "local-agent-terminal",
                evaluation.arguments,
                evaluation.isComplete
            )
        }
        if invocation.toolName == "process_write" {
            return (
                "process_write",
                ["interactive input hidden"],
                .init(level: "medium", reason: "任务请求向运行中的命令写入输入。"),
                nil,
                "向当前任务启动的命令写入输入",
                "local-agent-terminal",
                [incompleteOperationEvidence("标准输入内容被宿主隐藏")],
                false
            )
        }
        if invocation.toolName == "process_kill" {
            let evaluation = automaticApprovalEvidence(
                arguments: [NativeJSONValue(local: invocation.arguments).canonicalJSONString],
                operation: "进程终止参数"
            )
            return (
                "process_kill",
                ["task-owned process"],
                .init(level: "medium", reason: "任务请求终止运行中的命令。"),
                nil,
                "终止当前任务启动的命令",
                "local-agent-terminal",
                evaluation.arguments,
                evaluation.isComplete
            )
        }
        if invocation.toolName == "remote_connection_controller_run_command" {
            let command: String
            if case .object(let arguments) = invocation.arguments {
                command = string(arguments["command"]) ?? ""
            } else {
                command = ""
            }
            let evaluation = automaticApprovalEvidence(
                arguments: [NativeJSONValue(local: invocation.arguments).canonicalJSONString],
                operation: "远程 SSH 命令"
            )
            return (
                "remote SSH command",
                // Approval is a security boundary. Never truncate executable text:
                // doing so can split a pipeline, quote, redirection, or compound
                // command and make both the model and user review a different
                // operation from the one the Host will actually execute.
                [command],
                .init(level: "high", reason: "任务请求在已保存的远程连接上执行命令。"),
                nil,
                "在选定的远程连接上执行命令",
                "local-agent-remote-command",
                evaluation.arguments,
                evaluation.isComplete
            )
        }
        if [
            "remote_connection_controller_upload_file",
            "remote_connection_controller_upload_project_file",
        ].contains(invocation.toolName) {
            let path: String
            let localPath: String?
            if case .object(let arguments) = invocation.arguments {
                path = string(arguments["path"]) ?? ""
                localPath = string(arguments["local_path"])
            } else {
                path = ""
                localPath = nil
            }
            return (
                "remote file upload",
                [localPath.map { "\($0) -> \(path)" } ?? path],
                .init(level: "high", reason: "任务请求向已保存的远程连接写入文件。"),
                nil,
                "向选定的远程连接上传文件",
                "local-agent-remote-upload",
                [incompleteOperationEvidence(
                    localPath == nil
                        ? "远程上传内容未提供给审批模型"
                        : "远程上传的项目文件内容未提供给审批模型"
                )],
                false
            )
        }
        return (
            "commit_edit_session",
            ["commit staged project edits"],
            .init(level: "medium", reason: "任务请求提交已暂存的项目文件修改。"),
            nil,
            "修改当前项目中的已暂存文件",
            "local-agent-project-write",
            [incompleteOperationEvidence("暂存编辑内容尚未由宿主验证")],
            false
        )
    }

    private static let maximumAutomaticApprovalArgumentCharacters = 160_000

    private static func automaticApprovalEvidence(
        arguments: [String],
        operation: String
    ) -> (arguments: [String], isComplete: Bool) {
        var total = 0
        for argument in arguments {
            let (next, overflow) = total.addingReportingOverflow(argument.count)
            guard !overflow, next <= maximumAutomaticApprovalArgumentCharacters else {
                return (
                    [incompleteOperationEvidence(
                        "完整\(operation)超过审批上下文上限 \(maximumAutomaticApprovalArgumentCharacters) 字符"
                    )],
                    false
                )
            }
            total = next
        }
        return (arguments, true)
    }

    private static func incompleteOperationEvidence(_ reason: String) -> String {
        """
        EVIDENCE_COMPLETE=false
        \(reason)。AI 自动审批不得猜测或批准，必须转人工确认。
        """
    }

    private static func incompleteEditEvidence(
        _ reason: String
    ) -> NativeMCPCodeWriteApprovalEvidence {
        .init(
            evaluationSummary: """
            EVIDENCE_COMPLETE=false
            \(reason)。AI 自动审批不得猜测或批准，必须转人工确认并明确显示此原因。
            """,
            purpose: nil,
            isComplete: false
        )
    }

    private static func string(_ value: LocalAgentJSONValue?) -> String? {
        guard case let .string(value)? = value else { return nil }
        return value
    }
}
