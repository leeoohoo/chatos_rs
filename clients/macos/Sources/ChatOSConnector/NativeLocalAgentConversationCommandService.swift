// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public protocol NativeLocalAgentProjectRecordLoading: Sendable {
    func localAgentProjectRecord(
        ownerUserID: String,
        projectID: String
    ) async throws -> LocalProjectRecord
}

public enum NativeLocalAgentConversationCommandError: Error, Equatable, Sendable {
    case missingModelConfiguration
    case invalidCommandIdentity(String)
    case projectMismatch
    case invalidCreatedRun
}

extension NativeLocalAgentConversationCommandError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .missingModelConfiguration:
            "当前会话尚未选择可用模型"
        case let .invalidCommandIdentity(field):
            "本地聊天命令缺少有效的\(field)"
        case .projectMismatch:
            "当前会话的本地项目身份不一致"
        case .invalidCreatedRun:
            "本地 Agent Host 返回的运行身份与当前会话不一致"
        }
    }
}

/// The only Main Chat command boundary on macOS. It resolves all authority
/// from native account/session state, freezes immutable inputs, then creates
/// exactly one durable Rust Runtime Run for each user turn.
public actor NativeLocalAgentConversationCommandService: ConversationCommandServicing {
    private let accountSession: any NativeLocalAgentAccountSessionAccess
    private let scopes: NativeLocalAgentConversationScopeStore
    private let runtimeSettings: any ConversationRuntimeSettingsServicing
    private let contactContexts: any LocalAgentContactRuntimeContextServicing
    private let projects: any NativeLocalAgentProjectRecordLoading
    private let snapshots: NativeLocalAgentMainChatSnapshotFactory

    public init(
        accountSession: any NativeLocalAgentAccountSessionAccess,
        scopes: NativeLocalAgentConversationScopeStore,
        runtimeSettings: any ConversationRuntimeSettingsServicing,
        contactContexts: any LocalAgentContactRuntimeContextServicing,
        projects: any NativeLocalAgentProjectRecordLoading,
        snapshots: NativeLocalAgentMainChatSnapshotFactory = .init()
    ) {
        self.accountSession = accountSession
        self.scopes = scopes
        self.runtimeSettings = runtimeSettings
        self.contactContexts = contactContexts
        self.projects = projects
        self.snapshots = snapshots
    }

    public func sendNewTurn(
        _ command: ConversationSendCommand
    ) async throws -> ConversationCommandAck {
        try validateIdentity(command.sessionID, field: "会话 ID")
        try validateIdentity(command.turnID, field: "轮次 ID")
        try validateIdentity(command.messageID, field: "消息 ID")
        let trimmedContent = command.content.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedContent.isEmpty || !command.attachments.isEmpty else {
            throw NativeLocalAgentConversationCommandError.invalidCommandIdentity("消息内容")
        }

        let scope = try await scopes.scope(conversationID: command.sessionID)
        guard scope.conversationID == command.sessionID else {
            throw NativeLocalAgentConversationCommandError.invalidCreatedRun
        }

        async let settingsValue = runtimeSettings.fetchSettings(sessionID: command.sessionID)
        async let contactValue = contactContext(agentID: scope.contactAgentID)
        async let projectValue = projectRecord(
            ownerUserID: scope.accountID,
            projectID: scope.projectID
        )
        let (settings, contact, project) = try await (
            settingsValue,
            contactValue,
            projectValue
        )
        guard let modelConfigID = settings.selectedModelID?.trimmedNonEmpty else {
            throw NativeLocalAgentConversationCommandError.missingModelConfiguration
        }
        try validateIdentity(modelConfigID, field: "模型配置 ID")
        if let projectID = scope.projectID {
            guard project?.id == projectID, project?.ownerUserID == scope.accountID else {
                throw NativeLocalAgentConversationCommandError.projectMismatch
            }
        } else if project != nil {
            throw NativeLocalAgentConversationCommandError.projectMismatch
        }

        let frozen = try snapshots.make(contact: contact, project: project)
        let references = try await accountSession.stageAttachments(
            command.attachments,
            accountID: scope.accountID
        )
        let created: (operationID: String, run: LocalAgentRunSnapshot)
        do {
            let client = try await accountSession.client(accountID: scope.accountID)
            created = try await client.createMainChatTurn(
                LocalAgentCreateMainChatTurn(
                    threadID: command.sessionID,
                    turnID: command.turnID,
                    messageID: command.messageID,
                    projectID: scope.projectID,
                    modelConfigID: modelConfigID,
                    promptSnapshot: frozen.prompt,
                    capabilitySnapshot: frozen.capabilities,
                    projectSnapshot: frozen.project,
                    content: trimmedContent.isEmpty ? nil : trimmedContent,
                    attachments: references
                )
            )
        } catch {
            await accountSession.discardStagedAttachments(
                references,
                accountID: scope.accountID
            )
            throw error
        }
        guard validIdentity(created.operationID), validIdentity(created.run.runID),
              created.run.profileKey == "main_chat",
              created.run.ownerUserID == scope.accountID,
              created.run.ownerEntityType == "conversation",
              created.run.ownerEntityID == command.sessionID,
              created.run.projectID == scope.projectID,
              created.run.modelConfigID == modelConfigID
        else {
            // A successful creation response transfers ownership of staged
            // grants to the durable Run, even if its identity is invalid.
            throw NativeLocalAgentConversationCommandError.invalidCreatedRun
        }
        return ConversationCommandAck(
            operationID: created.operationID,
            runID: created.run.runID,
            turnID: command.turnID,
            userMessageID: command.messageID
        )
    }

    public func cancelRun(runID: String) async throws {
        try validateIdentity(runID, field: "运行 ID")
        let client = try await accountSession.activeClient()
        _ = try await client.accepted(.cancelRun(runID: runID))
    }

    private func contactContext(
        agentID: String?
    ) async throws -> LocalAgentContactRuntimeContext? {
        guard let agentID else { return nil }
        let context = try await contactContexts.fetchRuntimeContext(agentID: agentID)
        guard context.agentID == agentID else {
            throw NativeLocalAgentMainChatSnapshotError.invalidContactContext
        }
        return context
    }

    private func projectRecord(
        ownerUserID: String,
        projectID: String?
    ) async throws -> LocalProjectRecord? {
        guard let projectID else { return nil }
        return try await projects.localAgentProjectRecord(
            ownerUserID: ownerUserID,
            projectID: projectID
        )
    }

    private func validateIdentity(_ value: String, field: String) throws {
        guard validIdentity(value) else {
            throw NativeLocalAgentConversationCommandError.invalidCommandIdentity(field)
        }
    }

    private func validIdentity(_ value: String) -> Bool {
        !value.isEmpty
            && value.utf8.count <= 512
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let trimmed = trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}
