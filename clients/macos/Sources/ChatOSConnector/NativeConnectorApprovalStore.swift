// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

struct NativeConnectorApprovalPreferences: Codable, Equatable, Sendable {
    var defaultMode: LocalConnectorApprovalMode = .requestApproval
    var commandApprovalModelConfigID: String?
    var commandApprovalThinkingLevel: String?

    static let defaults = NativeConnectorApprovalPreferences()
}

enum NativeConnectorApprovalStoreError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case invalidStoredIdentity
    case invalidStoredMode

    var errorDescription: String? {
        switch self {
        case .invalidOwner: "审批记录不属于当前账户"
        case .invalidStoredIdentity: "审批记录身份无效"
        case .invalidStoredMode: "审批记录包含无效的审批模式"
        }
    }
}

/// Owns account-scoped approval preferences and audit history in the selected
/// Client Storage Provider. There is deliberately no state.json fallback,
/// migration, dual-write, or in-memory approval path before activation.
actor NativeConnectorApprovalStore {
    private struct ActiveState: Sendable {
        var ownerUserID: String
        var preferences: NativeConnectorApprovalPreferences
        var mutation: UInt64
    }

    private static let preferencesKey = "local_connector.approval_preferences"

    private let persistence: NativeLocalClientSettingStore<NativeConnectorApprovalPreferences>
    private let accountSession: any NativeLocalAgentAccountSessionAccess
    private var active: ActiveState?

    init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        self.accountSession = accountSession
        do {
            persistence = try NativeLocalClientSettingStore(
                key: Self.preferencesKey,
                accountSession: accountSession
            )
        } catch {
            preconditionFailure("Local Connector approval preference key is invalid")
        }
    }

    @discardableResult
    func activate(ownerUserID: String) async throws -> NativeConnectorApprovalPreferences {
        active = nil
        await persistence.reset()
        let preferences = try await persistence.load(
            ownerUserID: ownerUserID,
            defaultValue: .defaults
        )
        active = .init(ownerUserID: ownerUserID, preferences: preferences, mutation: 0)
        return preferences
    }

    func deactivate() async {
        active = nil
        await persistence.reset()
    }

    func preferences(ownerUserID: String) throws -> NativeConnectorApprovalPreferences {
        guard let active else { throw NativeLocalClientSettingStoreError.notLoaded }
        guard active.ownerUserID == ownerUserID else {
            throw NativeLocalClientSettingStoreError.accountMismatch
        }
        return active.preferences
    }

    func updateMode(
        ownerUserID: String,
        mode: LocalConnectorApprovalMode
    ) async throws -> NativeConnectorApprovalPreferences {
        var next = try preferences(ownerUserID: ownerUserID)
        next.defaultMode = mode
        return try await persist(ownerUserID: ownerUserID, preferences: next)
    }

    func updateModelSelection(
        ownerUserID: String,
        modelConfigID: String?,
        thinkingLevel: String?
    ) async throws -> NativeConnectorApprovalPreferences {
        var next = try preferences(ownerUserID: ownerUserID)
        next.commandApprovalModelConfigID = modelConfigID
        next.commandApprovalThinkingLevel = thinkingLevel
        return try await persist(ownerUserID: ownerUserID, preferences: next)
    }

    func append(
        ownerUserID: String,
        entry: LocalConnectorApprovalHistoryEntry
    ) async throws -> LocalConnectorApprovalHistoryEntry {
        _ = try preferences(ownerUserID: ownerUserID)
        let client = try await accountSession.client(accountID: ownerUserID)
        guard client.ownerUserID == ownerUserID else {
            throw NativeConnectorApprovalStoreError.invalidOwner
        }
        let snapshot = try await client.appendApprovalHistory(
            recordID: entry.id,
            draft: .init(
                command: entry.command,
                cwd: entry.cwd,
                source: entry.source,
                mode: entry.mode.rawValue,
                decision: entry.decision,
                risk: entry.risk,
                reason: entry.reason
            )
        )
        return try Self.entry(from: snapshot, ownerUserID: ownerUserID)
    }

    func history(ownerUserID: String) async throws -> [LocalConnectorApprovalHistoryEntry] {
        _ = try preferences(ownerUserID: ownerUserID)
        let client = try await accountSession.client(accountID: ownerUserID)
        guard client.ownerUserID == ownerUserID else {
            throw NativeConnectorApprovalStoreError.invalidOwner
        }
        return try await client.approvalHistoryRecords().map {
            try Self.entry(from: $0, ownerUserID: ownerUserID)
        }
    }

    private func persist(
        ownerUserID: String,
        preferences: NativeConnectorApprovalPreferences
    ) async throws -> NativeConnectorApprovalPreferences {
        guard var active else { throw NativeLocalClientSettingStoreError.notLoaded }
        guard active.ownerUserID == ownerUserID else {
            throw NativeLocalClientSettingStoreError.accountMismatch
        }
        active.mutation &+= 1
        let mutation = active.mutation
        active.preferences = preferences
        self.active = active
        do {
            _ = try await persistence.saveLatest(
                ownerUserID: ownerUserID,
                value: preferences,
                mutation: mutation
            )
        } catch {
            if self.active?.ownerUserID == ownerUserID,
               self.active?.mutation == mutation {
                self.active = nil
                await persistence.reset()
            }
            throw error
        }
        return try self.preferences(ownerUserID: ownerUserID)
    }

    private static func entry(
        from snapshot: LocalAgentApprovalHistorySnapshot,
        ownerUserID: String
    ) throws -> LocalConnectorApprovalHistoryEntry {
        guard snapshot.ownerUserID == ownerUserID,
              !snapshot.recordID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        else {
            throw NativeConnectorApprovalStoreError.invalidStoredIdentity
        }
        guard let mode = LocalConnectorApprovalMode(rawValue: snapshot.draft.mode) else {
            throw NativeConnectorApprovalStoreError.invalidStoredMode
        }
        return .init(
            id: snapshot.recordID,
            command: snapshot.draft.command,
            cwd: snapshot.draft.cwd,
            source: snapshot.draft.source,
            mode: mode,
            decision: snapshot.draft.decision,
            risk: snapshot.draft.risk,
            reason: snapshot.draft.reason,
            createdAt: snapshot.createdAt
        )
    }
}
