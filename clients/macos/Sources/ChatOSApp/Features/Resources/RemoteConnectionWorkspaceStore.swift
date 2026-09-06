import ChatOSCore

@MainActor
final class RemoteConnectionWorkspaceStore {
    private let terminalService: any RemoteTerminalCommandServicing
    private let fileService: any RemoteFileServicing
    private var terminalWorkspaces: [String: RemoteTerminalWorkspaceViewModel] = [:]
    private var fileWorkspaces: [String: RemoteSFTPViewModel] = [:]

    init(
        terminalService: any RemoteTerminalCommandServicing,
        fileService: any RemoteFileServicing
    ) {
        self.terminalService = terminalService
        self.fileService = fileService
    }

    func terminalWorkspace(for connection: RemoteConnection) -> RemoteTerminalWorkspaceViewModel {
        if let workspace = terminalWorkspaces[connection.id] {
            return workspace
        }
        let workspace = RemoteTerminalWorkspaceViewModel(
            connection: connection,
            service: terminalService
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
