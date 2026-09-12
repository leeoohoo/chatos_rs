// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation
import Security

public enum NativeLocalAgentAccountSessionError: Error, Equatable, Sendable {
    case invalidAccount
    case invalidAccessToken
    case invalidDeviceID
    case invalidPersistentKey(String)
    case credentialUnavailable(String)
    case inactive
    case accountMismatch
    case hostUnavailable
}

extension NativeLocalAgentAccountSessionError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .invalidAccount: "本地 Agent 账户身份无效"
        case .invalidAccessToken: "本地 Agent 登录凭据无效"
        case .invalidDeviceID: "本地 Agent 设备身份无效"
        case let .invalidPersistentKey(reference):
            "本地 Agent 持久密钥损坏（\(reference)）"
        case let .credentialUnavailable(reference):
            "本地 Agent 安全凭据不可用（\(reference)）"
        case .inactive: "本地 Agent 尚未随当前账户启动"
        case .accountMismatch: "本地 Agent 当前账户与请求账户不一致"
        case .hostUnavailable: "本地 Agent Host 尚未就绪"
        }
    }
}

protocol NativeLocalAgentCredentialAccess: Sendable {
    func load(accountID: String, reference: String) async throws -> Data?
    func save(_ secret: Data, accountID: String, reference: String) async throws
    func delete(accountID: String, reference: String) async throws
}

extension NativeLocalAgentCredentialStore: NativeLocalAgentCredentialAccess {}

protocol NativeLocalAgentHostSupervising: Sendable {
    func state() async -> NativeLocalAgentHostState
    func start(
        accountID: String,
        configurationProvider: @escaping @Sendable () async throws
            -> NativeLocalAgentHostLaunchConfiguration
    ) async throws
    func logout() async
}

extension NativeLocalAgentHostSupervisor: NativeLocalAgentHostSupervising {}

protocol NativeLocalAgentHostConfigurationBuilding: Sendable {
    func makeConfiguration(
        settings: NativeLocalAgentHostBootstrapSettings,
        credentialValues: [String: Data]
    ) async throws -> NativeLocalAgentHostLaunchConfiguration
}

extension NativeLocalAgentHostBootstrapBuilder: NativeLocalAgentHostConfigurationBuilding {}

/// Owns the one authenticated macOS Local Agent Host session.
///
/// The controller is the only native boundary allowed to persist account
/// credentials in Keychain, provision one launch's secrets to the validated
/// Host, start or stop the process, and resolve the current IPC endpoint after
/// an automatic Host restart.
public actor NativeLocalAgentAccountSession {
    public typealias SettingsProvider = @Sendable (String) async throws
        -> NativeLocalAgentHostBootstrapSettings

    public static let deviceIDReference = "device-id"
    public static let sqliteEncryptionKeyReference = "sqlite-encryption-key"

    private let credentials: any NativeLocalAgentCredentialAccess
    private let supervisor: any NativeLocalAgentHostSupervising
    private let builder: any NativeLocalAgentHostConfigurationBuilding
    private let randomBytes: @Sendable (Int) throws -> Data
    private var activeAccountID: String?
    private var activeSettings: NativeLocalAgentHostBootstrapSettings?
    private let attachmentStager = NativeLocalAgentAttachmentStager()

    public init() throws {
        self.credentials = try NativeLocalAgentCredentialStore()
        self.supervisor = try NativeLocalAgentHostSupervisor()
        self.builder = NativeLocalAgentHostBootstrapBuilder()
        self.randomBytes = SelfSecureRandom.bytes(count:)
    }

    init(
        credentials: any NativeLocalAgentCredentialAccess,
        supervisor: any NativeLocalAgentHostSupervising,
        builder: any NativeLocalAgentHostConfigurationBuilding,
        randomBytes: @escaping @Sendable (Int) throws -> Data
    ) {
        self.credentials = credentials
        self.supervisor = supervisor
        self.builder = builder
        self.randomBytes = randomBytes
    }

    public func login(
        accountID: String,
        accessToken: String,
        settingsProvider: SettingsProvider
    ) async throws {
        guard Self.validIdentity(accountID) else {
            throw NativeLocalAgentAccountSessionError.invalidAccount
        }
        guard Self.validSecret(accessToken) else {
            throw NativeLocalAgentAccountSessionError.invalidAccessToken
        }

        if let previousAccountID = activeAccountID, previousAccountID != accountID {
            await stop(accountID: previousAccountID)
        } else {
            await supervisor.logout()
        }

        do {
            let deviceID = try await persistentDeviceID(accountID: accountID)
            let settings = try await settingsProvider(deviceID)
            guard settings.accountID == accountID else {
                throw NativeLocalAgentAccountSessionError.invalidAccount
            }
            guard settings.deviceID == deviceID else {
                throw NativeLocalAgentAccountSessionError.invalidDeviceID
            }
            try await credentials.save(
                Data(accessToken.utf8),
                accountID: accountID,
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            )
            try await ensurePersistentKey(
                accountID: accountID,
                reference: NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference
            )
            if case let .sqlite(_, encryptionSecretReference) = settings.storage {
                try await ensurePersistentKey(
                    accountID: accountID,
                    reference: encryptionSecretReference
                )
            }

            let builder = self.builder
            let credentials = self.credentials
            try await supervisor.start(accountID: accountID) {
                let values = try await Self.loadCredentialValues(
                    from: credentials,
                    accountID: accountID,
                    storage: settings.storage
                )
                return try await builder.makeConfiguration(
                    settings: settings,
                    credentialValues: values
                )
            }
            activeAccountID = accountID
            activeSettings = settings
            _ = try await client(accountID: accountID)
        } catch {
            await supervisor.logout()
            try? await credentials.delete(
                accountID: accountID,
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            )
            activeAccountID = nil
            activeSettings = nil
            throw error
        }
    }

    /// Replaces an account token by restarting the Host so no process keeps
    /// using the superseded token in memory. Durable Runs resume from storage.
    public func updateAccessToken(accountID: String, accessToken: String) async throws {
        guard activeAccountID == accountID else {
            throw NativeLocalAgentAccountSessionError.accountMismatch
        }
        guard let activeSettings else {
            throw NativeLocalAgentAccountSessionError.inactive
        }
        guard Self.validSecret(accessToken) else {
            throw NativeLocalAgentAccountSessionError.invalidAccessToken
        }
        try await credentials.save(
            Data(accessToken.utf8),
            accountID: accountID,
            reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
        )
        let builder = self.builder
        let credentials = self.credentials
        do {
            try await supervisor.start(accountID: accountID) {
                let values = try await Self.loadCredentialValues(
                    from: credentials,
                    accountID: accountID,
                    storage: activeSettings.storage
                )
                return try await builder.makeConfiguration(
                    settings: activeSettings,
                    credentialValues: values
                )
            }
            _ = try await client(accountID: accountID)
        } catch {
            try? await credentials.delete(
                accountID: accountID,
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            )
            self.activeAccountID = nil
            self.activeSettings = nil
            throw error
        }
    }

    public func logout() async {
        guard let accountID = activeAccountID else {
            await supervisor.logout()
            return
        }
        await stop(accountID: accountID)
    }

    public func state() async -> NativeLocalAgentHostState {
        await supervisor.state()
    }

    public func client(accountID: String) async throws -> NativeLocalAgentIPCClient {
        guard let activeAccountID else {
            throw NativeLocalAgentAccountSessionError.inactive
        }
        guard activeAccountID == accountID else {
            throw NativeLocalAgentAccountSessionError.accountMismatch
        }
        guard case let .running(runningAccountID, _, endpoint, _) = await supervisor.state(),
              runningAccountID == accountID
        else {
            throw NativeLocalAgentAccountSessionError.hostUnavailable
        }
        let transport = try NativeLocalAgentUnixTransport(socketPath: endpoint)
        return try NativeLocalAgentIPCClient(ownerUserID: accountID, transport: transport)
    }

    public func stageAttachments(
        _ attachments: [ConversationAttachmentDraft],
        accountID: String
    ) throws -> [LocalAgentAttachmentReference] {
        guard let activeAccountID else {
            throw NativeLocalAgentAccountSessionError.inactive
        }
        guard activeAccountID == accountID else {
            throw NativeLocalAgentAccountSessionError.accountMismatch
        }
        guard let activeSettings else {
            throw NativeLocalAgentAccountSessionError.inactive
        }
        return try attachmentStager.stage(
            attachments,
            in: activeSettings.attachmentGrantDirectory
        )
    }

    private func stop(accountID: String) async {
        activeAccountID = nil
        activeSettings = nil
        await supervisor.logout()
        try? await credentials.delete(
            accountID: accountID,
            reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
        )
    }

    private func ensurePersistentKey(accountID: String, reference: String) async throws {
        if let existing = try await credentials.load(accountID: accountID, reference: reference) {
            guard existing.count == 32 else {
                throw NativeLocalAgentAccountSessionError.invalidPersistentKey(reference)
            }
            return
        }
        let generated = try randomBytes(32)
        guard generated.count == 32 else {
            throw NativeLocalAgentAccountSessionError.invalidPersistentKey(reference)
        }
        try await credentials.save(generated, accountID: accountID, reference: reference)
    }

    private func persistentDeviceID(accountID: String) async throws -> String {
        if let existing = try await credentials.load(
            accountID: accountID,
            reference: Self.deviceIDReference
        ) {
            let deviceID = String(decoding: existing, as: UTF8.self)
            guard Self.validDeviceID(deviceID) else {
                throw NativeLocalAgentAccountSessionError.invalidDeviceID
            }
            return deviceID
        }
        let entropy = try randomBytes(16)
        guard entropy.count == 16 else {
            throw NativeLocalAgentAccountSessionError.invalidDeviceID
        }
        let deviceID = "device-" + entropy.map { String(format: "%02x", $0) }.joined()
        try await credentials.save(
            Data(deviceID.utf8),
            accountID: accountID,
            reference: Self.deviceIDReference
        )
        return deviceID
    }

    private static func loadCredentialValues(
        from credentials: any NativeLocalAgentCredentialAccess,
        accountID: String,
        storage: NativeLocalAgentStorageBootstrap
    ) async throws -> [String: Data] {
        let storageReference: String
        switch storage {
        case let .sqlite(_, encryptionSecretReference):
            storageReference = encryptionSecretReference
        case let .postgres(connectionSecretReference):
            storageReference = connectionSecretReference
        }
        let references = Set([
            NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference,
            NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference,
            storageReference,
        ])
        var values: [String: Data] = [:]
        for reference in references {
            guard let value = try await credentials.load(
                accountID: accountID,
                reference: reference
            ), !value.isEmpty, value.count <= 64 * 1_024 else {
                throw NativeLocalAgentAccountSessionError.credentialUnavailable(reference)
            }
            values[reference] = value
        }
        guard values[NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference]?.count == 32
        else {
            throw NativeLocalAgentAccountSessionError.invalidPersistentKey(
                NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference
            )
        }
        if case .sqlite = storage, values[storageReference]?.count != 32 {
            throw NativeLocalAgentAccountSessionError.invalidPersistentKey(storageReference)
        }
        guard let token = values[NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference],
              String(data: token, encoding: .utf8).map(validSecret) == true
        else {
            throw NativeLocalAgentAccountSessionError.invalidAccessToken
        }
        return values
    }

    private static func validIdentity(_ value: String) -> Bool {
        !value.isEmpty
            && value.count <= 512
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }

    private static func validSecret(_ value: String) -> Bool {
        validIdentity(value) && value.utf8.count <= 64 * 1_024
    }

    private static func validDeviceID(_ value: String) -> Bool {
        guard value.count == 39, value.hasPrefix("device-") else { return false }
        return value.dropFirst("device-".count).unicodeScalars.allSatisfy { scalar in
            (48...57).contains(scalar.value) || (97...102).contains(scalar.value)
        }
    }
}

private enum SelfSecureRandom {
    static func bytes(count: Int) throws -> Data {
        guard count > 0 else {
            throw NativeLocalAgentAccountSessionError.invalidPersistentKey("random")
        }
        var data = Data(count: count)
        let status = data.withUnsafeMutableBytes { buffer in
            SecRandomCopyBytes(kSecRandomDefault, count, buffer.baseAddress!)
        }
        guard status == errSecSuccess else {
            throw NativeLocalAgentCredentialStoreError.keychain(
                operation: "secure-random",
                status: status
            )
        }
        return data
    }
}
