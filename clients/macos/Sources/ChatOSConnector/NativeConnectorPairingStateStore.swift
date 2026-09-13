// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

struct NativeConnectorPairingState: Codable, Equatable, Sendable {
    var user: LocalConnectorUser?
    var deviceID: String?
    var deviceName: String?
    var workspaces: [LocalConnectorWorkspace] = []
    /// `false` means the user explicitly blocked server-to-client calls.
    var gatewayConnectionEnabled: Bool?

    static let empty = NativeConnectorPairingState()
}

enum NativeConnectorPairingStateStoreError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case accountMismatch
    case notLoaded
    case invalidStoredState

    var errorDescription: String? {
        switch self {
        case .invalidOwner: "本机连接器配对账户无效"
        case .accountMismatch: "本机连接器配对状态不属于当前账户"
        case .notLoaded: "本机连接器配对状态尚未从权威存储加载"
        case .invalidStoredState: "本机连接器配对状态无效"
        }
    }
}

/// The only persistent source for Local Connector pairing and workspace state.
/// The selected Client Storage Provider owns the value; there is deliberately
/// no state.json migration, filesystem fallback, or dual-write path.
actor NativeConnectorPairingStateStore {
    private static let key = "local_connector.pairing"

    private struct ActiveState: Sendable {
        var ownerUserID: String
        var value: NativeConnectorPairingState
        var mutation: UInt64
    }

    private let persistence: NativeLocalClientSettingStore<NativeConnectorPairingState>
    private var active: ActiveState?

    init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        do {
            persistence = try NativeLocalClientSettingStore(
                key: Self.key,
                accountSession: accountSession
            )
        } catch {
            preconditionFailure("Local Connector pairing key is invalid")
        }
    }

    func activate(ownerUserID: String) async throws -> NativeConnectorPairingState {
        guard !ownerUserID.isEmpty else {
            throw NativeConnectorPairingStateStoreError.invalidOwner
        }
        active = nil
        await persistence.reset()
        let value = try await persistence.load(ownerUserID: ownerUserID, defaultValue: .empty)
        try Self.validate(value, ownerUserID: ownerUserID)
        active = .init(ownerUserID: ownerUserID, value: value, mutation: 0)
        return value
    }

    func deactivate() async {
        active = nil
        await persistence.reset()
    }

    func value(ownerUserID: String) throws -> NativeConnectorPairingState {
        guard let active else { throw NativeConnectorPairingStateStoreError.notLoaded }
        guard active.ownerUserID == ownerUserID else {
            throw NativeConnectorPairingStateStoreError.accountMismatch
        }
        return active.value
    }

    @discardableResult
    func save(
        ownerUserID: String,
        value: NativeConnectorPairingState
    ) async throws -> NativeConnectorPairingState {
        try Self.validate(value, ownerUserID: ownerUserID)
        guard var active else { throw NativeConnectorPairingStateStoreError.notLoaded }
        guard active.ownerUserID == ownerUserID else {
            throw NativeConnectorPairingStateStoreError.accountMismatch
        }
        active.mutation &+= 1
        let mutation = active.mutation
        active.value = value
        self.active = active
        do {
            _ = try await persistence.saveLatest(
                ownerUserID: ownerUserID,
                value: value,
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
        guard let current = self.active else {
            throw NativeConnectorPairingStateStoreError.notLoaded
        }
        return current.value
    }

    private static func validate(
        _ value: NativeConnectorPairingState,
        ownerUserID: String
    ) throws {
        guard !ownerUserID.isEmpty else {
            throw NativeConnectorPairingStateStoreError.invalidOwner
        }
        guard let deviceID = value.deviceID else {
            guard value.user == nil,
                  value.deviceName == nil,
                  value.workspaces.isEmpty,
                  value.gatewayConnectionEnabled == nil else {
                throw NativeConnectorPairingStateStoreError.invalidStoredState
            }
            return
        }
        guard value.user?.id == ownerUserID else {
            throw NativeConnectorPairingStateStoreError.accountMismatch
        }
        let deviceName = value.deviceName?.trimmingCharacters(in: .whitespacesAndNewlines)
        let workspaceIDs = value.workspaces.map(\.id)
        guard !deviceID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              deviceName?.isEmpty == false,
              value.gatewayConnectionEnabled != nil,
              !value.workspaces.isEmpty,
              Set(workspaceIDs).count == workspaceIDs.count,
              value.workspaces.allSatisfy({ workspace in
                  !workspace.id.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                      && !workspace.alias.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                      && workspace.absoluteRoot.hasPrefix("/")
                      && !workspace.fingerprint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
              }) else {
            throw NativeConnectorPairingStateStoreError.invalidStoredState
        }
    }
}
