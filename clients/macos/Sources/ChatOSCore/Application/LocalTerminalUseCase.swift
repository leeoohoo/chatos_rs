import Foundation

public enum LocalTerminalEvent: Sendable, Equatable {
    case output(Data)
    case terminated(exitCode: Int32?)
    case failed(String)
}

public protocol LocalTerminalSessionPort: AnyObject, Sendable {
    var isRunning: Bool { get }
    var shellName: String { get }
    func events() -> AsyncStream<LocalTerminalEvent>
    func start(columns: Int, rows: Int)
    func send(_ data: Data)
    func resize(columns: Int, rows: Int)
    func interrupt()
    func close()
}

public protocol LocalTerminalSessionProviding: Sendable {
    func makeLocalTerminalSession(
        workingDirectory: String
    ) throws -> any LocalTerminalSessionPort
}

public struct LocalTerminalUseCase: Sendable {
    private let provider: any LocalTerminalSessionProviding

    public init(provider: any LocalTerminalSessionProviding) {
        self.provider = provider
    }

    public func open(
        workingDirectory: String
    ) throws -> any LocalTerminalSessionPort {
        try provider.makeLocalTerminalSession(workingDirectory: workingDirectory)
    }
}

public struct LocalTerminalSessionToken: Sendable, Equatable {
    public let terminalID: UUID
    public let generation: UInt64
    public let requestID: UUID

    public init(terminalID: UUID, generation: UInt64, requestID: UUID) {
        self.terminalID = terminalID
        self.generation = generation
        self.requestID = requestID
    }
}

/// Event-stream identity for one local PTY. Closing or replacing the session
/// invalidates all output and termination events from the previous process.
public struct LocalTerminalLifecycleState: Sendable, Equatable {
    public private(set) var activeSession: LocalTerminalSessionToken?
    private var generation: UInt64 = 0

    public init() {}

    public mutating func begin(terminalID: UUID) -> LocalTerminalSessionToken {
        generation &+= 1
        let token = LocalTerminalSessionToken(
            terminalID: terminalID,
            generation: generation,
            requestID: UUID()
        )
        activeSession = token
        return token
    }

    public func accepts(_ token: LocalTerminalSessionToken) -> Bool {
        activeSession == token
    }

    @discardableResult
    public mutating func finish(_ token: LocalTerminalSessionToken) -> Bool {
        guard accepts(token) else { return false }
        activeSession = nil
        return true
    }

    public mutating func reset() {
        generation &+= 1
        activeSession = nil
    }
}
