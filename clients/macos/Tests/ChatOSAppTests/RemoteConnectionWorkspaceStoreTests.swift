import ChatOSCore
import Foundation
import Testing
@testable import ChatOSApp

@Suite("Remote connection workspace store")
@MainActor
struct RemoteConnectionWorkspaceStoreTests {
    @Test("keeps independent terminal and SFTP state for every connection")
    func cachesWorkspacesByConnectionID() {
        let service = RemoteWorkspaceServiceStub()
        let store = RemoteConnectionWorkspaceStore(
            terminalService: service,
            fileService: service
        )
        let aliyun = connection(id: "aliyun", username: "root")
        let tengxun = connection(id: "tengxun", username: "ubuntu")

        let aliyunTerminal = store.terminalWorkspace(for: aliyun)
        let tengxunTerminal = store.terminalWorkspace(for: tengxun)
        let aliyunFiles = store.fileWorkspace(for: aliyun.id)
        let tengxunFiles = store.fileWorkspace(for: tengxun.id)

        #expect(store.terminalWorkspace(for: aliyun) === aliyunTerminal)
        #expect(store.fileWorkspace(for: aliyun.id) === aliyunFiles)
        #expect(aliyunTerminal !== tengxunTerminal)
        #expect(aliyunFiles !== tengxunFiles)
        #expect(aliyunFiles.connectionID == "aliyun")
        #expect(tengxunFiles.connectionID == "tengxun")
    }

    @Test("removes only the edited or deleted connection workspace")
    func invalidatesOneConnection() {
        let service = RemoteWorkspaceServiceStub()
        let store = RemoteConnectionWorkspaceStore(
            terminalService: service,
            fileService: service
        )
        let aliyun = connection(id: "aliyun", username: "root")
        let tengxun = connection(id: "tengxun", username: "ubuntu")
        let oldAliyun = store.terminalWorkspace(for: aliyun)
        let oldTengxun = store.terminalWorkspace(for: tengxun)

        store.removeWorkspace(for: aliyun.id)

        #expect(store.terminalWorkspace(for: aliyun) !== oldAliyun)
        #expect(store.terminalWorkspace(for: tengxun) === oldTengxun)
    }

    private func connection(id: String, username: String) -> RemoteConnection {
        RemoteConnection(
            id: id,
            name: id,
            host: "example.com",
            port: 22,
            username: username,
            authenticationType: .privateKey,
            hasPassword: false,
            hasPrivateKeyPath: true,
            hasCertificatePath: false,
            defaultRemotePath: nil,
            hostKeyPolicy: .acceptNew,
            localConnectorDeviceID: "device",
            localConnectorWorkspaceID: "workspace",
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
}

private struct RemoteWorkspaceServiceStub: RemoteTerminalCommandServicing, RemoteFileServicing {
    func executeRemoteCommand(
        connectionID: String,
        command: String,
        workingDirectory: String
    ) async throws -> RemoteTerminalCommandResult {
        .init(output: "", error: "", exitCode: 0, workingDirectory: workingDirectory)
    }

    func initialDirectory(connectionID: String) async throws -> String { "/" }

    func listDirectory(connectionID: String, path: String) async throws -> RemoteDirectoryListing {
        .init(path: path, parentPath: nil, entries: [])
    }

    func uploadFile(
        connectionID: String,
        localURL: URL,
        remoteDirectory: String,
        overwrite: Bool
    ) async throws -> String { remoteDirectory }

    func downloadFile(
        connectionID: String,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws {}

    func createDirectory(connectionID: String, parentPath: String, name: String) async throws {}

    func renameEntry(connectionID: String, path: String, newName: String) async throws {}

    func deleteEntry(connectionID: String, path: String, recursively: Bool) async throws {}
}
