@testable import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native Remote File Service")
struct NativeRemoteFileServiceTests {
    @Test("resolves the saved default directory and maps remote metadata")
    func listsResolvedDirectory() async throws {
        let runtime = RemoteFileRuntimeStub()
        let ssh = RemoteFileSSHSpy()
        let service = NativeRemoteFileService(runtime: runtime, ssh: ssh)

        let initial = try await service.initialDirectory(connectionID: "server-1")
        #expect(initial == "/srv/app")

        let listing = try await service.listDirectory(
            connectionID: "server-1",
            path: "/srv/app"
        )
        #expect(listing.path == "/srv/app")
        #expect(listing.parentPath == "/srv")
        #expect(listing.entries.map(\.name) == ["logs", "app.log"])
        #expect(listing.entries[0].kind == .directory)
        #expect(listing.entries[1].size == 42)
    }

    @Test("uploads and downloads direct local files without cloud relay")
    func transfersFilesDirectly() async throws {
        let runtime = RemoteFileRuntimeStub()
        let ssh = RemoteFileSSHSpy()
        let service = NativeRemoteFileService(runtime: runtime, ssh: ssh)
        let local = URL(fileURLWithPath: "/tmp/report.txt")

        let remotePath = try await service.uploadFile(
            connectionID: "server-1",
            localURL: local,
            remoteDirectory: "/srv/app",
            overwrite: true
        )
        #expect(remotePath == "/srv/app/report.txt")
        let upload = await ssh.lastUpload()
        #expect(upload?.localURL == local)
        #expect(upload?.remotePath == "/srv/app/report.txt")

        let destination = URL(fileURLWithPath: "/tmp/downloaded.txt")
        try await service.downloadFile(
            connectionID: "server-1",
            remotePath: "/srv/app/report.txt",
            localURL: destination,
            overwrite: false
        )
        let download = await ssh.lastDownload()
        #expect(download?.remotePath == "/srv/app/report.txt")
        #expect(download?.localURL == destination)
    }
}

private actor RemoteFileRuntimeStub: NativeRemoteConnectionRuntimeProviding {
    func listConnections() async throws -> [RemoteConnection] { [] }

    func testSaved(
        id: String,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        .init(success: true, message: nil)
    }

    func resolvedDraft(id: String) async throws -> RemoteConnectionDraft {
        RemoteConnectionDraft(
            name: "Server",
            host: "server.example.com",
            port: 22,
            username: "root",
            authenticationType: .password,
            password: "secret",
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
}

private actor RemoteFileSSHSpy: NativeRemoteSSHExecuting {
    struct Transfer: Sendable {
        let localURL: URL
        let remotePath: String
        let overwrite: Bool
    }

    private var uploadTransfer: Transfer?
    private var downloadTransfer: Transfer?

    func runCommand(
        draft: RemoteConnectionDraft,
        command: String,
        timeoutSeconds: Int,
        maximumOutputCharacters: Int
    ) async throws -> NativeRemoteCommandResult {
        .init(exitCode: 0, stdout: "", stderr: "", truncated: false, timedOut: false)
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
                modifiedAt: Date(timeIntervalSince1970: 10),
                permissions: "-rw-r--r--"
            ),
            .init(
                name: "logs",
                path: "/srv/app/logs",
                type: "directory",
                size: 4_096,
                modifiedAt: Date(timeIntervalSince1970: 20),
                permissions: "drwxr-xr-x"
            ),
        ]
    }

    func resolveDirectory(
        draft: RemoteConnectionDraft,
        path: String
    ) async throws -> String { path == "." ? "/root" : path }

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
    ) async throws {
        uploadTransfer = .init(localURL: localURL, remotePath: remotePath, overwrite: overwrite)
    }

    func downloadFile(
        draft: RemoteConnectionDraft,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws {
        downloadTransfer = .init(localURL: localURL, remotePath: remotePath, overwrite: overwrite)
    }

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

    func lastUpload() -> Transfer? { uploadTransfer }
    func lastDownload() -> Transfer? { downloadTransfer }
}
