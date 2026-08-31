import ChatOSCore

public actor ChatOSLocalConnectorPairingTicketProvider: LocalConnectorPairingTicketProviding {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func issueLocalConnectorPairingTicket() async throws -> String {
        let response: PairingTicketResponse = try await client.request(
            "/auth/local-connector-ticket",
            method: "POST"
        )
        return response.ticket
    }
}

private struct PairingTicketResponse: Decodable, Sendable {
    var ticket: String
}
