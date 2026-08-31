import ChatOSCore
import Foundation

public actor ChatOSAuthenticationService: AuthenticationServicing {
    private let client: ChatOSAPIClient
    private let credentialStore: any CredentialStoring
    private let encoder = JSONEncoder()

    public init(
        client: ChatOSAPIClient,
        credentialStore: any CredentialStoring
    ) {
        self.client = client
        self.credentialStore = credentialStore
    }

    public func restoreSession() async throws -> AuthSession? {
        guard let token = try await credentialStore.loadAccessToken()?.trimmedNonEmpty else {
            return nil
        }

        do {
            try await client.setAccessToken(token)
            let response: MeResponseDTO = try await client.request("/auth/me")
            return AuthSession(user: response.user.domainModel)
        } catch ChatOSAPIError.unauthorized {
            try? await client.setAccessToken(nil)
            return nil
        }
    }

    public func login(username: String, password: String) async throws -> AuthSession {
        let username = username.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !username.isEmpty, !password.isEmpty else {
            throw ChatOSAPIError.invalidCredentials
        }

        let body = try encoder.encode(LoginRequestDTO(username: username, password: password))
        let response: LoginResponseDTO = try await client.request(
            "/auth/login",
            method: "POST",
            body: body
        )
        try await client.setAccessToken(response.accessToken)
        return AuthSession(user: response.user.domainModel)
    }

    public func logout() async {
        try? await client.setAccessToken(nil)
    }
}

private struct LoginRequestDTO: Encodable {
    var username: String
    var password: String
}

private struct LoginResponseDTO: Decodable, Sendable {
    var accessToken: String
    var user: AuthUserDTO

    enum CodingKeys: String, CodingKey {
        case accessToken = "access_token"
        case user
    }
}

private struct MeResponseDTO: Decodable, Sendable {
    var user: AuthUserDTO
}

private struct AuthUserDTO: Decodable, Sendable {
    var id: String
    var username: String
    var displayName: String?
    var role: String

    enum CodingKeys: String, CodingKey {
        case id
        case username
        case displayName = "display_name"
        case role
    }

    var domainModel: AuthUser {
        AuthUser(
            id: id,
            username: username,
            displayName: displayName,
            role: role
        )
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
