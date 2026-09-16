import ChatOSCore
import ChatOSConnector

@MainActor
final class RemoteConnectionWorkspaceStore {
    private let terminalSessionProvider: (any NativeRemoteTerminalSessionProviding)?
    private let fileService: any RemoteFileServicing
    private var terminalWorkspaces: [String: NativeRemoteTerminalViewModel] = [:]
    private var fileWorkspaces: [String: RemoteSFTPViewModel] = [:]

    init(
        terminalService: any RemoteTerminalCommandServicing,
        fileService: any RemoteFileServicing
    ) {
        self.terminalSessionProvider = terminalService as? any NativeRemoteTerminalSessionProviding
        self.fileService = fileService
    }

    func terminalWorkspace(for connection: RemoteConnection) -> NativeRemoteTerminalViewModel {
        if let workspace = terminalWorkspaces[connection.id] {
            return workspace
        }
        let workspace = NativeRemoteTerminalViewModel(
            connectionID: connection.id,
            connectionName: connection.name,
            initialWorkingDirectory: connection.defaultRemotePath?.remoteWorkspaceNonEmpty ?? "~",
            sessionProvider: terminalSessionProvider
        )
        terminalWorkspaces[connection.id] = workspace
        return workspace
    }

    func fileWorkspace(for connectionID: String) -> RemoteSFTPViewModel {
        if let workspace = fileWorkspaces[connectionID] {
            return workspace
        }
        let workspace = RemoteSFTPViewModel(
            connectionID: connectionID,
            service: fileService
        )
        fileWorkspaces[connectionID] = workspace
        return workspace
    }

    func removeWorkspace(for connectionID: String) {
        terminalWorkspaces.removeValue(forKey: connectionID)?.disconnect()
        fileWorkspaces.removeValue(forKey: connectionID)
    }

    func removeAllWorkspaces() {
        terminalWorkspaces.values.forEach { $0.disconnect() }
        terminalWorkspaces.removeAll()
        fileWorkspaces.removeAll()
    }
}

private extension String {
    var remoteWorkspaceNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
