import ChatOSProcessRuntime
import Darwin
import Foundation

enum NativeProcessPipeReader {
    static func install(
        on handle: FileHandle,
        onData: @escaping @Sendable (Data) -> Void = { _ in },
        onEOF: @escaping @Sendable () -> Void = {}
    ) {
        handle.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty else {
                // FileHandle repeatedly reports a pipe at EOF as readable. The
                // handler must be removed here rather than relying on a process
                // termination callback that can be delayed across system sleep.
                handle.readabilityHandler = nil
                onEOF()
                return
            }
            onData(data)
        }
    }
}

/// Thread-safe, fixed-size capture used by short-lived child processes. The
/// pipe must keep draining even after the retained prefix is full so the child
/// can never block on a full stdout or stderr pipe.
final class NativeBoundedProcessOutput: @unchecked Sendable {
    private let lock = NSLock()
    private let maximumBytes: Int
    private var storage = Data()
    private var didDiscardData = false

    init(maximumBytes: Int) {
        self.maximumBytes = max(1, maximumBytes)
    }

    func append(_ data: Data) {
        guard !data.isEmpty else { return }
        lock.lock()
        let remaining = max(0, maximumBytes - storage.count)
        storage.append(data.prefix(remaining))
        if data.count > remaining {
            didDiscardData = true
        }
        lock.unlock()
    }

    var snapshot: (data: Data, discarded: Bool) {
        lock.lock()
        let result = (storage, didDiscardData)
        lock.unlock()
        return result
    }
}

/// A one-shot process exit notification. Waiting uses a kernel-backed dispatch
/// primitive instead of repeatedly sampling `isRunning` on a timer.
final class NativeProcessExitSignal: @unchecked Sendable {
    private let group = DispatchGroup()
    private let lock = NSLock()
    private var exitCode: Int32?

    init() {
        group.enter()
    }

    func complete(exitCode: Int32) {
        lock.lock()
        guard self.exitCode == nil else {
            lock.unlock()
            return
        }
        self.exitCode = exitCode
        lock.unlock()
        group.leave()
    }

    func wait(timeout: TimeInterval) -> Int32? {
        let result = group.wait(timeout: .now() + max(0, timeout))
        guard result == .success else { return nil }
        lock.lock()
        let value = exitCode
        lock.unlock()
        return value
    }

    func waitAsync(timeout: TimeInterval) async -> Int32? {
        await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .utility).async {
                continuation.resume(returning: self.wait(timeout: timeout))
            }
        }
    }

    static func install(on process: Process) -> NativeProcessExitSignal {
        let signal = NativeProcessExitSignal()
        process.terminationHandler = { terminated in
            signal.complete(exitCode: terminated.terminationStatus)
        }
        return signal
    }

    static func reap(processID: pid_t) -> NativeProcessExitSignal {
        let signal = NativeProcessExitSignal()
        DispatchQueue.global(qos: .utility).async {
            var exitCode: Int32 = 1
            let result = chatos_reap_process(processID, &exitCode)
            signal.complete(exitCode: result == 0 ? exitCode : 1)
        }
        return signal
    }
}
