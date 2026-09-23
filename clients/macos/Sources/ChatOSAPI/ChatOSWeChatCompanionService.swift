import Foundation

public struct WeChatBindingStatus: Decodable, Sendable, Equatable {
    public var bound: Bool
    public var createdAt: String?
    public var lastLoginAt: String?

    public init(bound: Bool, createdAt: String?, lastLoginAt: String?) {
        self.bound = bound
        self.createdAt = createdAt
        self.lastLoginAt = lastLoginAt
    }

    enum CodingKeys: String, CodingKey {
        case bound
        case createdAt = "created_at"
        case lastLoginAt = "last_login_at"
    }
}

public struct WeChatBindTicket: Sendable, Equatable {
    public var id: String
    public var scene: String
    public var expiresAtUnix: Int64
    public var codeImageData: Data
}

public struct WeChatBindTicketStatus: Decodable, Sendable, Equatable {
    public var ticketID: String
    public var status: String
    public var expiresAtUnix: Int64
    public var claimedAt: String?

    enum CodingKeys: String, CodingKey {
        case ticketID = "ticket_id"
        case status
        case expiresAtUnix = "expires_at_unix"
        case claimedAt = "claimed_at"
    }
}

public struct WeChatClientSession: Decodable, Sendable, Identifiable, Equatable {
    public var id: String
    public var clientType: String
    public var createdAt: String
    public var lastSeenAt: String
    public var expiresAtUnix: Int64
    public var revokedAt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case clientType = "client_type"
        case createdAt = "created_at"
        case lastSeenAt = "last_seen_at"
        case expiresAtUnix = "expires_at_unix"
        case revokedAt = "revoked_at"
    }
}

public actor ChatOSWeChatCompanionService {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func bindingStatus() async throws -> WeChatBindingStatus {
        try await client.request(
            "/auth/wechat/mini-program/binding",
            service: .userService
        )
    }

    public func issueBindTicket() async throws -> WeChatBindTicket {
        let response: IssueBindTicketResponse = try await client.request(
            "/auth/wechat/mini-program/bind-tickets",
            method: "POST",
            service: .userService
        )
        guard let comma = response.qrCodeDataURL.firstIndex(of: ","),
              response.qrCodeDataURL[..<comma].contains(";base64"),
              let imageData = Data(base64Encoded: String(response.qrCodeDataURL[response.qrCodeDataURL.index(after: comma)...])),
              !imageData.isEmpty else {
            throw ChatOSAPIError.decoding("服务器未返回有效的小程序码")
        }
        return WeChatBindTicket(
            id: response.ticketID,
            scene: response.scene,
            expiresAtUnix: response.expiresAtUnix,
            codeImageData: imageData
        )
    }

    public func bindTicketStatus(id: String) async throws -> WeChatBindTicketStatus {
        try await client.request(
            "/auth/wechat/mini-program/bind-tickets/\(id.pathEncoded)",
            service: .userService
        )
    }

    public func confirmBindTicket(id: String) async throws {
        let _: ConfirmBindTicketResponse = try await client.request(
            "/auth/wechat/mini-program/bind-tickets/\(id.pathEncoded)/confirm",
            method: "POST",
            service: .userService
        )
    }

    public func clientSessions() async throws -> [WeChatClientSession] {
        try await client.request("/auth/client-sessions", service: .userService)
    }

    public func revokeClientSession(id: String) async throws {
        try await client.requestVoid(
            "/auth/client-sessions/\(id.pathEncoded)",
            method: "DELETE",
            service: .userService
        )
    }

    public func unbind() async throws {
        try await client.requestVoid(
            "/auth/wechat/mini-program/binding",
            method: "DELETE",
            service: .userService
        )
    }
}

private struct IssueBindTicketResponse: Decodable, Sendable {
    var ticketID: String
    var scene: String
    var expiresAtUnix: Int64
    var qrCodeDataURL: String

    enum CodingKeys: String, CodingKey {
        case ticketID = "ticket_id"
        case scene
        case expiresAtUnix = "expires_at_unix"
        case qrCodeDataURL = "qr_code_data_url"
    }
}

private struct ConfirmBindTicketResponse: Decodable, Sendable {
    var status: String
}

private extension String {
    var pathEncoded: String {
        addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? self
    }
}
