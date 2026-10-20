public struct ConversationRealtimeUseCase: Sendable {
    private let service: any ConversationRealtimeStreaming

    public init(service: any ConversationRealtimeStreaming) {
        self.service = service
    }

    public func events(
        sessionID: String
    ) async -> AsyncThrowingStream<ConversationRealtimeSignal, Error> {
        await service.events(sessionID: sessionID)
    }

    public func action(
        for signal: ConversationRealtimeSignal
    ) -> ConversationRealtimeAction {
        ConversationRealtimePolicy.action(for: signal)
    }
}
