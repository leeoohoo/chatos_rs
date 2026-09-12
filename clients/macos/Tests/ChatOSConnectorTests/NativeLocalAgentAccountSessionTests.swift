// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

@testable import ChatOSConnector
import Foundation
import Testing

@Suite("Native local Agent account lifecycle")
struct NativeLocalAgentAccountSessionTests {
    @Test("the native app provisions Keychain credentials to the validated Rust Host")
    func launchesRealRustHost() async throws {
        guard let executablePath = ProcessInfo.processInfo.environment[
            "CHATOS_LOCAL_AGENT_HOST_TEST_EXECUTABLE"
        ] else { return }
        let accountID = "swift-host-test-\(UUID().uuidString.lowercased())"
        let root = URL(
            fileURLWithPath: "/tmp/csh-\(UUID().uuidString.prefix(8).lowercased())",
            isDirectory: true
        )
        let executableURL = URL(fileURLWithPath: executablePath)
        let credentials = try NativeLocalAgentCredentialStore()
        let session = NativeLocalAgentAccountSession(
            credentials: credentials,
            supervisor: try NativeLocalAgentHostSupervisor(),
            builder: NativeLocalAgentHostBootstrapBuilder(),
            randomBytes: { Data(repeating: 0x7b, count: $0) }
        )
        func cleanUp() async {
            await session.logout()
            for reference in [
                NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference,
                NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference,
                NativeLocalAgentAccountSession.deviceIDReference,
                NativeLocalAgentAccountSession.sqliteEncryptionKeyReference,
            ] {
                try? await credentials.delete(accountID: accountID, reference: reference)
            }
            try? FileManager.default.removeItem(at: root)
        }

        do {
            try await session.login(
                accountID: accountID,
                accessToken: "process-boundary-token",
                settingsProvider: { deviceID in
                    NativeLocalAgentHostBootstrapSettings(
                        executableURL: executableURL,
                        accountID: accountID,
                        deviceID: deviceID,
                        runtimeDirectory: root.appendingPathComponent("runtime"),
                        attachmentGrantDirectory: root.appendingPathComponent("attachments"),
                        platformStateDirectory: root.appendingPathComponent("state"),
                        modelGatewayBaseURL: URL(string: "https://gateway.example.test")!,
                        memoryEngineBaseURL: URL(string: "https://memory.example.test")!,
                        storage: .sqlite(
                            databaseURL: root.appendingPathComponent("client.sqlite3"),
                            encryptionSecretReference: NativeLocalAgentAccountSession
                                .sqliteEncryptionKeyReference
                        )
                    )
                }
            )
            let client = try await session.client(accountID: accountID)
            let listed = try await client.runs()
            #expect(listed.runs.isEmpty)
        } catch {
            await cleanUp()
            throw error
        }
        await cleanUp()
    }

    @Test("login prepares persistent keys and starts exactly one account Host")
    func loginStartsHost() async throws {
        let credentials = InMemoryLocalAgentCredentials()
        let supervisor = FakeLocalAgentSupervisor()
        let session = NativeLocalAgentAccountSession(
            credentials: credentials,
            supervisor: supervisor,
            builder: FakeLocalAgentBuilder(),
            randomBytes: { Data(repeating: 0x5a, count: $0) }
        )

        try await session.login(
            accountID: "user-1",
            accessToken: "access-token",
            settingsProvider: { self.settings(accountID: "user-1", deviceID: $0) }
        )

        #expect(await supervisor.startedAccounts() == ["user-1"])
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            ) == Data("access-token".utf8)
        )
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference
            )?.count == 32
        )
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentAccountSession.sqliteEncryptionKeyReference
            )?.count == 32
        )
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentAccountSession.deviceIDReference
            ) == Data("device-5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a".utf8)
        )
        _ = try await session.client(accountID: "user-1")
        await #expect(throws: NativeLocalAgentAccountSessionError.accountMismatch) {
            _ = try await session.client(accountID: "user-2")
        }
    }

    @Test("logout stops Host and removes only the replaceable access token")
    func logoutPreservesEncryptionKeys() async throws {
        let credentials = InMemoryLocalAgentCredentials()
        let supervisor = FakeLocalAgentSupervisor()
        let session = NativeLocalAgentAccountSession(
            credentials: credentials,
            supervisor: supervisor,
            builder: FakeLocalAgentBuilder(),
            randomBytes: { Data(repeating: 0x33, count: $0) }
        )
        try await session.login(
            accountID: "user-1",
            accessToken: "access-token",
            settingsProvider: { self.settings(accountID: "user-1", deviceID: $0) }
        )

        await session.logout()

        #expect(await supervisor.currentState() == .stopped)
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            ) == nil
        )
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference
            ) != nil
        )
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentAccountSession.sqliteEncryptionKeyReference
            ) != nil
        )
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentAccountSession.deviceIDReference
            ) != nil
        )
        await #expect(throws: NativeLocalAgentAccountSessionError.inactive) {
            _ = try await session.client(accountID: "user-1")
        }
    }

    @Test("switching accounts stops the old Host and isolates credentials and IPC identity")
    func accountSwitchIsIsolated() async throws {
        let credentials = InMemoryLocalAgentCredentials()
        let supervisor = FakeLocalAgentSupervisor()
        let session = NativeLocalAgentAccountSession(
            credentials: credentials,
            supervisor: supervisor,
            builder: FakeLocalAgentBuilder(),
            randomBytes: { Data(repeating: 0x29, count: $0) }
        )
        try await session.login(
            accountID: "user-1",
            accessToken: "token-1",
            settingsProvider: { self.settings(accountID: "user-1", deviceID: $0) }
        )

        try await session.login(
            accountID: "user-2",
            accessToken: "token-2",
            settingsProvider: { self.settings(accountID: "user-2", deviceID: $0) }
        )

        #expect(await supervisor.startedAccounts() == ["user-1", "user-2"])
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            ) == nil
        )
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference
            ) != nil
        )
        #expect(
            await credentials.value(
                accountID: "user-2",
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            ) == Data("token-2".utf8)
        )
        await #expect(throws: NativeLocalAgentAccountSessionError.accountMismatch) {
            _ = try await session.client(accountID: "user-1")
        }
        _ = try await session.client(accountID: "user-2")
    }

    @Test("a damaged persistent key aborts startup without retaining the access token")
    func rejectsDamagedPersistentKey() async throws {
        let credentials = InMemoryLocalAgentCredentials()
        try await credentials.save(
            Data(repeating: 1, count: 12),
            accountID: "user-1",
            reference: NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference
        )
        let supervisor = FakeLocalAgentSupervisor()
        let session = NativeLocalAgentAccountSession(
            credentials: credentials,
            supervisor: supervisor,
            builder: FakeLocalAgentBuilder(),
            randomBytes: { Data(repeating: 0x44, count: $0) }
        )

        await #expect(
            throws: NativeLocalAgentAccountSessionError.invalidPersistentKey(
                NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference
            )
        ) {
            try await session.login(
                accountID: "user-1",
                accessToken: "access-token",
                settingsProvider: { self.settings(accountID: "user-1", deviceID: $0) }
            )
        }

        #expect(await supervisor.startedAccounts().isEmpty)
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            ) == nil
        )
    }

    @Test("rotating a token restarts the durable Host with the same account settings")
    func tokenRotationRestartsHost() async throws {
        let credentials = InMemoryLocalAgentCredentials()
        let supervisor = FakeLocalAgentSupervisor()
        let session = NativeLocalAgentAccountSession(
            credentials: credentials,
            supervisor: supervisor,
            builder: FakeLocalAgentBuilder(),
            randomBytes: { Data(repeating: 0x66, count: $0) }
        )
        try await session.login(
            accountID: "user-1",
            accessToken: "first-token",
            settingsProvider: { self.settings(accountID: "user-1", deviceID: $0) }
        )

        try await session.updateAccessToken(accountID: "user-1", accessToken: "second-token")

        #expect(await supervisor.startedAccounts() == ["user-1", "user-1"])
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            ) == Data("second-token".utf8)
        )
    }

    private func settings(
        accountID: String,
        deviceID: String
    ) -> NativeLocalAgentHostBootstrapSettings {
        let root = URL(fileURLWithPath: "/tmp/chatos-account-session-tests", isDirectory: true)
        return NativeLocalAgentHostBootstrapSettings(
            executableURL: URL(fileURLWithPath: "/bin/sh"),
            accountID: accountID,
            deviceID: deviceID,
            runtimeDirectory: root.appendingPathComponent("runtime"),
            attachmentGrantDirectory: root.appendingPathComponent("attachments"),
            platformStateDirectory: root.appendingPathComponent("state"),
            modelGatewayBaseURL: URL(string: "https://gateway.example.test")!,
            memoryEngineBaseURL: URL(string: "https://memory.example.test")!,
            storage: .sqlite(
                databaseURL: root.appendingPathComponent("client.sqlite3"),
                encryptionSecretReference: NativeLocalAgentAccountSession.sqliteEncryptionKeyReference
            )
        )
    }
}

private actor InMemoryLocalAgentCredentials: NativeLocalAgentCredentialAccess {
    private var values: [String: Data] = [:]

    func load(accountID: String, reference: String) async throws -> Data? {
        values[key(accountID: accountID, reference: reference)]
    }

    func save(_ secret: Data, accountID: String, reference: String) async throws {
        values[key(accountID: accountID, reference: reference)] = secret
    }

    func delete(accountID: String, reference: String) async throws {
        values.removeValue(forKey: key(accountID: accountID, reference: reference))
    }

    func value(accountID: String, reference: String) -> Data? {
        values[key(accountID: accountID, reference: reference)]
    }

    private func key(accountID: String, reference: String) -> String {
        "\(accountID)\u{0}\(reference)"
    }
}

private actor FakeLocalAgentSupervisor: NativeLocalAgentHostSupervising {
    private var hostState: NativeLocalAgentHostState = .stopped
    private var accounts: [String] = []

    func state() async -> NativeLocalAgentHostState { hostState }

    func stateUpdates() async -> AsyncStream<NativeLocalAgentHostState> {
        let state = hostState
        return AsyncStream { continuation in
            continuation.yield(state)
            continuation.finish()
        }
    }

    func start(
        accountID: String,
        configurationProvider: @escaping @Sendable () async throws
            -> NativeLocalAgentHostLaunchConfiguration
    ) async throws {
        let configuration = try await configurationProvider()
        accounts.append(accountID)
        hostState = .running(
            accountID: accountID,
            processID: UInt32(accounts.count),
            clientEndpoint: configuration.expectedClientEndpoint,
            restartCount: 0
        )
    }

    func logout() async { hostState = .stopped }
    func startedAccounts() -> [String] { accounts }
    func currentState() -> NativeLocalAgentHostState { hostState }
}

private struct FakeLocalAgentBuilder: NativeLocalAgentHostConfigurationBuilding {
    func makeConfiguration(
        settings: NativeLocalAgentHostBootstrapSettings,
        credentialValues: [String: Data]
    ) async throws -> NativeLocalAgentHostLaunchConfiguration {
        try NativeLocalAgentHostLaunchConfiguration(
            executableURL: settings.executableURL,
            launchID: UUID().uuidString,
            expectedClientEndpoint: "/tmp/chatos-session-\(UUID().uuidString).sock",
            launchRequestJSON: Data("{}".utf8),
            secretFrameJSON: Data("{}".utf8)
        )
    }
}
