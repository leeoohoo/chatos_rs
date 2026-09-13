// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public enum NativeLocalClientSettingStoreError: LocalizedError, Equatable, Sendable {
    case invalidKey
    case accountMismatch
    case notLoaded
    case invalidStoredIdentity

    public var errorDescription: String? {
        switch self {
        case .invalidKey:
            "客户端设置键无效"
        case .accountMismatch:
            "客户端设置不属于当前账户"
        case .notLoaded:
            "客户端设置尚未从权威存储加载"
        case .invalidStoredIdentity:
            "客户端设置返回了不匹配的记录身份"
        }
    }
}

/// A typed, account-scoped façade over the Local Agent Host's
/// `ClientSettingRepository`. It deliberately owns no file/UserDefaults
/// fallback: an unavailable selected provider remains an explicit error.
public actor NativeLocalClientSettingStore<Value: Codable & Sendable> {
    private struct Cache: Sendable {
        var ownerUserID: String
        var value: Value
        var revision: UInt64?
        var highestObservedMutation: UInt64
    }

    private let key: String
    private let accountSession: any NativeLocalAgentAccountSessionAccess
    private var cache: Cache?
    private var writeTail: Task<Void, Never>?

    public init(
        key: String,
        accountSession: any NativeLocalAgentAccountSessionAccess
    ) throws {
        let normalized = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty, normalized == key else {
            throw NativeLocalClientSettingStoreError.invalidKey
        }
        self.key = key
        self.accountSession = accountSession
    }

    public func load(ownerUserID: String, defaultValue: Value) async throws -> Value {
        let client = try await accountSession.client(accountID: ownerUserID)
        guard client.ownerUserID == ownerUserID else {
            throw NativeLocalClientSettingStoreError.accountMismatch
        }
        if let cache, cache.ownerUserID == ownerUserID {
            return cache.value
        }

        do {
            let snapshot = try await client.clientSetting(key: key)
            guard snapshot.ownerUserID == ownerUserID, snapshot.key == key else {
                throw NativeLocalClientSettingStoreError.invalidStoredIdentity
            }
            let value = try Self.decode(snapshot.value)
            cache = Cache(
                ownerUserID: ownerUserID,
                value: value,
                revision: snapshot.revision,
                highestObservedMutation: 0
            )
            return value
        } catch NativeLocalAgentIPCError.rejected(let rejection)
            where rejection.code == "client_setting_not_found" {
            cache = Cache(
                ownerUserID: ownerUserID,
                value: defaultValue,
                revision: nil,
                highestObservedMutation: 0
            )
            return defaultValue
        }
    }

    /// Persists only the newest mutation observed by this typed store. A late
    /// task carrying an older mutation number is ignored before it can replace
    /// a newer value. Returns `true` only when a provider write was committed.
    public func saveLatest(
        ownerUserID: String,
        value: Value,
        mutation: UInt64
    ) async throws -> Bool {
        guard var current = cache else {
            throw NativeLocalClientSettingStoreError.notLoaded
        }
        guard current.ownerUserID == ownerUserID else {
            throw NativeLocalClientSettingStoreError.accountMismatch
        }
        guard mutation > current.highestObservedMutation else { return false }
        current.highestObservedMutation = mutation
        cache = current

        let predecessor = writeTail
        let operation = Task { [self] in
            await predecessor?.value
            return try await commitLatest(
                ownerUserID: ownerUserID,
                value: value,
                mutation: mutation
            )
        }
        writeTail = Task {
            _ = try? await operation.value
        }
        return try await operation.value
    }

    public func reset() async {
        await writeTail?.value
        writeTail = nil
        cache = nil
    }

    private func commitLatest(
        ownerUserID: String,
        value: Value,
        mutation: UInt64
    ) async throws -> Bool {
        guard let current = cache else {
            throw NativeLocalClientSettingStoreError.notLoaded
        }
        guard current.ownerUserID == ownerUserID else {
            throw NativeLocalClientSettingStoreError.accountMismatch
        }
        guard mutation == current.highestObservedMutation else { return false }

        let client = try await accountSession.client(accountID: ownerUserID)
        guard client.ownerUserID == ownerUserID else {
            throw NativeLocalClientSettingStoreError.accountMismatch
        }
        let snapshot = try await client.putClientSetting(
            key: key,
            expectedRevision: current.revision,
            value: try Self.encode(value)
        )
        guard snapshot.ownerUserID == ownerUserID, snapshot.key == key else {
            throw NativeLocalClientSettingStoreError.invalidStoredIdentity
        }
        guard var latest = cache, latest.ownerUserID == ownerUserID else {
            throw NativeLocalClientSettingStoreError.accountMismatch
        }
        latest.value = value
        latest.revision = snapshot.revision
        cache = latest
        return true
    }

    private static func encode(_ value: Value) throws -> LocalAgentJSONValue {
        try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentJSONValue.self,
            from: JSONEncoder().encode(value)
        )
    }

    private static func decode(_ value: LocalAgentJSONValue) throws -> Value {
        try JSONDecoder().decode(
            Value.self,
            from: LocalAgentProtocolJSON.encoder().encode(value)
        )
    }
}
