// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

@testable import ChatOSConnector
import Foundation
import Testing

@Suite("Native local Agent account lifecycle")
struct NativeLocalAgentAccountSessionTests {
    @Test("the native app provisions secure credentials to the validated Rust Host")
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
        let credentials = InMemoryLocalAgentCredentials()
        let processVerifier: @Sendable (pid_t) throws -> Void = { _ in }
        let session = NativeLocalAgentAccountSession(
            credentials: credentials,
            supervisor: try NativeLocalAgentHostSupervisor(
                launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
                attacher: NativeLocalAgentHostAttacher(
                    testingIdentityVerifier: processVerifier
                )
            ),
            builder: NativeLocalAgentHostBootstrapBuilder(),
            randomBytes: { Data(repeating: 0x7b, count: $0) },
            clientFactory: { accountID, endpoint in
                let transport = try NativeLocalAgentUnixTransport(
                    socketPath: endpoint,
                    peerIdentityVerifier: processVerifier
                )
                return try NativeLocalAgentIPCClient(
                    ownerUserID: accountID,
                    transport: transport
                )
            }
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
                        pluginManagementBaseURL: URL(string: "https://plugins.example.test")!,
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

    @Test("login accepts server access tokens longer than an account identity")
    func loginAcceptsLongServerToken() async throws {
        let credentials = InMemoryLocalAgentCredentials()
        let supervisor = FakeLocalAgentSupervisor()
        let session = NativeLocalAgentAccountSession(
            credentials: credentials,
            supervisor: supervisor,
            builder: FakeLocalAgentBuilder(),
            randomBytes: { Data(repeating: 0x5a, count: $0) }
        )
        let accessToken = String(repeating: "t", count: 684)

        try await session.login(
            accountID: "user-1",
            accessToken: accessToken,
            settingsProvider: { self.settings(accountID: "user-1", deviceID: $0) }
        )

        #expect(await supervisor.startedAccounts() == ["user-1"])
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            ) == Data(accessToken.utf8)
        )
    }

    @Test("login rejects malformed or oversized server access tokens")
    func loginRejectsUnsafeServerTokens() async throws {
        for accessToken in [
            " token",
            "token\n",
            "token\u{0000}value",
            String(repeating: "t", count: 64 * 1_024 + 1),
        ] {
            let session = NativeLocalAgentAccountSession(
                credentials: InMemoryLocalAgentCredentials(),
                supervisor: FakeLocalAgentSupervisor(),
                builder: FakeLocalAgentBuilder(),
                randomBytes: { Data(repeating: 0x5a, count: $0) }
            )
            await #expect(throws: NativeLocalAgentAccountSessionError.invalidAccessToken) {
                try await session.login(
                    accountID: "user-1",
                    accessToken: accessToken,
                    settingsProvider: { self.settings(accountID: "user-1", deviceID: $0) }
                )
            }
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

    @Test("closing the UI detaches without revoking the account Host token")
    func applicationExitDetachesWithoutLogout() async throws {
        let credentials = InMemoryLocalAgentCredentials()
        let supervisor = FakeLocalAgentSupervisor()
        let session = NativeLocalAgentAccountSession(
            credentials: credentials,
            supervisor: supervisor,
            builder: FakeLocalAgentBuilder(),
            randomBytes: { Data(repeating: 0x31, count: $0) }
        )
        try await session.login(
            accountID: "user-1",
            accessToken: "access-token",
            settingsProvider: { self.settings(accountID: "user-1", deviceID: $0) }
        )

        await session.detach()

        #expect(await supervisor.detachCount() == 1)
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            ) == Data("access-token".utf8)
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

    @Test("rotating a token updates the running Host without changing its PID")
    func tokenRotationKeepsHostRunning() async throws {
        let credentials = InMemoryLocalAgentCredentials()
        let supervisor = FakeLocalAgentSupervisor()
        let updates = AccessTokenUpdateRecorder()
        let session = NativeLocalAgentAccountSession(
            credentials: credentials,
            supervisor: supervisor,
            builder: FakeLocalAgentBuilder(),
            randomBytes: { Data(repeating: 0x66, count: $0) },
            accessTokenUpdater: { accountID, endpoint, accessToken in
                await updates.record(
                    accountID: accountID,
                    endpoint: endpoint,
                    accessToken: accessToken
                )
            }
        )
        try await session.login(
            accountID: "user-1",
            accessToken: "first-token",
            settingsProvider: { self.settings(accountID: "user-1", deviceID: $0) }
        )

        let stateBefore = await supervisor.currentState()
        try await session.updateAccessToken(accountID: "user-1", accessToken: "second-token")
        let stateAfter = await supervisor.currentState()

        #expect(await supervisor.startedAccounts() == ["user-1"])
        #expect(stateBefore == stateAfter)
        #expect(await updates.values().count == 2)
        #expect(await updates.values().last?.accountID == "user-1")
        #expect(await updates.values().last?.accessToken == "second-token")
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            ) == Data("second-token".utf8)
        )
    }

    @Test("a failed token handoff stops the Host and clears the active session")
    func tokenRotationFailureFailsClosed() async throws {
        let credentials = InMemoryLocalAgentCredentials()
        let supervisor = FakeLocalAgentSupervisor()
        let session = NativeLocalAgentAccountSession(
            credentials: credentials,
            supervisor: supervisor,
            builder: FakeLocalAgentBuilder(),
            randomBytes: { Data(repeating: 0x66, count: $0) },
            accessTokenUpdater: { _, _, token in
                if token == "second-token" { throw TokenUpdateFailure.rejected }
            }
        )
        try await session.login(
            accountID: "user-1",
            accessToken: "first-token",
            settingsProvider: { self.settings(accountID: "user-1", deviceID: $0) }
        )

        await #expect(throws: TokenUpdateFailure.rejected) {
            try await session.updateAccessToken(
                accountID: "user-1",
                accessToken: "second-token"
            )
        }

        #expect(await supervisor.currentState() == .stopped)
        #expect(
            await credentials.value(
                accountID: "user-1",
                reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
            ) == nil
        )
        await #expect(throws: NativeLocalAgentAccountSessionError.inactive) {
            _ = try await session.client(accountID: "user-1")
        }
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
            pluginManagementBaseURL: URL(string: "https://plugins.example.test")!,
            storage: .sqlite(
                databaseURL: root.appendingPathComponent("client.sqlite3"),
                encryptionSecretReference: NativeLocalAgentAccountSession.sqliteEncryptionKeyReference
            )
        )
    }
}

private enum TokenUpdateFailure: Error, Equatable {
    case rejected
}

private actor AccessTokenUpdateRecorder {
    struct Value: Sendable {
        let accountID: String
        let endpoint: String
        let accessToken: String
    }

    private var recorded: [Value] = []

    func record(accountID: String, endpoint: String, accessToken: String) {
        recorded.append(Value(
            accountID: accountID,
            endpoint: endpoint,
            accessToken: accessToken
        ))
    }

    func values() -> [Value] { recorded }
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
    private var detachments = 0

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

    func detach() async {
        detachments += 1
        hostState = .stopped
    }
    func startedAccounts() -> [String] { accounts }
    func detachCount() -> Int { detachments }
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
