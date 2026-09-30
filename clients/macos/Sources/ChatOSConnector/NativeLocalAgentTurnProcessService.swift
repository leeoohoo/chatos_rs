import ChatOSCore
import Foundation

public actor NativeLocalAgentTurnProcessService: TurnProcessServicing {
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
        let conversation = try await conversationClient.get(
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
        for _ in 0..<20 {
            let page = try await taskClient.events(
                ownerUserID: ownerUserID,
                runID: turn.runID,
                afterCursor: cursor,
                limit: 500
            )
            events.append(contentsOf: page.events)
            guard page.events.count == 500, page.nextCursor > cursor else { break }
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
