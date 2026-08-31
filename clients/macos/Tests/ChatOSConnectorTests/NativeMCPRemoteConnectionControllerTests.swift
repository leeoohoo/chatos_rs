import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeMCPRemoteConnectionControllerTests {
    @Test
    func listsConnectionsWithoutExposingCredentials() async throws {
        let provider = RemoteRuntimeStub()
        let controller = NativeMCPRemoteConnectionController(
            provider: provider,
            ssh: RemoteSSHStub()
        )

        let result = try await controller.call(name: "list_connections", arguments: [:])
        let serialized = result.canonicalJSONString

        #expect(serialized.contains("connection-1"))
        #expect(serialized.contains("credentials_available"))
        #expect(!serialized.contains("local-password"))
    }

    @Test
    func runsRemoteCommandThroughNativeSSHRuntime() async throws {
        let ssh = RemoteSSHStub()
        let controller = NativeMCPRemoteConnectionController(
            provider: RemoteRuntimeStub(),
            ssh: ssh
        )
        let result = try await controller.call(
            name: "run_command",
            arguments: [
                "connection_id": .string("connection-1"),
                "command": .string("uname -a"),
            ]
        )

        #expect(result.canonicalJSONString.contains("remote-output"))
        #expect(await ssh.lastCommand() == "uname -a")
    }
}

private actor RemoteRuntimeStub: NativeRemoteConnectionRuntimeProviding {
    func listConnections() async throws -> [RemoteConnection] { [Self.connection] }

    func testSaved(id: String, verificationCode: String?) async throws -> RemoteConnectionTestResult {
        .init(success: true, message: "连接成功")
    }

    func resolvedDraft(id: String) async throws -> RemoteConnectionDraft {
        .init(
            name: "Server",
            host: "server.example.com",
            port: 22,
            username: "root",
            authenticationType: .password,
            password: "local-password",
            privateKeyPath: nil,
            certificatePath: nil,
            defaultRemotePath: "/srv/app",
            hostKeyPolicy: .acceptNew,
            localConnectorDeviceID: NativeRemoteConnectionService.nativeDeviceID,
            localConnectorWorkspaceID: NativeRemoteConnectionService.nativeWorkspaceID,
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

    private static let connection = RemoteConnection(
        id: "connection-1",
        name: "Server",
        host: "server.example.com",
        port: 22,
        username: "root",
        authenticationType: .password,
        hasPassword: true,
        hasPrivateKeyPath: false,
        hasCertificatePath: false,
        defaultRemotePath: "/srv/app",
        hostKeyPolicy: .acceptNew,
        localConnectorDeviceID: NativeRemoteConnectionService.nativeDeviceID,
        localConnectorWorkspaceID: NativeRemoteConnectionService.nativeWorkspaceID,
        jumpEnabled: false,
        jumpConnectionID: nil,
        jumpHost: nil,
        jumpPort: nil,
        jumpUsername: nil,
        hasJumpPrivateKeyPath: false,
        hasJumpCertificatePath: false,
        hasJumpPassword: false,
        lastActiveAt: nil
    )
}

private actor RemoteSSHStub: NativeRemoteSSHExecuting {
    private var command: String?

    func runCommand(
        draft: RemoteConnectionDraft,
        command: String,
        timeoutSeconds: Int,
        maximumOutputCharacters: Int
    ) async throws -> NativeRemoteCommandResult {
        self.command = command
        return .init(exitCode: 0, stdout: "remote-output", stderr: "", truncated: false, timedOut: false)
    }

    func listDirectory(
        draft: RemoteConnectionDraft,
        path: String,
        limit: Int
    ) async throws -> [NativeRemoteDirectoryEntry] { [] }

    func resolveDirectory(
        draft: RemoteConnectionDraft,
        path: String
    ) async throws -> String { path }

    func download(
        draft: RemoteConnectionDraft,
        path: String,
        maximumBytes: Int
    ) async throws -> Data { Data() }

    func upload(
        draft: RemoteConnectionDraft,
        path: String,
        data: Data,
        createParentDirectories: Bool,
        overwrite: Bool
    ) async throws {}

    func uploadFile(
        draft: RemoteConnectionDraft,
        localURL: URL,
        remotePath: String,
        overwrite: Bool
    ) async throws {}

    func downloadFile(
        draft: RemoteConnectionDraft,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws {}

    func createDirectory(draft: RemoteConnectionDraft, path: String) async throws {}

    func renameEntry(
        draft: RemoteConnectionDraft,
        path: String,
        destinationPath: String
    ) async throws {}

    func deleteEntry(
        draft: RemoteConnectionDraft,
        path: String,
        recursively: Bool
    ) async throws {}

    func lastCommand() -> String? { command }
}
