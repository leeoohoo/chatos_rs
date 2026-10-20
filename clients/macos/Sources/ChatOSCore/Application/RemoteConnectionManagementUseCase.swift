import Foundation

public enum RemoteConnectionOperationKind: Sendable, Equatable {
    case testDraft
    case save
    case testSaved(connectionID: String)
    case delete(connectionID: String)
}

public struct RemoteConnectionOperationToken: Sendable, Equatable {
    public let kind: RemoteConnectionOperationKind
    public let requestID: UUID

    public init(kind: RemoteConnectionOperationKind, requestID: UUID) {
        self.kind = kind
        self.requestID = requestID
    }
}

/// Single-flight lifecycle identity for remote-connection UI operations.
///
/// Saving, testing, and deleting all touch credentials or a remote host. The
/// presentation layer must therefore keep exactly one explicit operation in
/// flight and reject results after its sheet/page has been dismissed.
public struct RemoteConnectionOperationState: Sendable, Equatable {
    public private(set) var activeToken: RemoteConnectionOperationToken?

    public init() {}

    public mutating func begin(
        _ kind: RemoteConnectionOperationKind
    ) -> RemoteConnectionOperationToken? {
        guard activeToken == nil else { return nil }
        let token = RemoteConnectionOperationToken(kind: kind, requestID: UUID())
        activeToken = token
        return token
    }

    public func accepts(_ token: RemoteConnectionOperationToken) -> Bool {
        activeToken == token
    }

    @discardableResult
    public mutating func finish(_ token: RemoteConnectionOperationToken) -> Bool {
        guard accepts(token) else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}

/// Application boundary shared by the editor and saved-connection detail UI.
/// Views never coordinate a concrete Connector directly.
public struct RemoteConnectionManagementUseCase: Sendable {
    private let service: any RemoteConnectionServicing

    public init(service: any RemoteConnectionServicing) {
        self.service = service
    }

    public func save(
        editingConnectionID: String?,
        draft: RemoteConnectionDraft
    ) async throws -> RemoteConnection {
        if let editingConnectionID {
            return try await service.updateConnection(
                id: editingConnectionID,
                draft: draft
            )
        }
        return try await service.createConnection(draft)
    }

    public func testDraft(
        _ draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        try await service.testDraft(draft, verificationCode: verificationCode)
    }

    public func testSaved(
        connectionID: String,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        try await service.testSaved(id: connectionID, verificationCode: verificationCode)
    }

    public func delete(connectionID: String) async throws {
        try await service.deleteConnection(id: connectionID)
    }
}
