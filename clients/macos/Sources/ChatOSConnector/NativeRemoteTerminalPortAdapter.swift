import ChatOSCore
import Foundation

extension NativeRemoteConnectionService: RemoteTerminalSessionProviding {
    public func openRemoteTerminalSession(
        connectionID: String,
        verificationCode: String?
    ) async throws -> any RemoteTerminalSessionPort {
        let session = try await makeRemoteTerminalSession(
            connectionID: connectionID,
            verificationCode: verificationCode
        )
        return NativeRemoteTerminalPortAdapter(session: session)
    }
}

/// Converts the delegate-based native PTY into the Core byte-stream Port.
/// Credential paths, OpenSSH arguments, and process handles remain inside the
/// Connector module.
private final class NativeRemoteTerminalPortAdapter: RemoteTerminalSessionPort,
    @unchecked Sendable {
    private let session: NativeRemoteTerminalSession
    private let stream: AsyncStream<RemoteTerminalEvent>
    private let continuation: AsyncStream<RemoteTerminalEvent>.Continuation
    private let observer: NativeRemoteTerminalPortObserver

    init(session: NativeRemoteTerminalSession) {
        self.session = session
        let pair = AsyncStream<RemoteTerminalEvent>.makeStream(
            bufferingPolicy: .bufferingNewest(256)
        )
        self.stream = pair.stream
        self.continuation = pair.continuation
        self.observer = NativeRemoteTerminalPortObserver(
            onOutput: { pair.continuation.yield(.output($0)) },
            onTermination: {
                pair.continuation.yield(.terminated(exitCode: $0))
                pair.continuation.finish()
            }
        )
        session.delegate = observer
    }

    var isRunning: Bool { session.running }

    func events() -> AsyncStream<RemoteTerminalEvent> { stream }
    func start(columns: Int, rows: Int) { session.start(columns: columns, rows: rows) }
    func send(_ data: Data) { session.send(data) }
    func resize(columns: Int, rows: Int) { session.resize(columns: columns, rows: rows) }
    func interrupt() { session.interrupt() }

    func close() {
        session.delegate = nil
        session.close()
        continuation.finish()
    }

    deinit {
        close()
    }
}

private final class NativeRemoteTerminalPortObserver: NSObject,
    NativeRemoteTerminalSessionDelegate {
    private let onOutput: @Sendable (Data) -> Void
    private let onTermination: @Sendable (Int32?) -> Void

    init(
        onOutput: @escaping @Sendable (Data) -> Void,
        onTermination: @escaping @Sendable (Int32?) -> Void
    ) {
        self.onOutput = onOutput
        self.onTermination = onTermination
    }

    func remoteTerminalSession(
        _ session: NativeRemoteTerminalSession,
        didReceive data: Data
    ) {
        onOutput(data)
    }

    func remoteTerminalSession(
        _ session: NativeRemoteTerminalSession,
        didTerminateWith exitCode: Int32?
    ) {
        onTermination(exitCode)
    }
}
