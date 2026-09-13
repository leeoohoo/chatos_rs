// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSAgentRuntime

/// Account-scoped Agent preferences backed only by the selected Client Storage
/// Provider. Missing records use current defaults; unavailable storage fails
/// explicitly and never falls back to platform preferences or a local file.
public actor NativeAgentRuntimeSettingsStore: AgentRuntimePreferencesProviding {
    private static let key = "agent_runtime.preferences"

    private let persistence: NativeLocalClientSettingStore<AgentRuntimePreferences>
    private var mutation: UInt64 = 0

    public init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        do {
            persistence = try NativeLocalClientSettingStore(
                key: Self.key,
                accountSession: accountSession
            )
        } catch {
            preconditionFailure("Agent Runtime Client Setting key is invalid")
        }
    }

    public func load(ownerUserID: String) async throws -> AgentRuntimePreferences {
        let value = try await persistence.load(
            ownerUserID: ownerUserID,
            defaultValue: .init()
        )
        try value.validate()
        return value
    }

    public func save(
        ownerUserID: String,
        preferences: AgentRuntimePreferences
    ) async throws {
        try preferences.validate()
        _ = try await load(ownerUserID: ownerUserID)
        mutation &+= 1
        _ = try await persistence.saveLatest(
            ownerUserID: ownerUserID,
            value: preferences,
            mutation: mutation
        )
    }

    public func deactivate() async {
        mutation = 0
        await persistence.reset()
    }
}
