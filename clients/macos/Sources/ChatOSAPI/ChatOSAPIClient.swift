import ChatOSCore
import Foundation

public actor ChatOSAPIClient {
    public struct Configuration: Sendable {
        public var baseURL: URL
        public var clientSurface: String

        public init(
            baseURL: URL,
            clientSurface: String = "local-connector-desktop"
        ) {
            self.baseURL = baseURL
            self.clientSurface = clientSurface
        }
    }

    private let configuration: Configuration
    private let transport: any HTTPTransport
    private let credentialStore: (any CredentialStoring)?
    private let decoder: JSONDecoder
    private var accessToken: String?

    public init(
        configuration: Configuration,
        accessToken: String? = nil,
        credentialStore: (any CredentialStoring)? = nil,
        transport: any HTTPTransport = URLSessionHTTPTransport()
    ) {
        self.configuration = configuration
        self.accessToken = accessToken?.trimmedNonEmpty
        self.credentialStore = credentialStore
        self.transport = transport
        self.decoder = JSONDecoder()
    }

    public func setAccessToken(_ token: String?) async throws {
        accessToken = token?.trimmedNonEmpty
        if let accessToken {
            try await credentialStore?.saveAccessToken(accessToken)
        } else {
            try await credentialStore?.deleteAccessToken()
        }
    }

    public func currentAccessToken() -> String? {
        accessToken
    }

    public func webSocketURL(path: String, ticket: String) -> URL? {
        let base = configuration.baseURL.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let cleanedPath = path.hasPrefix("/") ? path : "/\(path)"
        guard var components = URLComponents(string: base + cleanedPath) else { return nil }
        components.scheme = components.scheme == "https" ? "wss" : "ws"
        components.queryItems = [URLQueryItem(name: "ws_ticket", value: ticket)]
        return components.url
    }

    func request<Response: Decodable & Sendable>(
        _ endpoint: String,
        method: String = "GET",
        body: Data? = nil,
        additionalHeaders: [String: String] = [:],
        timeoutInterval: TimeInterval? = nil
    ) async throws -> Response {
        guard let url = makeURL(endpoint: endpoint) else {
            throw ChatOSAPIError.invalidEndpoint
        }

        let requestAccessToken = accessToken
        var headers = [
            "Accept": "application/json",
            "Content-Type": "application/json",
            "X-Chatos-Client-Surface": configuration.clientSurface,
        ]
        if let requestAccessToken {
            headers["Authorization"] = "Bearer \(requestAccessToken)"
        }
        additionalHeaders.forEach { headers[$0] = $1 }

        let response = try await transport.send(
            HTTPRequest(
                url: url,
                method: method,
                headers: headers,
                body: body,
                timeoutInterval: timeoutInterval
            )
        )
        if let refreshedToken = response.headers["x-access-token"]?.trimmedNonEmpty {
            accessToken = refreshedToken
            try await credentialStore?.saveAccessToken(refreshedToken)
        }
        if response.statusCode == 401 {
            if let requestAccessToken, accessToken == requestAccessToken {
                accessToken = nil
                try? await credentialStore?.deleteAccessToken()
                NotificationCenter.default.post(
                    name: .chatOSAuthenticationDidExpire,
                    object: nil
                )
            }
            throw ChatOSAPIError.unauthorized
        }
        guard (200..<300).contains(response.statusCode) else {
            let payload = APIErrorPayload.decode(
                response.body,
                statusCode: response.statusCode
            )
            if payload.code != nil || payload.challengePrompt != nil {
                throw ChatOSAPIError.serverDetail(
                    statusCode: response.statusCode,
                    message: payload.resolvedMessage,
                    code: payload.code,
                    challengePrompt: payload.challengePrompt
                )
            }
            throw ChatOSAPIError.server(
                statusCode: response.statusCode,
                message: payload.resolvedMessage
            )
        }

        do {
            return try decoder.decode(Response.self, from: response.body)
        } catch {
            throw ChatOSAPIError.decoding(error.localizedDescription)
        }
    }

    private func makeURL(endpoint: String) -> URL? {
        let base = configuration.baseURL.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let cleanedEndpoint = endpoint.hasPrefix("/") ? endpoint : "/\(endpoint)"
        return URL(string: base + cleanedEndpoint)
    }
}

private struct APIErrorPayload: Decodable {
    var message: String?
    var error: String?
    var code: String?
    var challengePrompt: String?

    enum CodingKeys: String, CodingKey {
        case message, error, code
        case challengePrompt = "challenge_prompt"
    }

    var resolvedMessage: String { message ?? error ?? "Request failed" }

    static func decode(_ data: Data, statusCode: Int) -> Self {
        if let payload = try? JSONDecoder().decode(Self.self, from: data) {
            return payload
        }

        if let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let nested = object["error"] as? [String: Any] {
            return .init(
                message: nested["message"] as? String,
                error: nil,
                code: nested["code"] as? String,
                challengePrompt: nested["challenge_prompt"] as? String
            )
        }

        let rawText = String(decoding: data, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if rawText.localizedCaseInsensitiveContains("<html")
            || rawText.localizedCaseInsensitiveContains("<!doctype") {
            return .init(message: friendlyGatewayMessage(statusCode: statusCode))
        }
        return .init(message: rawText.isEmpty ? friendlyGatewayMessage(statusCode: statusCode) : rawText)
    }

    private static func friendlyGatewayMessage(statusCode: Int) -> String {
        switch statusCode {
        case 502:
            "服务网关暂时无法连接后端，请稍后重试。"
        case 503:
            "服务正在启动或暂时不可用，请稍后重试。"
        case 504:
            "服务响应超时，请稍后重试。"
        default:
            "服务器请求失败，请稍后重试。"
        }
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
