// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public enum NativeTerminalHistoryStoreError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case invalidExitCode
    case invalidStoredIdentity
    case invalidStoredState

    public var errorDescription: String? {
        switch self {
        case .invalidOwner: "终端历史不属于当前账户"
        case .invalidExitCode: "终端历史退出码超出有效范围"
        case .invalidStoredIdentity: "终端历史返回了不匹配的记录身份"
        case .invalidStoredState: "终端历史记录内容无效"
        }
    }
}

/// Account-scoped terminal history backed only by the selected Client Storage
/// Provider. This store has no JSON file fallback, migration, or dual-write.
public actor NativeTerminalHistoryStore {
    private struct StoredState: Codable, Sendable {
        var source: String
        var workspaceAlias: String?
        var cwd: String?
        var display: String
        var status: String
        var stdoutPreview: String?
        var stderrPreview: String?
        var error: String?
        var startedAt: String

        private enum CodingKeys: String, CodingKey {
            case source
            case workspaceAlias = "workspace_alias"
            case cwd
            case display
            case status
            case stdoutPreview = "stdout_preview"
            case stderrPreview = "stderr_preview"
            case error
            case startedAt = "started_at"
        }
    }

    private let accountSession: any NativeLocalAgentAccountSessionAccess

    public init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        self.accountSession = accountSession
    }

    @discardableResult
    public func append(
        ownerUserID: String,
        entry: LocalConnectorCommandHistoryEntry,
        projectID: String? = nil
    ) async throws -> LocalConnectorCommandHistoryEntry {
        guard !ownerUserID.isEmpty else { throw NativeTerminalHistoryStoreError.invalidOwner }
        let client = try await accountSession.client(accountID: ownerUserID)
        guard client.ownerUserID == ownerUserID else {
            throw NativeTerminalHistoryStoreError.invalidOwner
        }
        let exitCode: Int32?
        if let value = entry.exitCode {
            guard let converted = Int32(exactly: value) else {
                throw NativeTerminalHistoryStoreError.invalidExitCode
            }
            exitCode = converted
        } else {
            exitCode = nil
        }
        let state = StoredState(
            source: entry.source,
            workspaceAlias: entry.workspaceAlias,
            cwd: entry.cwd,
            display: entry.display,
            status: entry.status,
            stdoutPreview: entry.stdoutPreview,
            stderrPreview: entry.stderrPreview,
            error: entry.error,
            startedAt: entry.startedAt
        )
        let snapshot = try await client.appendTerminalHistory(
            recordID: entry.id,
            draft: .init(
                projectID: projectID,
                terminalSessionID: entry.source,
                command: entry.display,
                exitCode: exitCode,
                state: try Self.encode(state)
            )
        )
        return try Self.entry(from: snapshot, ownerUserID: ownerUserID)
    }

    public func list(
        ownerUserID: String,
        limit: Int
    ) async throws -> [LocalConnectorCommandHistoryEntry] {
        guard !ownerUserID.isEmpty else { throw NativeTerminalHistoryStoreError.invalidOwner }
        let client = try await accountSession.client(accountID: ownerUserID)
        guard client.ownerUserID == ownerUserID else {
            throw NativeTerminalHistoryStoreError.invalidOwner
        }
        let boundedLimit = max(1, min(limit, 200))
        return try await client.terminalHistoryRecords()
            .prefix(boundedLimit)
            .map { try Self.entry(from: $0, ownerUserID: ownerUserID) }
    }

    public func clear(ownerUserID: String) async throws {
        guard !ownerUserID.isEmpty else { throw NativeTerminalHistoryStoreError.invalidOwner }
        let client = try await accountSession.client(accountID: ownerUserID)
        guard client.ownerUserID == ownerUserID else {
            throw NativeTerminalHistoryStoreError.invalidOwner
        }
        try await client.clearTerminalHistory()
    }

    private static func entry(
        from snapshot: LocalAgentTerminalHistorySnapshot,
        ownerUserID: String
    ) throws -> LocalConnectorCommandHistoryEntry {
        guard snapshot.ownerUserID == ownerUserID else {
            throw NativeTerminalHistoryStoreError.invalidStoredIdentity
        }
        let state: StoredState
        do {
            state = try decode(snapshot.draft.state)
        } catch {
            throw NativeTerminalHistoryStoreError.invalidStoredState
        }
        guard snapshot.recordID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false,
              snapshot.draft.terminalSessionID == state.source,
              snapshot.draft.command == state.display
        else {
            throw NativeTerminalHistoryStoreError.invalidStoredIdentity
        }
        return .init(
            id: snapshot.recordID,
            source: state.source,
            workspaceAlias: state.workspaceAlias,
            cwd: state.cwd,
            display: state.display,
            status: state.status,
            exitCode: snapshot.draft.exitCode.map(Int.init),
            stdoutPreview: state.stdoutPreview,
            stderrPreview: state.stderrPreview,
            error: state.error,
            startedAt: state.startedAt
        )
    }

    private static func encode(_ value: StoredState) throws -> LocalAgentJSONValue {
        try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentJSONValue.self,
            from: JSONEncoder().encode(value)
        )
    }

    private static func decode(_ value: LocalAgentJSONValue) throws -> StoredState {
        try JSONDecoder().decode(
            StoredState.self,
            from: LocalAgentProtocolJSON.encoder().encode(value)
        )
    }
}
