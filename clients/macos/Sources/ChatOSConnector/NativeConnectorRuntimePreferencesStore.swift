// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation

struct NativeConnectorRuntimePreferences: Codable, Equatable, Sendable {
    var developerMode = false
    var sandboxEnabled = true
    var permissionProfileID = ":workspace-write"
    var approvalPolicy = "per_call"
    var approvalReviewer = "user"
    var networkAccess = "restricted"
    var policyRevision: String?

    static let defaults = NativeConnectorRuntimePreferences()
}

/// Owns the account-scoped Local Connector runtime preferences stored by the
/// selected Client Storage Provider. The optimistic cache exists only after an
/// authoritative account activation and is discarded after any latest-write
/// failure, so callers cannot silently continue with an uncommitted value.
actor NativeConnectorRuntimePreferencesStore {
    private struct ActiveState: Sendable {
        var ownerUserID: String
        var value: NativeConnectorRuntimePreferences
        var mutation: UInt64
    }

    private static let key = "local_connector.runtime_preferences"

    private let persistence: NativeLocalClientSettingStore<NativeConnectorRuntimePreferences>
    private var active: ActiveState?

    init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        do {
            persistence = try NativeLocalClientSettingStore(
                key: Self.key,
                accountSession: accountSession
            )
        } catch {
            preconditionFailure("Local Connector runtime preference key is invalid")
        }
    }

    @discardableResult
    func activate(ownerUserID: String) async throws -> NativeConnectorRuntimePreferences {
        active = nil
        await persistence.reset()
        let value = try await persistence.load(
            ownerUserID: ownerUserID,
            defaultValue: .defaults
        )
        active = ActiveState(ownerUserID: ownerUserID, value: value, mutation: 0)
        return value
    }

    func deactivate() async {
        active = nil
        await persistence.reset()
    }

    func value(ownerUserID: String) throws -> NativeConnectorRuntimePreferences {
        guard let active else {
            throw NativeLocalClientSettingStoreError.notLoaded
        }
        guard active.ownerUserID == ownerUserID else {
            throw NativeLocalClientSettingStoreError.accountMismatch
        }
        return active.value
    }

    func updateDeveloperMode(
        ownerUserID: String,
        enabled: Bool
    ) async throws -> NativeConnectorRuntimePreferences {
        var next = try value(ownerUserID: ownerUserID)
        next.developerMode = enabled
        return try await persist(ownerUserID: ownerUserID, value: next)
    }

    func updateSandbox(
        ownerUserID: String,
        enabled: Bool?,
        permissionProfileID: String?,
        approvalPolicy: String?,
        approvalReviewer: String?,
        networkAccess: String?
    ) async throws -> NativeConnectorRuntimePreferences {
        var next = try value(ownerUserID: ownerUserID)
        if let enabled { next.sandboxEnabled = enabled }
        if let permissionProfileID { next.permissionProfileID = permissionProfileID }
        if let approvalPolicy { next.approvalPolicy = approvalPolicy }
        if let approvalReviewer { next.approvalReviewer = approvalReviewer }
        if let networkAccess { next.networkAccess = networkAccess }
        next.policyRevision = "native-\(ISO8601DateFormatter().string(from: Date()))"
        return try await persist(ownerUserID: ownerUserID, value: next)
    }

    private func persist(
        ownerUserID: String,
        value: NativeConnectorRuntimePreferences
    ) async throws -> NativeConnectorRuntimePreferences {
        guard var active else {
            throw NativeLocalClientSettingStoreError.notLoaded
        }
        guard active.ownerUserID == ownerUserID else {
            throw NativeLocalClientSettingStoreError.accountMismatch
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

        guard let latest = self.active else {
            throw NativeLocalClientSettingStoreError.notLoaded
        }
        guard latest.ownerUserID == ownerUserID else {
            throw NativeLocalClientSettingStoreError.accountMismatch
        }
        return latest.value
    }
}
