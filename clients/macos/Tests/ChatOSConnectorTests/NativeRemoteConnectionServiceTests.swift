@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeRemoteConnectionServiceTests: XCTestCase {
    func testStoresCredentialsLocallyAndNeverSendsThemToCloud() async throws {
        let upstream = RemoteConnectionUpstreamStub()
        let tester = RemoteConnectionTesterSpy()
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-remote-test-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let credentialStore = NativeRemoteConnectionCredentialStore(
            secretStore: NativeConnectorSecretStore(rootURL: root)
        )
        let service = NativeRemoteConnectionService(
            upstream: upstream,
            tester: tester,
            credentialStore: credentialStore
        )

        let created = try await service.createConnection(Self.passwordDraft)
        XCTAssertTrue(created.hasPassword)

        let capturedCreatedDraft = await upstream.lastCreatedDraft()
        let sentDraft = try XCTUnwrap(capturedCreatedDraft)
        XCTAssertNil(sentDraft.password)
        XCTAssertNil(sentDraft.privateKeyPath)
        XCTAssertEqual(
            sentDraft.localConnectorDeviceID,
            NativeRemoteConnectionService.nativeDeviceID
        )
        XCTAssertEqual(
            sentDraft.localConnectorWorkspaceID,
            NativeRemoteConnectionService.nativeWorkspaceID
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

    func listConnections() async throws -> [RemoteConnection] {
        connections
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
        try await createConnection(draft)
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
