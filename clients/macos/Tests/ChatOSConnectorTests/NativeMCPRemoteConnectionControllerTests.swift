import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeMCPRemoteConnectionControllerTests {
    @Test
    func hidesConnectionDiscoveryAndInternalIDsFromToolDefinitions() {
        let serialized = NativeJSONValue.array(
            NativeMCPRemoteConnectionController.toolDefinitions
        ).canonicalJSONString

        #expect(!serialized.contains("list_connections"))
        #expect(!serialized.contains("connection_id"))
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
        #expect(!result.canonicalJSONString.contains("connection_id"))
        #expect(await ssh.lastCommand() == "uname -a")
    }

    @Test
    func directoryListingIncludesUsefulFileMetadataWithoutInternalIDs() async throws {
        let controller = NativeMCPRemoteConnectionController(
            provider: RemoteRuntimeStub(),
            ssh: RemoteSSHStub()
        )
        let result = try await controller.call(
            name: "list_directory",
            arguments: ["connection_id": .string("connection-1")]
        )
        let serialized = result.canonicalJSONString

        #expect(serialized.contains("size_bytes"))
        #expect(serialized.contains("modified_at"))
        #expect(serialized.contains("permissions"))
        #expect(!serialized.contains("connection_id"))
    }

    @Test
    func connectionAndFileToolsUseTheBoundConnectionWithoutLeakingItsID() async throws {
        let ssh = RemoteSSHStub()
        let controller = NativeMCPRemoteConnectionController(
            provider: RemoteRuntimeStub(),
            ssh: ssh
        )

        let tested = try await controller.call(
            name: "test_connection",
            arguments: ["connection_id": .string("connection-1")]
        )
        let read = try await controller.call(
            name: "read_file",
            arguments: [
                "connection_id": .string("connection-1"),
                "path": .string("/srv/app/config.txt"),
            ]
        )
        let downloaded = try await controller.call(
            name: "download_file",
            arguments: [
                "connection_id": .string("connection-1"),
                "path": .string("/srv/app/config.txt"),
                "encoding": .string("base64"),
            ]
        )
        let uploaded = try await controller.call(
            name: "upload_file",
            arguments: [
                "connection_id": .string("connection-1"),
                "path": .string("/srv/app/output.txt"),
                "content": .string("updated"),
            ]
        )

        let serialized = [tested, read, downloaded, uploaded]
            .map(\.canonicalJSONString)
            .joined(separator: "\n")
        #expect(serialized.contains("连接成功"))
        #expect(serialized.contains("remote-file"))
        #expect(serialized.contains(Data("remote-file".utf8).base64EncodedString()))
        #expect(serialized.contains("uploaded"))
        #expect(!serialized.contains("connection_id"))
        #expect(await ssh.lastUploadPath() == "/srv/app/output.txt")
    }
}

private actor RemoteRuntimeStub: NativeRemoteConnectionRuntimeProviding {
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
    private var uploadPath: String?

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
    ) async throws -> [NativeRemoteDirectoryEntry] {
        [
            .init(
                name: "app.log",
                path: "/srv/app/app.log",
                type: "file",
                size: 42,
                modifiedAt: Date(timeIntervalSince1970: 1_700_000_000),
                permissions: "-rw-r--r--"
            ),
        ]
    }

    func resolveDirectory(
        draft: RemoteConnectionDraft,
        path: String
    ) async throws -> String { path }

    func download(
        draft: RemoteConnectionDraft,
        path: String,
        maximumBytes: Int
    ) async throws -> Data { Data("remote-file".utf8) }

    func upload(
        draft: RemoteConnectionDraft,
        path: String,
        data: Data,
        createParentDirectories: Bool,
        overwrite: Bool
    ) async throws {
        uploadPath = path
    }

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

    func lastUploadPath() -> String? { uploadPath }
}
