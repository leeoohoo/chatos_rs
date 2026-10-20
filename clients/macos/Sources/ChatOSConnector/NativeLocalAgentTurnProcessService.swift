import ChatOSCore
import Foundation

public actor NativeLocalAgentTurnProcessService: TurnProcessServicing {
    // Process payloads are projected to bounded summaries by the Host. Keep pages small enough
    // that even a page full of maximum-size tool batches remains below the IPC frame limit.
    private static let eventPageSize: UInt32 = 25
    private static let maximumEventPages = 400

    private let conversationClient: NativeLocalAgentConversationClient
    private let taskClient: NativeLocalAgentTaskClient
    private var ownerUserID: String?

    public init(host: any LocalAgentHostClientServicing) {
        conversationClient = NativeLocalAgentConversationClient(host: host)
        taskClient = NativeLocalAgentTaskClient(host: host)
    }

    public func configure(ownerUserID: String) {
        self.ownerUserID = ownerUserID
    }

    public func reset() {
        ownerUserID = nil
    }

    public func fetchProcessNodes(
        sessionID: String,
        turnID: String
    ) async throws -> [TurnProcessNode] {
        let ownerUserID = try configuredOwner()
        let conversation = try await conversationClient.context(
            ownerUserID: ownerUserID,
            conversationID: sessionID
        )
        guard let turn = conversation.turns.first(where: {
            $0.turnID == turnID && $0.conversationID == sessionID
        }) else {
            throw NativeLocalAgentTurnProcessServiceError.turnNotFound
        }
        var cursor: Int64 = 0
        var events: [LocalAgentEventRecord] = []
        for _ in 0..<Self.maximumEventPages {
            let page = try await taskClient.events(
                ownerUserID: ownerUserID,
                runID: turn.runID,
                afterCursor: cursor,
                limit: Self.eventPageSize,
                payloadMode: .process
            )
            events.append(contentsOf: page.events)
            guard page.events.count == Int(Self.eventPageSize),
                  page.nextCursor > cursor else { break }
            cursor = page.nextCursor
        }
        return LocalAgentTurnProcessMapper.map(events)
    }

    private func configuredOwner() throws -> String {
        guard let ownerUserID else {
            throw NativeLocalAgentTurnProcessServiceError.notConfigured
        }
        return ownerUserID
    }
}

public enum NativeLocalAgentTurnProcessServiceError: LocalizedError {
    case notConfigured
    case turnNotFound

    public var errorDescription: String? {
        switch self {
        case .notConfigured: "Local Agent Turn Process service is not configured."
        case .turnNotFound: "The local conversation Turn no longer exists."
        }
    }
}
