import Foundation

@MainActor
final class AgentChangeRefreshCoalescer {
    private let delay: Duration
    private let refresh: @MainActor () async -> Void
    private var refreshTask: Task<Void, Never>?
    private var receivedChangeDuringRefresh = false

    init(
        delay: Duration = .milliseconds(120),
        refresh: @escaping @MainActor () async -> Void
    ) {
        self.delay = delay
        self.refresh = refresh
    }

    func signal() {
        if refreshTask != nil {
            receivedChangeDuringRefresh = true
            return
        }
        refreshTask = Task { [weak self] in
            await self?.runRefreshLoop()
        }
    }

    func cancel() {
        refreshTask?.cancel()
        refreshTask = nil
        receivedChangeDuringRefresh = false
    }

    private func runRefreshLoop() async {
        while !Task.isCancelled {
            do {
                try await Task.sleep(for: delay)
            } catch {
                break
            }
            receivedChangeDuringRefresh = false
            await refresh()
            guard receivedChangeDuringRefresh else { break }
        }
        refreshTask = nil
        receivedChangeDuringRefresh = false
    }
}
