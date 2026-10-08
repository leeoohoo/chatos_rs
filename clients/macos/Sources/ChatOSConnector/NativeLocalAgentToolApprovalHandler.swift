import ChatOSCore
import Foundation

protocol NativeLocalAgentToolApprovalHandling: Sendable {
    func resolveNextPending(ownerUserID: String) async throws -> Bool
}

struct NativeLocalAgentToolApprovalHandler: NativeLocalAgentToolApprovalHandling, Sendable {
    private let client: NativeLocalAgentToolClient
    private let contextResolver: NativeLocalAgentProjectContextResolver
    private let connector: NativeLocalConnectorService
    private let reviewerID: String

    init(
        host: any LocalAgentHostClientServicing,
        projects: NativeLocalProjectsService,
        connector: NativeLocalConnectorService,
        reviewerID: String = "macos-local-approval"
    ) {
        client = .init(host: host)
        contextResolver = .init(host: host, projects: projects, connector: connector)
        self.connector = connector
        self.reviewerID = reviewerID
    }

    func resolveNextPending(ownerUserID: String) async throws -> Bool {
        let pending = try await client.pendingApprovals(ownerUserID: ownerUserID)
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
        let presentation = Self.presentation(invocation)
        let decision = await connector.approvalDecision(
            requestID: invocation.invocationID,
            command: presentation.command,
            arguments: presentation.arguments,
            cwd: approvalScope.rootURL,
            projectRoot: approvalScope.rootURL,
            source: "Local Agent Task",
            risk: presentation.risk,
            requestedPermissionsDescription: presentation.permission,
            approvalScopeKey: presentation.scope + ":\(context.conversationID)",
            workspaceID: approvalScope.workspaceID
        )
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
        _ = try await client.decideApproval(
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

    private static func presentation(
        _ invocation: LocalAgentToolInvocationRecord
    ) -> (
        command: String,
        arguments: [String],
        risk: NativeApprovalRisk,
        permission: String,
        scope: String
    ) {
        if invocation.toolName == "execute_command",
           case let .object(arguments) = invocation.arguments {
            let command = NativeLocalAgentTerminalCommandResolver.resolve(arguments) ?? ""
            let shellArguments = ["-lc", command]
            return (
                "/bin/zsh",
                shellArguments,
                NativeApprovalRiskEvaluator.evaluate(
                    command: "/bin/zsh",
                    arguments: shellArguments
                ),
                "在当前本地项目中执行命令",
                "local-agent-terminal"
            )
        }
        if invocation.toolName == "process_write" {
            return (
                "process_write",
                ["interactive input hidden"],
                .init(level: "medium", reason: "本地任务请求向运行中的命令写入输入。"),
                "向当前任务启动的本地命令写入输入",
                "local-agent-terminal"
            )
        }
        if invocation.toolName == "process_kill" {
            return (
                "process_kill",
                ["task-owned process"],
                .init(level: "medium", reason: "本地任务请求终止运行中的命令。"),
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
                "向选定的远程连接上传文件",
                "local-agent-remote-upload"
            )
        }
        return (
            "commit_edit_session",
            ["commit staged project edits"],
            .init(level: "medium", reason: "本地任务请求提交已暂存的项目文件修改。"),
            "修改当前本地项目中的已暂存文件",
            "local-agent-project-write"
        )
    }

    private static func string(_ value: LocalAgentJSONValue?) -> String? {
        guard case let .string(value)? = value else { return nil }
        return value
    }
}
