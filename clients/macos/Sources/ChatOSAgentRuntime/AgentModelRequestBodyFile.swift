import Darwin
import Foundation

struct AgentModelRequestBodyFile: @unchecked Sendable {
    let url: URL
    let byteCount: Int

    private let cleanupOwner: CleanupOwner

    init(url: URL, byteCount: Int) {
        self.url = url
        self.byteCount = byteCount
        cleanupOwner = CleanupOwner(url: url)
    }

    func removeIfNeeded() {
        cleanupOwner.removeIfNeeded()
    }

    private final class CleanupOwner: @unchecked Sendable {
        private let lock = NSLock()
        private let url: URL
        private var removed = false

        init(url: URL) {
            self.url = url
        }

        deinit {
            removeIfNeeded()
        }

        func removeIfNeeded() {
            lock.lock()
            guard !removed else {
                lock.unlock()
                return
            }
            removed = true
            lock.unlock()
            _ = url.withUnsafeFileSystemRepresentation { path in
                path.map(Darwin.unlink) ?? -1
            }
        }
    }
}

struct AgentPreparedModelRequest: @unchecked Sendable {
    let request: URLRequest
    let bodyFile: AgentModelRequestBodyFile?
}
