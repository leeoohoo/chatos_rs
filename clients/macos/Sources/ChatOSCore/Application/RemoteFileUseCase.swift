import Foundation

public enum RemoteFileLoadTarget: Sendable, Equatable {
    case initial
    case remote(path: String)
    case local(path: String)
}

public enum RemoteFileMutationKind: Sendable, Equatable {
    case upload(remoteDirectory: String)
    case download(remotePath: String)
    case createDirectory(parentPath: String)
    case rename(path: String)
    case delete(path: String)
}

public struct RemoteFileRequestToken: Sendable, Equatable {
    public let connectionID: String
    public let target: RemoteFileLoadTarget
    public let generation: UInt64
    public let requestID: UUID

    public init(
        connectionID: String,
        target: RemoteFileLoadTarget,
        generation: UInt64,
        requestID: UUID
    ) {
        self.connectionID = connectionID
        self.target = target
        self.generation = generation
        self.requestID = requestID
    }
}

public struct RemoteFileMutationToken: Sendable, Equatable {
    public let connectionID: String
    public let kind: RemoteFileMutationKind
    public let generation: UInt64
    public let requestID: UUID

    public init(
        connectionID: String,
        kind: RemoteFileMutationKind,
        generation: UInt64,
        requestID: UUID
    ) {
        self.connectionID = connectionID
        self.kind = kind
        self.generation = generation
        self.requestID = requestID
    }
}

/// Independent latest-wins identities for remote navigation, local navigation,
/// and one side-effecting transfer/mutation lane.
public struct RemoteFileLifecycleState: Sendable, Equatable {
    public private(set) var remoteLoad: RemoteFileRequestToken?
    public private(set) var localLoad: RemoteFileRequestToken?
    public private(set) var mutation: RemoteFileMutationToken?
    private var generation: UInt64 = 0

    public init() {}

    public mutating func beginRemoteLoad(
        connectionID: String,
        target: RemoteFileLoadTarget
    ) -> RemoteFileRequestToken? {
        guard mutation == nil else { return nil }
        generation &+= 1
        let token = RemoteFileRequestToken(
            connectionID: connectionID,
            target: target,
            generation: generation,
            requestID: UUID()
        )
        remoteLoad = token
        return token
    }

    public mutating func beginLocalLoad(
        connectionID: String,
        path: String
    ) -> RemoteFileRequestToken? {
        guard mutation == nil else { return nil }
        generation &+= 1
        let token = RemoteFileRequestToken(
            connectionID: connectionID,
            target: .local(path: path),
            generation: generation,
            requestID: UUID()
        )
        localLoad = token
        return token
    }

    public mutating func beginMutation(
        connectionID: String,
        kind: RemoteFileMutationKind
    ) -> RemoteFileMutationToken? {
        guard mutation == nil else { return nil }
        generation &+= 1
        remoteLoad = nil
        localLoad = nil
        let token = RemoteFileMutationToken(
            connectionID: connectionID,
            kind: kind,
            generation: generation,
            requestID: UUID()
        )
        mutation = token
        return token
    }

    public func acceptsRemoteLoad(_ token: RemoteFileRequestToken) -> Bool {
        remoteLoad == token && mutation == nil
    }

    public func acceptsLocalLoad(_ token: RemoteFileRequestToken) -> Bool {
        localLoad == token && mutation == nil
    }

    public func acceptsMutation(_ token: RemoteFileMutationToken) -> Bool {
        mutation == token
    }

    @discardableResult
    public mutating func finishRemoteLoad(_ token: RemoteFileRequestToken) -> Bool {
        guard acceptsRemoteLoad(token) else { return false }
        remoteLoad = nil
        return true
    }

    @discardableResult
    public mutating func finishLocalLoad(_ token: RemoteFileRequestToken) -> Bool {
        guard acceptsLocalLoad(token) else { return false }
        localLoad = nil
        return true
    }

    @discardableResult
    public mutating func finishMutation(_ token: RemoteFileMutationToken) -> Bool {
        guard acceptsMutation(token) else { return false }
        mutation = nil
        return true
    }

    public mutating func reset() {
        remoteLoad = nil
        localLoad = nil
        mutation = nil
    }

    public mutating func cancelRemoteLoad() {
        remoteLoad = nil
    }

    public mutating func cancelLocalLoad() {
        localLoad = nil
    }
}

public struct RemoteFileMutationResult: Sendable, Equatable {
    public let refreshedRemoteListing: RemoteDirectoryListing?

    public init(refreshedRemoteListing: RemoteDirectoryListing?) {
        self.refreshedRemoteListing = refreshedRemoteListing
    }
}

/// Application boundary for the remote file workspace. Side effects and their
/// best-effort presentation refresh are deliberately separate, so a committed
/// upload/rename/delete is never reported as though it did not happen.
public struct RemoteFileUseCase: Sendable {
    private let service: any RemoteFileServicing

    public init(service: any RemoteFileServicing) {
        self.service = service
    }

    public func connect(
        connectionID: String,
        verificationCode: String?
    ) async throws -> RemoteDirectoryListing {
        try await service.authenticate(
            connectionID: connectionID,
            verificationCode: verificationCode
        )
        let path = try await service.initialDirectory(connectionID: connectionID)
        return try await service.listDirectory(connectionID: connectionID, path: path)
    }

    public func list(
        connectionID: String,
        path: String
    ) async throws -> RemoteDirectoryListing {
        try await service.listDirectory(connectionID: connectionID, path: path)
    }

    public func upload(
        connectionID: String,
        localURL: URL,
        remoteDirectory: String,
        overwrite: Bool
    ) async throws -> RemoteFileMutationResult {
        _ = try await service.uploadFile(
            connectionID: connectionID,
            localURL: localURL,
            remoteDirectory: remoteDirectory,
            overwrite: overwrite
        )
        return await refreshedRemote(connectionID: connectionID, path: remoteDirectory)
    }

    public func download(
        connectionID: String,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws {
        try await service.downloadFile(
            connectionID: connectionID,
            remotePath: remotePath,
            localURL: localURL,
            overwrite: overwrite
        )
    }

    public func createDirectory(
        connectionID: String,
        parentPath: String,
        name: String
    ) async throws -> RemoteFileMutationResult {
        try await service.createDirectory(
            connectionID: connectionID,
            parentPath: parentPath,
            name: name
        )
        return await refreshedRemote(connectionID: connectionID, path: parentPath)
    }

    public func rename(
        connectionID: String,
        path: String,
        newName: String,
        parentPath: String
    ) async throws -> RemoteFileMutationResult {
        try await service.renameEntry(
            connectionID: connectionID,
            path: path,
            newName: newName
        )
        return await refreshedRemote(connectionID: connectionID, path: parentPath)
    }

    public func delete(
        connectionID: String,
        path: String,
        recursively: Bool,
        parentPath: String
    ) async throws -> RemoteFileMutationResult {
        try await service.deleteEntry(
            connectionID: connectionID,
            path: path,
            recursively: recursively
        )
        return await refreshedRemote(connectionID: connectionID, path: parentPath)
    }

    private func refreshedRemote(
        connectionID: String,
        path: String
    ) async -> RemoteFileMutationResult {
        let listing = try? await service.listDirectory(
            connectionID: connectionID,
            path: path
        )
        return .init(refreshedRemoteListing: listing)
    }
}
