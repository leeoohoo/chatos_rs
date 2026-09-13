@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeRemoteConnectionServiceTests: XCTestCase {
    func testStoresCredentialsLocallyAndNeverSendsThemToCloud() async throws {
        let upstream = RemoteConnectionUpstreamStub()
        let tester = RemoteConnectionTesterSpy()
        let credentialStore = NativeRemoteConnectionCredentialStore(
            secretStore: TestNativeConnectorSecretStore()
        )
        let routeStore = NativeConnectorRouteStore()
        routeStore.replace(deviceID: "device-current", workspaceID: "workspace-current")
        let service = NativeRemoteConnectionService(
            upstream: upstream,
            tester: tester,
            credentialStore: credentialStore,
            routeStore: routeStore
        )

        let created = try await service.createConnection(Self.passwordDraft)
        XCTAssertTrue(created.hasPassword)

        let capturedCreatedDraft = await upstream.lastCreatedDraft()
        let sentDraft = try XCTUnwrap(capturedCreatedDraft)
        XCTAssertNil(sentDraft.password)
        XCTAssertNil(sentDraft.privateKeyPath)
        XCTAssertEqual(
            sentDraft.localConnectorDeviceID,
            "device-current"
        )
        XCTAssertEqual(
            sentDraft.localConnectorWorkspaceID,
            "workspace-current"
        )

        _ = try await service.testSaved(id: created.id, verificationCode: nil)
        let capturedTestDraft = await tester.lastDraft()
        let testedDraft = try XCTUnwrap(capturedTestDraft)
        XCTAssertEqual(testedDraft.password, "local-secret")
        XCTAssertEqual(testedDraft.host, "server.example.com")
    }

    func testSSHConfigUsesNativeOpenSSHAndProxyJump() throws {
        var draft = Self.passwordDraft
        draft.jumpEnabled = true
        draft.jumpHost = "jump.example.com"
        draft.jumpPort = 2202
        draft.jumpUsername = "jump-user"
        draft.jumpPassword = "jump-secret"

        let config = try NativeSSHConnectionTester.sshConfig(for: draft)

        XCTAssertTrue(config.contains("Host chatos-target"))
        XCTAssertTrue(config.contains("Host chatos-jump"))
        XCTAssertTrue(config.contains("ProxyJump chatos-jump"))
        XCTAssertTrue(config.contains("StrictHostKeyChecking accept-new"))
        XCTAssertFalse(config.contains("local_connector"))
        XCTAssertFalse(config.contains("local-secret"))
        XCTAssertFalse(config.contains("jump-secret"))
    }

    func testResolvedDraftCachesDirectConnectionLookupForConsecutiveTools() async throws {
        let upstream = RemoteConnectionUpstreamStub()
        let saved = try await upstream.createConnection(Self.passwordDraft)
        let service = NativeRemoteConnectionService(
            upstream: upstream,
            tester: RemoteConnectionTesterSpy(),
            credentialStore: NativeRemoteConnectionCredentialStore(
                secretStore: TestNativeConnectorSecretStore()
            ),
            connectionCacheTTL: 15
        )

        _ = try await service.resolvedDraft(id: saved.id)
        _ = try await service.resolvedDraft(id: saved.id)

        let getRequests = await upstream.getRequestCount()
        let listRequests = await upstream.listRequestCount()
        XCTAssertEqual(getRequests, 1)
        XCTAssertEqual(listRequests, 0)
    }

    func testLoadingAnExistingConnectionNeverRewritesItsFrozenRoute() async throws {
        let upstream = RemoteConnectionUpstreamStub()
        let saved = try await upstream.createConnection(Self.passwordDraft)
        let routeStore = NativeConnectorRouteStore()
        routeStore.replace(deviceID: "device-current", workspaceID: "workspace-current")
        let service = NativeRemoteConnectionService(
            upstream: upstream,
            tester: RemoteConnectionTesterSpy(),
            credentialStore: NativeRemoteConnectionCredentialStore(
                secretStore: TestNativeConnectorSecretStore()
            ),
            routeStore: routeStore
        )

        let loaded = try await service.getConnection(id: saved.id)
        let unchanged = try XCTUnwrap(loaded)
        let updateRequests = await upstream.updateRequestCount()

        XCTAssertEqual(unchanged.localConnectorDeviceID, "legacy-device")
        XCTAssertEqual(unchanged.localConnectorWorkspaceID, "legacy-workspace")
        XCTAssertEqual(updateRequests, 0)
    }

    func testCreatingAConnectionFailsClosedWithoutAnActiveConnectorRoute() async throws {
        let upstream = RemoteConnectionUpstreamStub()
        let service = NativeRemoteConnectionService(
            upstream: upstream,
            tester: RemoteConnectionTesterSpy(),
            credentialStore: NativeRemoteConnectionCredentialStore(
                secretStore: TestNativeConnectorSecretStore()
            ),
            routeStore: NativeConnectorRouteStore()
        )

        do {
            _ = try await service.createConnection(Self.passwordDraft)
            XCTFail("Expected a missing Local Connector route to fail")
        } catch let error as NativeConnectorRouteError {
            XCTAssertEqual(error, .unavailable)
        }
        let createdDraft = await upstream.lastCreatedDraft()
        XCTAssertNil(createdDraft)
    }

    func testSSHConfigCanReuseAConnectionWithoutExposingTheControlPath() throws {
        let config = try NativeSSHConnectionTester.sshConfig(
            for: Self.passwordDraft,
            controlPath: "/tmp/chatos-control-test"
        )

        XCTAssertTrue(config.contains("ControlMaster auto"))
        XCTAssertTrue(config.contains("ControlPersist 120"))
        XCTAssertTrue(config.contains("ControlPath \"/tmp/chatos-control-test\""))
    }

    func testPersistentSSHControlPathFitsMacOSUnixSocketLimit() throws {
        let path = try NativeOpenSSHClient.persistentControlPath(for: Self.passwordDraft).path

        XCTAssertTrue(path.hasPrefix("/tmp/chatos-ssh-"))
        XCTAssertLessThan(path.utf8.count + 16, 104)
    }

    func testRemoteTerminalOutputKeepsVisibleTextAndUpdatesWorkingDirectory() {
        let marker = "__CHATOS_REMOTE_CWD_TEST__"
        let parsed = NativeRemoteConnectionService.parseTerminalOutput(
            "first line\nsecond line\n\(marker)/srv/project\n",
            marker: marker,
            fallbackDirectory: "/root"
        )

        XCTAssertEqual(parsed.output, "first line\nsecond line")
        XCTAssertEqual(parsed.workingDirectory, "/srv/project")
    }

    private static let passwordDraft = RemoteConnectionDraft(
        name: "Server",
        host: "server.example.com",
        port: 22,
        username: "root",
        authenticationType: .password,
        password: "local-secret",
        privateKeyPath: nil,
        certificatePath: nil,
        defaultRemotePath: "/srv/app",
        hostKeyPolicy: .acceptNew,
        localConnectorDeviceID: "legacy-device",
        localConnectorWorkspaceID: "legacy-workspace",
        jumpEnabled: false,
        jumpConnectionID: nil,
        jumpHost: nil,
        jumpPort: nil,
        jumpUsername: nil,
        jumpPrivateKeyPath: nil,
        jumpCertificatePath: nil,
        jumpPassword: nil
    )
}

private actor RemoteConnectionUpstreamStub: RemoteConnectionServicing {
    private var connections: [RemoteConnection] = []
    private var createdDraft: RemoteConnectionDraft?
    private var getRequests = 0
    private var listRequests = 0
    private var updateRequests = 0

    func listConnections() async throws -> [RemoteConnection] {
        listRequests += 1
        return connections
    }

    func getConnection(id: String) async throws -> RemoteConnection? {
        getRequests += 1
        return connections.first { $0.id == id }
    }

    func createConnection(_ draft: RemoteConnectionDraft) async throws -> RemoteConnection {
        createdDraft = draft
        let connection = RemoteConnection(
            id: "connection-1",
            name: draft.name ?? "Server",
            host: draft.host,
            port: draft.port,
            username: draft.username,
            authenticationType: draft.authenticationType,
            hasPassword: false,
            hasPrivateKeyPath: false,
            hasCertificatePath: false,
            defaultRemotePath: draft.defaultRemotePath,
            hostKeyPolicy: draft.hostKeyPolicy,
            localConnectorDeviceID: draft.localConnectorDeviceID,
            localConnectorWorkspaceID: draft.localConnectorWorkspaceID,
            jumpEnabled: draft.jumpEnabled,
            jumpConnectionID: draft.jumpConnectionID,
            jumpHost: draft.jumpHost,
            jumpPort: draft.jumpPort,
            jumpUsername: draft.jumpUsername,
            hasJumpPrivateKeyPath: false,
            hasJumpCertificatePath: false,
            hasJumpPassword: false,
            lastActiveAt: nil
        )
        connections = [connection]
        return connection
    }

    func updateConnection(
        id: String,
        draft: RemoteConnectionDraft
    ) async throws -> RemoteConnection {
        updateRequests += 1
        let updated = RemoteConnection(
            id: id,
            name: draft.name ?? "Server",
            host: draft.host,
            port: draft.port,
            username: draft.username,
            authenticationType: draft.authenticationType,
            hasPassword: false,
            hasPrivateKeyPath: false,
            hasCertificatePath: false,
            defaultRemotePath: draft.defaultRemotePath,
            hostKeyPolicy: draft.hostKeyPolicy,
            localConnectorDeviceID: draft.localConnectorDeviceID,
            localConnectorWorkspaceID: draft.localConnectorWorkspaceID,
            jumpEnabled: draft.jumpEnabled,
            jumpConnectionID: draft.jumpConnectionID,
            jumpHost: draft.jumpHost,
            jumpPort: draft.jumpPort,
            jumpUsername: draft.jumpUsername,
            hasJumpPrivateKeyPath: false,
            hasJumpCertificatePath: false,
            hasJumpPassword: false,
            lastActiveAt: nil
        )
        connections.removeAll { $0.id == id }
        connections.append(updated)
        return updated
    }

    func deleteConnection(id: String) async throws {
        connections.removeAll { $0.id == id }
    }

    func testDraft(
        _ draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        XCTFail("Native service must not use the cloud test endpoint")
        return .init(success: false, message: nil)
    }

    func testSaved(
        id: String,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        XCTFail("Native service must not use the cloud test endpoint")
        return .init(success: false, message: nil)
    }

    func lastCreatedDraft() -> RemoteConnectionDraft? {
        createdDraft
    }

    func getRequestCount() -> Int { getRequests }

    func listRequestCount() -> Int { listRequests }

    func updateRequestCount() -> Int { updateRequests }
}

private actor RemoteConnectionTesterSpy: NativeRemoteConnectionTesting {
    private var draft: RemoteConnectionDraft?

    func test(
        draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        self.draft = draft
        return .init(success: true, message: "ok")
    }

    func lastDraft() -> RemoteConnectionDraft? {
        draft
    }
}

private final class TestNativeConnectorSecretStore: NativeConnectorSecretStoring,
    @unchecked Sendable {
    private let lock = NSLock()
    private var values: [String: Data] = [:]

    func load(account: String) throws -> Data? {
        lock.lock()
        defer { lock.unlock() }
        return values[account]
    }

    func save(_ value: Data, account: String) throws {
        lock.lock()
        defer { lock.unlock() }
        values[account] = value
    }

    func delete(account: String) throws {
        lock.lock()
        defer { lock.unlock() }
        values[account] = nil
    }
}
