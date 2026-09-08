import ChatOSCore
import Foundation

public protocol NativeRemoteConnectionRuntimeProviding: Sendable {
    func testSaved(id: String, verificationCode: String?) async throws -> RemoteConnectionTestResult
    func resolvedDraft(id: String) async throws -> RemoteConnectionDraft
}

public actor NativeRemoteConnectionService: RemoteConnectionServicing,
    NativeRemoteConnectionRuntimeProviding,
    RemoteTerminalCommandServicing {
    public static let nativeDeviceID = "chatos-swift-native-client"
    public static let nativeWorkspaceID = "local-machine"

    private let upstream: any RemoteConnectionServicing
    private let tester: any NativeRemoteConnectionTesting
    private let credentialStore: NativeRemoteConnectionCredentialStore
    private let ssh: any NativeRemoteSSHExecuting
    private let connectorStateStore: NativeConnectorStateStore?
    private let connectionCacheTTL: TimeInterval
    private var connectionCache: [String: CachedRemoteConnection] = [:]

    public init(
        upstream: any RemoteConnectionServicing,
        connectorStateURL: URL? = nil
    ) {
        self.upstream = upstream
        self.tester = NativeSSHConnectionTester()
        self.credentialStore = NativeRemoteConnectionCredentialStore()
        self.ssh = NativeOpenSSHClient()
        self.connectorStateStore = connectorStateURL.map(NativeConnectorStateStore.init(stateURL:))
        self.connectionCacheTTL = 15
    }

    init(
        upstream: any RemoteConnectionServicing,
        tester: any NativeRemoteConnectionTesting,
        credentialStore: NativeRemoteConnectionCredentialStore,
        ssh: any NativeRemoteSSHExecuting = NativeOpenSSHClient(),
        connectorStateStore: NativeConnectorStateStore? = nil,
        connectionCacheTTL: TimeInterval = 15
    ) {
        self.upstream = upstream
        self.tester = tester
        self.credentialStore = credentialStore
        self.ssh = ssh
        self.connectorStateStore = connectorStateStore
        self.connectionCacheTTL = max(connectionCacheTTL, 0)
    }

    public func listConnections() async throws -> [RemoteConnection] {
        let connections = try await upstream.listConnections()
        cache(connections)
        return connections.map(decorateWithLocalCredentialState)
    }

    public func getConnection(id: String) async throws -> RemoteConnection? {
        try await loadConnection(id: id).map(decorateWithLocalCredentialState)
    }

    public func createConnection(_ draft: RemoteConnectionDraft) async throws -> RemoteConnection {
        let resolvedDraft = try resolveLocalCredentials(in: draft)
        let created = try await upstream.createConnection(sanitizedForCloud(resolvedDraft))
        do {
            try credentialStore.save(.from(resolvedDraft), connectionID: created.id)
        } catch {
            try? await upstream.deleteConnection(id: created.id)
            throw error
        }
        cache(created)
        return decorateWithLocalCredentialState(created)
    }

    public func updateConnection(
        id: String,
        draft: RemoteConnectionDraft
    ) async throws -> RemoteConnection {
        var referencedDraft = draft
        referencedDraft.localCredentialReferenceID = id
        let resolvedDraft = try resolveLocalCredentials(in: referencedDraft)
        let updated = try await upstream.updateConnection(
            id: id,
            draft: sanitizedForCloud(resolvedDraft)
        )
        try credentialStore.save(.from(resolvedDraft), connectionID: id)
        cache(updated)
        return decorateWithLocalCredentialState(updated)
    }

    public func deleteConnection(id: String) async throws {
        try await upstream.deleteConnection(id: id)
        try credentialStore.delete(connectionID: id)
        connectionCache[id] = nil
    }

    public func testDraft(
        _ draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        try await tester.test(
            draft: resolveLocalCredentials(in: draft),
            verificationCode: verificationCode
        )
    }

    public func testSaved(
        id: String,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        guard let connection = try await loadConnection(id: id) else {
            throw NativeRemoteConnectionServiceError("远端连接不存在。")
        }
        var draft = try await draft(for: connection)
        draft.localCredentialReferenceID = id
        return try await tester.test(
            draft: resolveLocalCredentials(in: draft),
            verificationCode: verificationCode
        )
    }

    public func resolvedDraft(id: String) async throws -> RemoteConnectionDraft {
        guard let connection = try await loadConnection(id: id) else {
            throw NativeRemoteConnectionServiceError("远端连接不存在。")
        }
        var resolved = try await draft(for: connection)
        resolved.localCredentialReferenceID = id
        return try resolveLocalCredentials(in: resolved)
    }

    public func executeRemoteCommand(
        connectionID: String,
        command: String,
        workingDirectory: String
    ) async throws -> RemoteTerminalCommandResult {
        let draft = try await resolvedDraft(id: connectionID)
        let marker = "__CHATOS_REMOTE_CWD_\(UUID().uuidString)__"
        let requestedDirectory = workingDirectory.trimmedNonEmpty
            ?? draft.defaultRemotePath?.trimmedNonEmpty
            ?? "~"
        let changeDirectory = requestedDirectory == "~"
            ? "cd -- \"$HOME\""
            : "cd -- \(Self.shellQuote(requestedDirectory))"
        let script = """
        \(changeDirectory) || exit 72
        \(command)
        chatos_status=$?
        printf '\n\(marker)%s\n' "$PWD"
        exit $chatos_status
        """
        let result = try await ssh.runCommand(
            draft: draft,
            command: script,
            timeoutSeconds: 15 * 60,
            maximumOutputCharacters: 200_000
        )
        let parsed = Self.parseTerminalOutput(
            result.stdout,
            marker: marker,
            fallbackDirectory: requestedDirectory
        )
        return .init(
            output: parsed.output,
            error: result.stderr.trimmingCharacters(in: .newlines),
            exitCode: Int32(clamping: result.exitCode),
            workingDirectory: parsed.workingDirectory
        )
    }

    private func resolveLocalCredentials(
        in draft: RemoteConnectionDraft
    ) throws -> RemoteConnectionDraft {
        var resolved = draft
        if let connectionID = draft.localCredentialReferenceID?.trimmedNonEmpty,
           let stored = try credentialStore.load(connectionID: connectionID) {
            resolved = stored.applying(to: resolved)
        }
        if let jumpConnectionID = draft.jumpConnectionID?.trimmedNonEmpty,
           let jumpCredentials = try credentialStore.load(connectionID: jumpConnectionID) {
            resolved.jumpPassword = resolved.jumpPassword?.trimmedNonEmpty
                ?? jumpCredentials.password?.trimmedNonEmpty
            resolved.jumpPrivateKeyPath = resolved.jumpPrivateKeyPath?.trimmedNonEmpty
                ?? jumpCredentials.privateKeyPath?.trimmedNonEmpty
            resolved.jumpCertificatePath = resolved.jumpCertificatePath?.trimmedNonEmpty
                ?? jumpCredentials.certificatePath?.trimmedNonEmpty
        }
        return resolved
    }

    static func parseTerminalOutput(
        _ output: String,
        marker: String,
        fallbackDirectory: String
    ) -> (output: String, workingDirectory: String) {
        guard let markerRange = output.range(of: marker, options: .backwards) else {
            return (output.trimmingCharacters(in: .newlines), fallbackDirectory)
        }
        let visibleOutput = String(output[..<markerRange.lowerBound])
            .trimmingCharacters(in: .newlines)
        let suffix = output[markerRange.upperBound...]
        let directory = suffix
            .split(whereSeparator: \Character.isNewline)
            .first
            .map(String.init)?
            .trimmedNonEmpty ?? fallbackDirectory
        return (visibleOutput, directory)
    }

    private static func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    private func sanitizedForCloud(_ draft: RemoteConnectionDraft) -> RemoteConnectionDraft {
        var sanitized = draft
        sanitized.password = nil
        sanitized.privateKeyPath = nil
        sanitized.certificatePath = nil
        sanitized.jumpPassword = nil
        sanitized.jumpPrivateKeyPath = nil
        sanitized.jumpCertificatePath = nil
        let route = currentLocalConnectorRoute()
        sanitized.localConnectorDeviceID = route.deviceID
        sanitized.localConnectorWorkspaceID = route.workspaceID
        sanitized.localCredentialReferenceID = nil
        return sanitized
    }

    private func decorateWithLocalCredentialState(_ connection: RemoteConnection) -> RemoteConnection {
        guard let credentials = try? credentialStore.load(connectionID: connection.id) else {
            return connection
        }
        var decorated = connection
        decorated.hasPassword = credentials.password?.trimmedNonEmpty != nil
        decorated.hasPrivateKeyPath = credentials.privateKeyPath?.trimmedNonEmpty != nil
        decorated.hasCertificatePath = credentials.certificatePath?.trimmedNonEmpty != nil
        decorated.hasJumpPrivateKeyPath = credentials.jumpPrivateKeyPath?.trimmedNonEmpty != nil
        decorated.hasJumpCertificatePath = credentials.jumpCertificatePath?.trimmedNonEmpty != nil
        decorated.hasJumpPassword = credentials.jumpPassword?.trimmedNonEmpty != nil
        return decorated
    }

    private func draft(for connection: RemoteConnection) async throws -> RemoteConnectionDraft {
        var jumpHost = connection.jumpHost
        var jumpPort = connection.jumpPort
        var jumpUsername = connection.jumpUsername
        if let jumpID = connection.jumpConnectionID?.trimmedNonEmpty,
           let jump = try await loadConnection(id: jumpID) {
            jumpHost = jump.host
            jumpPort = jump.port
            jumpUsername = jump.username
        }
        let route = currentLocalConnectorRoute()
        return RemoteConnectionDraft(
            name: connection.name,
            host: connection.host,
            port: connection.port,
            username: connection.username,
            authenticationType: connection.authenticationType,
            password: nil,
            privateKeyPath: nil,
            certificatePath: nil,
            defaultRemotePath: connection.defaultRemotePath,
            hostKeyPolicy: connection.hostKeyPolicy,
            localConnectorDeviceID: route.deviceID,
            localConnectorWorkspaceID: route.workspaceID,
            jumpEnabled: connection.jumpEnabled,
            jumpConnectionID: connection.jumpConnectionID,
            jumpHost: jumpHost,
            jumpPort: jumpPort,
            jumpUsername: jumpUsername,
            jumpPrivateKeyPath: nil,
            jumpCertificatePath: nil,
            jumpPassword: nil,
            localCredentialReferenceID: connection.id
        )
    }

    private func currentLocalConnectorRoute() -> (deviceID: String, workspaceID: String) {
        guard let state = try? connectorStateStore?.load(),
              let deviceID = state.deviceID?.trimmedNonEmpty,
              let workspaceID = state.workspaces.first?.id.trimmedNonEmpty else {
            return (Self.nativeDeviceID, Self.nativeWorkspaceID)
        }
        return (deviceID, workspaceID)
    }

    private func loadConnection(id: String) async throws -> RemoteConnection? {
        let now = Date()
        if let cached = connectionCache[id], cached.expiresAt > now {
            return cached.connection
        }
        guard let connection = try await upstream.getConnection(id: id) else {
            connectionCache[id] = nil
            return nil
        }
        cache(connection, now: now)
        return connection
    }

    private func cache(_ connections: [RemoteConnection]) {
        let now = Date()
        for connection in connections {
            cache(connection, now: now)
        }
    }

    private func cache(_ connection: RemoteConnection, now: Date = Date()) {
        connectionCache[connection.id] = CachedRemoteConnection(
            connection: connection,
            expiresAt: now.addingTimeInterval(connectionCacheTTL)
        )
    }
}

private struct CachedRemoteConnection: Sendable {
    let connection: RemoteConnection
    let expiresAt: Date
}

private struct NativeRemoteConnectionServiceError: LocalizedError {
    let message: String

    init(_ message: String) {
        self.message = message
    }

    var errorDescription: String? { message }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
