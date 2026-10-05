import ChatOSCore
import Foundation

enum LocalConnectorApprovalMonitoringPolicy {
    static func consistencyCheckInterval(hasStreamingService: Bool) -> Duration {
        hasStreamingService ? .seconds(60) : .seconds(2)
    }
}

extension LocalConnectorControlCenterViewModel {
    nonisolated static func fetchStatusWithStartupRetry(
        service: any LocalConnectorControlServicing
    ) async throws -> LocalConnectorStatus {
        var lastError: Error?
        for attempt in 0..<20 {
            do {
                return try await service.fetchStatus()
            } catch {
                try Task.checkCancellation()
                lastError = error
                if attempt < 19 {
                    try await Task.sleep(for: .milliseconds(150))
                }
            }
        }
        throw lastError ?? URLError(.cannotConnectToHost)
    }
}
