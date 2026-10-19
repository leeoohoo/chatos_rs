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
            workspaceID: request.workspaceID
        )
    }
}

struct NativeLocalAgentToolApprovalHandler: NativeLocalAgentToolApprovalHandling, Sendable {
    private let durableApprovals: any NativeLocalAgentDurableApprovalServicing
    private let contextResolver: any NativeLocalAgentApprovalContextResolving
    private let humanApprovals: any NativeLocalHumanApprovalServicing
    private let reviewerID: String

    init(
        host: any LocalAgentHostClientServicing,
        projects: NativeLocalProjectsService,
        connector: NativeLocalConnectorService,
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
        self.reviewerID = reviewerID
    }

    init(
        durableApprovals: any NativeLocalAgentDurableApprovalServicing,
        contextResolver: any NativeLocalAgentApprovalContextResolving,
        humanApprovals: any NativeLocalHumanApprovalServicing,
        reviewerID: String = "macos-local-approval"
    ) {
        self.durableApprovals = durableApprovals
        self.contextResolver = contextResolver
        self.humanApprovals = humanApprovals
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
                ].contains($0.toolName)
        }) else { return false }
        let context = try await contextResolver.resolve(
            ownerUserID: ownerUserID,
            runID: invocation.runID
        )
        let approvalScope = Self.approvalScope(for: context)
        guard let presentation = Self.presentation(invocation) else {
            try await durableApprovals.decideApproval(
                ownerUserID: ownerUserID,
                invocation: invocation,
                approve: false,
                decidedBy: reviewerID,
                reason: "终端命令缺少有效的运行目的，请重新创建任务。"
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
            try await durableApprovals.decideApproval(
                ownerUserID: ownerUserID,
                invocation: invocation,
                approve: false,
                decidedBy: reviewerID,
                reason: "终端命令工作目录无效或超出当前项目范围。"
            )
            return true
        }
        let decision = await humanApprovals.decideHumanApproval(.init(
            requestID: invocation.invocationID,
            command: presentation.command,
            arguments: presentation.arguments,
            cwd: cwd,
            projectRoot: approvalScope.rootURL,
            source: "Local Agent Task",
            risk: presentation.risk,
            purpose: presentation.purpose,
            requestedPermissionsDescription: presentation.permission,
            approvalScopeKey: presentation.scope + ":\(context.conversationID)",
            workspaceID: approvalScope.workspaceID
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
        try await durableApprovals.decideApproval(
            ownerUserID: ownerUserID,
            invocation: invocation,
            approve: approved,
            decidedBy: reviewerID,
            reason: String(reason.prefix(4_000))
        )
        return true
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
        scope: String
    )? {
        if invocation.toolName == "execute_command" {
            guard case let .object(arguments) = invocation.arguments else { return nil }
            let command = NativeLocalAgentTerminalCommandResolver.resolve(arguments) ?? ""
            guard let purpose = NativeLocalAgentTerminalCommandResolver.purpose(arguments) else {
                return nil
            }
            let shellArguments = ["-lc", command]
            return (
                "/bin/zsh",
                shellArguments,
                NativeApprovalRiskEvaluator.evaluate(
                    command: "/bin/zsh",
                    arguments: shellArguments
                ),
                purpose,
                "在当前本地项目中执行命令",
                "local-agent-terminal"
            )
        }
        if invocation.toolName == "process_write" {
            return (
                "process_write",
                ["interactive input hidden"],
                .init(level: "medium", reason: "本地任务请求向运行中的命令写入输入。"),
                nil,
                "向当前任务启动的本地命令写入输入",
                "local-agent-terminal"
            )
        }
        if invocation.toolName == "process_kill" {
            return (
                "process_kill",
                ["task-owned process"],
                .init(level: "medium", reason: "本地任务请求终止运行中的命令。"),
                nil,
                "终止当前任务启动的本地命令",
                "local-agent-terminal"
            )
        }
        if invocation.toolName == "remote_connection_controller_run_command" {
            let command: String
            if case .object(let arguments) = invocation.arguments {
                command = string(arguments["command"]) ?? ""
            } else {
                command = ""
            }
            return (
                "remote SSH command",
                [String(command.prefix(500))],
                .init(level: "high", reason: "本地任务请求在已保存的远程连接上执行命令。"),
                nil,
                "在选定的远程连接上执行命令",
                "local-agent-remote-command"
            )
        }
        if invocation.toolName == "remote_connection_controller_upload_file" {
            let path: String
            if case .object(let arguments) = invocation.arguments {
                path = string(arguments["path"]) ?? ""
            } else {
                path = ""
            }
            return (
                "remote file upload",
                [String(path.prefix(500))],
                .init(level: "high", reason: "本地任务请求向已保存的远程连接写入文件。"),
                nil,
                "向选定的远程连接上传文件",
                "local-agent-remote-upload"
            )
        }
        return (
            "commit_edit_session",
            ["commit staged project edits"],
            .init(level: "medium", reason: "本地任务请求提交已暂存的项目文件修改。"),
            nil,
            "修改当前本地项目中的已暂存文件",
            "local-agent-project-write"
        )
    }

    private static func string(_ value: LocalAgentJSONValue?) -> String? {
        guard case let .string(value)? = value else { return nil }
        return value
    }
}
