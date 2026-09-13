// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

actor NativeLocalProjectRunPreferencesStore {
    private static let settingKey = "project_run.preferences"

    private let accountSession: any NativeLocalAgentAccountSessionAccess
    private var preferences = NativeProjectRunPreferences()
    private var revision: UInt64?
    private var loaded = false
    private var loadTask: Task<(NativeProjectRunPreferences, UInt64?), Error>?

    init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        self.accountSession = accountSession
    }

    func selection(projectID: String) async throws -> NativeProjectRunSelection {
        try await ensureLoaded()
        return preferences.projects[projectID] ?? .init()
    }

    func save(projectID: String, selection: NativeProjectRunSelection) async throws {
        try await ensureLoaded()
        var updated = preferences
        updated.projects[projectID] = selection
        let client = try await accountSession.activeClient()
        let snapshot = try await client.putClientSetting(
            key: Self.settingKey,
            expectedRevision: revision,
            value: try Self.encode(updated)
        )
        preferences = updated
        revision = snapshot.revision
    }

    private func ensureLoaded() async throws {
        guard !loaded else { return }
        let task: Task<(NativeProjectRunPreferences, UInt64?), Error>
        if let loadTask {
            task = loadTask
        } else {
            let accountSession = accountSession
            task = Task {
                let client = try await accountSession.activeClient()
                do {
                    let snapshot = try await client.clientSetting(key: Self.settingKey)
                    return (try Self.decode(snapshot.value), snapshot.revision)
                } catch NativeLocalAgentIPCError.rejected(let rejection)
                    where rejection.code == "client_setting_not_found" {
                    return (.init(), nil)
                }
            }
            loadTask = task
        }
        do {
            let result = try await task.value
            preferences = result.0
            revision = result.1
            loaded = true
            loadTask = nil
        } catch {
            loadTask = nil
            throw error
        }
    }

    private static func encode(
        _ preferences: NativeProjectRunPreferences
    ) throws -> LocalAgentJSONValue {
        try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentJSONValue.self,
            from: JSONEncoder().encode(preferences)
        )
    }

    private static func decode(
        _ value: LocalAgentJSONValue
    ) throws -> NativeProjectRunPreferences {
        try JSONDecoder().decode(
            NativeProjectRunPreferences.self,
            from: LocalAgentProtocolJSON.encoder().encode(value)
        )
    }
}

struct NativeProjectRunPreferences: Codable, Sendable {
    var projects: [String: NativeProjectRunSelection] = [:]
}

struct NativeProjectRunSelection: Codable, Sendable, Equatable {
    var defaultTargetID: String?
    var selectedToolchains: [String: String] = [:]
    var customToolchains: [String: ProjectRunCustomToolchain] = [:]
    var environmentVariables: [String: String] = [:]
}
