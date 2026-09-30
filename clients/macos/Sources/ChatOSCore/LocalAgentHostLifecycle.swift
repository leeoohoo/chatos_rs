import Foundation

public protocol LocalAgentHostLifecycleServicing: Sendable {
    func start(ownerUserID: String) async throws

    func stop() async
}
