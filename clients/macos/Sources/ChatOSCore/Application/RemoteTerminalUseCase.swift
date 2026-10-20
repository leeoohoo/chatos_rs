import Foundation

public enum RemoteTerminalEvent: Sendable, Equatable {
    case output(Data)
    case terminated(exitCode: Int32?)
}

public protocol RemoteTerminalSessionPort: AnyObject, Sendable {
    var isRunning: Bool { get }
    func events() -> AsyncStream<RemoteTerminalEvent>
    func start(columns: Int, rows: Int)
    func send(_ data: Data)
    func resize(columns: Int, rows: Int)
    func interrupt()
    func close()
}

public protocol RemoteTerminalSessionProviding: Sendable {
    func openRemoteTerminalSession(
        connectionID: String,
        verificationCode: String?
    ) async throws -> any RemoteTerminalSessionPort
}

public struct RemoteTerminalConnectToken: Sendable, Equatable {
    public let connectionID: String
    public let generation: UInt64
    public let requestID: UUID

    public init(connectionID: String, generation: UInt64, requestID: UUID) {
        self.connectionID = connectionID
        self.generation = generation
        self.requestID = requestID
    }
}

public struct RemoteTerminalSessionToken: Sendable, Equatable {
    public let connectionID: String
    public let generation: UInt64
    public let requestID: UUID

    public init(connectionID: String, generation: UInt64, requestID: UUID) {
        self.connectionID = connectionID
        self.generation = generation
        self.requestID = requestID
    }
}

/// Connect and event-stream identity for one interactive remote terminal.
/// Reconnect/disconnect invalidates both an in-flight open and all events from
/// the previous session.
public struct RemoteTerminalLifecycleState: Sendable, Equatable {
    public private(set) var activeConnect: RemoteTerminalConnectToken?
    public private(set) var activeSession: RemoteTerminalSessionToken?
    private var generation: UInt64 = 0

    public init() {}

    public mutating func beginConnect(connectionID: String) -> RemoteTerminalConnectToken {
        generation &+= 1
        activeSession = nil
        let token = RemoteTerminalConnectToken(
            connectionID: connectionID,
            generation: generation,
            requestID: UUID()
        )
        activeConnect = token
        return token
    }

    public func acceptsConnect(_ token: RemoteTerminalConnectToken) -> Bool {
        activeConnect == token
    }

    public mutating func activate(
        _ token: RemoteTerminalConnectToken
    ) -> RemoteTerminalSessionToken? {
        guard acceptsConnect(token) else { return nil }
        let session = RemoteTerminalSessionToken(
            connectionID: token.connectionID,
            generation: token.generation,
            requestID: UUID()
        )
        activeConnect = nil
        activeSession = session
        return session
    }

    @discardableResult
    public mutating func finishConnect(_ token: RemoteTerminalConnectToken) -> Bool {
        guard acceptsConnect(token) else { return false }
        activeConnect = nil
        return true
    }

    public func acceptsSession(_ token: RemoteTerminalSessionToken) -> Bool {
        activeSession == token
    }

    @discardableResult
    public mutating func finishSession(_ token: RemoteTerminalSessionToken) -> Bool {
        guard acceptsSession(token) else { return false }
        activeSession = nil
        return true
    }

    public mutating func reset() {
        activeConnect = nil
        activeSession = nil
    }
}

public struct RemoteTerminalUseCase: Sendable {
    private let provider: any RemoteTerminalSessionProviding

    public init(provider: any RemoteTerminalSessionProviding) {
        self.provider = provider
    }

    public func connect(
        connectionID: String,
        verificationCode: String?
    ) async throws -> any RemoteTerminalSessionPort {
        try await provider.openRemoteTerminalSession(
            connectionID: connectionID,
            verificationCode: verificationCode
        )
    }
}
