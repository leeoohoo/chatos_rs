import Testing
@testable import ChatOSCore

@Suite("ConversationRealtimeUseCase")
struct ConversationRealtimeUseCaseTests {
    @Test
    func forwardsSessionAndStreamsSignals() async throws {
        let signal = Self.makeSignal(kind: .updated)
        let service = RealtimeServiceSpy(signal: signal)
        let useCase = ConversationRealtimeUseCase(service: service)

        let stream = await useCase.events(sessionID: "session-1")
        var received: [ConversationRealtimeSignal] = []
        for try await event in stream {
            received.append(event)
        }

        #expect(received == [signal])
        #expect(await service.requestedSessionIDs == ["session-1"])
    }

    @Test
    func mapsSignalsToApplicationActions() {
        let useCase = ConversationRealtimeUseCase(
            service: RealtimeServiceSpy(signal: Self.makeSignal(kind: .updated))
        )

        #expect(useCase.action(for: Self.makeSignal(kind: .completed)) == .refreshHistory)
        #expect(useCase.action(for: Self.makeSignal(kind: .updated)) == .ignore)
    }

    private static func makeSignal(
        kind: ConversationRealtimeKind
    ) -> ConversationRealtimeSignal {
        ConversationRealtimeSignal(
            eventID: "event-1",
            eventSequence: 1,
            sessionID: "session-1",
            turnID: "turn-1",
            kind: kind,
            eventName: "turn.updated",
            timestamp: "2026-10-20T00:00:00Z"
        )
    }
}

private actor RealtimeServiceSpy: ConversationRealtimeStreaming {
    private let signal: ConversationRealtimeSignal
    private(set) var requestedSessionIDs: [String] = []

    init(signal: ConversationRealtimeSignal) {
        self.signal = signal
    }

    func events(
        sessionID: String
    ) async -> AsyncThrowingStream<ConversationRealtimeSignal, Error> {
        requestedSessionIDs.append(sessionID)
        let signal = signal
        return AsyncThrowingStream { continuation in
            continuation.yield(signal)
            continuation.finish()
        }
    }
}
