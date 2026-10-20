public struct TurnProcessUseCase: Sendable {
    private let service: any TurnProcessServicing

    public init(service: any TurnProcessServicing) {
        self.service = service
    }

    public func load(
        sessionID: String,
        turnID: String
    ) async throws -> [TurnProcessNode] {
        try await service.fetchProcessNodes(
            sessionID: sessionID,
            turnID: turnID
        )
    }
}
