// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

struct NativeInstalledPluginRecord: Codable, Sendable, Equatable {
    var pluginID: String
    var releaseID: String
    var version: String
    var artifactSHA256: String
    var installationPath: String
    var installedAt: String
    var pluginKey: String? = nil
}

enum NativePluginStateStoreError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case duplicatePluginID
    case invalidStoredIdentity
    case notInstalled

    var errorDescription: String? {
        switch self {
        case .invalidOwner: "Plugin 安装状态不属于当前账户"
        case .duplicatePluginID: "Plugin 安装状态包含重复身份"
        case .invalidStoredIdentity: "Plugin 安装状态身份无效"
        case .notInstalled: "Plugin 尚未安装"
        }
    }
}

struct NativePluginInstallationState: Equatable, Sendable {
    var record: NativeInstalledPluginRecord
    var enabled: Bool
}

/// The only macOS projection of installed Plugin state. Installation metadata,
/// Release identity and enablement live in the selected Client Storage
/// Provider through PluginStateRepository. No state.json migration, fallback,
/// or dual-write exists.
actor NativePluginStateStore {
    private struct ActiveState: Sendable {
        var ownerUserID: String
        var snapshots: [String: LocalAgentInstalledPluginSnapshot]
    }

    private let accountSession: any NativeLocalAgentAccountSessionAccess
    private var active: ActiveState?

    init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        self.accountSession = accountSession
    }

    func activate(ownerUserID: String) async throws {
        active = nil
        let client = try await client(ownerUserID: ownerUserID)
        let records = try await client.installedPluginRecords()
        var snapshots: [String: LocalAgentInstalledPluginSnapshot] = [:]
        for snapshot in records {
            try Self.validate(snapshot, ownerUserID: ownerUserID)
            guard snapshots.updateValue(snapshot, forKey: snapshot.draft.pluginID) == nil else {
                throw NativePluginStateStoreError.duplicatePluginID
            }
        }
        active = .init(ownerUserID: ownerUserID, snapshots: snapshots)
    }

    func deactivate() {
        active = nil
    }

    func records(ownerUserID: String) throws -> [String: NativeInstalledPluginRecord] {
        let state = try requireActive(ownerUserID: ownerUserID)
        return try state.snapshots.mapValues { try Self.installation(from: $0) }
    }

    func installations(ownerUserID: String) throws -> [String: NativePluginInstallationState] {
        let state = try requireActive(ownerUserID: ownerUserID)
        return try state.snapshots.mapValues { snapshot in
            .init(
                record: try Self.installation(from: snapshot),
                enabled: snapshot.draft.enabled
            )
        }
    }

    func record(ownerUserID: String, pluginID: String) throws -> NativeInstalledPluginRecord? {
        let state = try requireActive(ownerUserID: ownerUserID)
        guard let snapshot = state.snapshots[pluginID] else { return nil }
        return try Self.installation(from: snapshot)
    }

    func isEnabled(ownerUserID: String, pluginID: String, defaultValue: Bool = true) throws -> Bool {
        let state = try requireActive(ownerUserID: ownerUserID)
        return state.snapshots[pluginID]?.draft.enabled ?? defaultValue
    }

    @discardableResult
    func put(
        ownerUserID: String,
        record: NativeInstalledPluginRecord,
        enabled: Bool = true
    ) async throws -> NativeInstalledPluginRecord {
        var state = try requireActive(ownerUserID: ownerUserID)
        let current = state.snapshots[record.pluginID]
        let client = try await client(ownerUserID: ownerUserID)
        do {
            let snapshot = try await client.putInstalledPlugin(
                expectedRevision: current?.revision,
                draft: .init(
                    pluginID: record.pluginID,
                    release: record.releaseID,
                    enabled: enabled,
                    installation: try Self.encode(record)
                )
            )
            try Self.validate(snapshot, ownerUserID: ownerUserID)
            state.snapshots[record.pluginID] = snapshot
            active = state
            return try Self.installation(from: snapshot)
        } catch {
            active = nil
            throw error
        }
    }

    func setEnabled(ownerUserID: String, pluginID: String, enabled: Bool) async throws {
        let record = try record(ownerUserID: ownerUserID, pluginID: pluginID)
        guard let record else { throw NativePluginStateStoreError.notInstalled }
        _ = try await put(ownerUserID: ownerUserID, record: record, enabled: enabled)
    }

    func remove(ownerUserID: String, pluginID: String) async throws {
        var state = try requireActive(ownerUserID: ownerUserID)
        guard let snapshot = state.snapshots[pluginID] else { return }
        let client = try await client(ownerUserID: ownerUserID)
        do {
            try await client.deleteInstalledPlugin(
                pluginID: pluginID,
                expectedRevision: snapshot.revision
            )
            state.snapshots[pluginID] = nil
            active = state
        } catch {
            active = nil
            throw error
        }
    }

    private func requireActive(ownerUserID: String) throws -> ActiveState {
        guard let active else { throw NativeLocalClientSettingStoreError.notLoaded }
        guard active.ownerUserID == ownerUserID else {
            throw NativeLocalClientSettingStoreError.accountMismatch
        }
        return active
    }

    private func client(ownerUserID: String) async throws -> NativeLocalAgentIPCClient {
        guard !ownerUserID.isEmpty else { throw NativePluginStateStoreError.invalidOwner }
        let client = try await accountSession.client(accountID: ownerUserID)
        guard client.ownerUserID == ownerUserID else {
            throw NativePluginStateStoreError.invalidOwner
        }
        return client
    }

    private static func validate(
        _ snapshot: LocalAgentInstalledPluginSnapshot,
        ownerUserID: String
    ) throws {
        guard snapshot.ownerUserID == ownerUserID,
              !snapshot.recordID.isEmpty,
              snapshot.revision > 0,
              !snapshot.draft.pluginID.isEmpty,
              !snapshot.draft.release.isEmpty
        else {
            throw NativePluginStateStoreError.invalidStoredIdentity
        }
        let record = try installation(from: snapshot)
        guard record.pluginID == snapshot.draft.pluginID,
              record.releaseID == snapshot.draft.release else {
            throw NativePluginStateStoreError.invalidStoredIdentity
        }
    }

    private static func encode(_ value: NativeInstalledPluginRecord) throws -> LocalAgentJSONValue {
        try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentJSONValue.self,
            from: JSONEncoder().encode(value)
        )
    }

    private static func installation(
        from snapshot: LocalAgentInstalledPluginSnapshot
    ) throws -> NativeInstalledPluginRecord {
        try JSONDecoder().decode(
            NativeInstalledPluginRecord.self,
            from: LocalAgentProtocolJSON.encoder().encode(snapshot.draft.installation)
        )
    }
}
