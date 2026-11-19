import ChatOSProcessRuntime
import Darwin
import Dispatch
import Foundation

final class NativeProcessController: @unchecked Sendable {
    private static let forceKillQueue = DispatchQueue(
        label: "com.chatos.native-process-termination",
        qos: .utility,
        attributes: .concurrent
    )

    private let lock = NSLock()
    private var cancellationRequested = false
    private var activeProcessID: pid_t?
    private var activeRegistration: UUID?

    var isCancelled: Bool {
        lock.lock()
        defer { lock.unlock() }
        return cancellationRequested
    }

    func throwIfCancelled() throws {
        if isCancelled { throw CancellationError() }
    }

    func register(_ processID: pid_t) -> UUID {
        let registration = UUID()
        lock.lock()
        activeProcessID = processID
        activeRegistration = registration
        let cancelled = cancellationRequested
        lock.unlock()
        if cancelled {
            requestTermination(of: processID, registration: registration)
        }
        return registration
    }

    func clear(_ processID: pid_t, registration: UUID) {
        lock.lock()
        if activeProcessID == processID, activeRegistration == registration {
            activeProcessID = nil
            activeRegistration = nil
        }
        lock.unlock()
    }

    func cancel() {
        lock.lock()
        cancellationRequested = true
        let processID = activeProcessID
        let registration = activeRegistration
        lock.unlock()
        if let processID, let registration {
            requestTermination(of: processID, registration: registration)
        }
    }

    private func requestTermination(of processID: pid_t, registration: UUID) {
        _ = chatos_signal_process_group(processID, SIGTERM)
        Self.forceKillQueue.asyncAfter(deadline: .now() + 2) { [weak self] in
            guard let self, self.owns(processID, registration: registration) else { return }
            _ = chatos_signal_process_group(processID, SIGKILL)
        }
    }

    private func owns(_ processID: pid_t, registration: UUID) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return activeProcessID == processID && activeRegistration == registration
    }
}

typealias NativeOpenSSHProcessController = NativeProcessController
typealias NativeGitProcessController = NativeProcessController
typealias NativeNPMProcessController = NativeProcessController
typealias NativePluginPermissionProcessController = NativeProcessController
