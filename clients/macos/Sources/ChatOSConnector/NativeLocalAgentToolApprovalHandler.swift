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
        }) else { return false }
        let context = try await contextResolver.resolve(
            ownerUserID: ownerUserID,
            runID: invocation.runID
        )
        let decision = await connector.approvalDecision(
            requestID: invocation.invocationID,
            command: invocation.toolName,
            arguments: Self.safeArgumentSummary(invocation),
            cwd: context.resolvedPath.absoluteURL,
            projectRoot: context.resolvedPath.absoluteURL,
            source: "Local Agent Task",
            risk: .init(
                level: "medium",
                reason: "本地任务请求提交已暂存的项目文件修改。"
            ),
            requestedPermissionsDescription: "修改当前本地项目中的已暂存文件",
            approvalScopeKey: "local-agent-project-write:\(context.conversationID)",
            workspaceID: context.resolvedPath.workspace.id
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

    private static func safeArgumentSummary(
        _ invocation: LocalAgentToolInvocationRecord
    ) -> [String] {
        guard invocation.toolName == "commit_edit_session" else {
            return ["project-bound operation"]
        }
        return ["commit staged project edits"]
    }
}
