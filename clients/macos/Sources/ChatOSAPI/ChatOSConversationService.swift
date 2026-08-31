import ChatOSCore
import Foundation

public struct ChatOSConversationService: ConversationRemoteServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetchHistory(_ query: ConversationHistoryQuery) async throws -> HistoryPage {
        let sessionID = query.sessionID.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed)
            ?? query.sessionID
        var components = URLComponents()
        components.queryItems = [
            URLQueryItem(name: "limit", value: String(max(1, query.limit))),
            query.before.map { URLQueryItem(name: "before", value: $0) },
        ].compactMap { $0 }
        let queryString = components.percentEncodedQuery.map { "?\($0)" } ?? ""
        let response: CompactHistoryResponseDTO = try await client.request(
            "/conversations/\(sessionID)/compact-history\(queryString)"
        )
        return ConversationHistoryMapper.map(
            response,
            sessionID: query.sessionID,
            requestGeneration: query.requestGeneration
        )
    }

    public func issueWebSocketTicket() async throws -> String {
        let response: WebSocketTicketDTO = try await client.request(
            "/auth/ws-ticket",
            method: "POST"
        )
        let ticket = response.ticket.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !ticket.isEmpty else {
            throw ChatOSAPIError.missingWebSocketTicket
        }
        return ticket
    }
}
