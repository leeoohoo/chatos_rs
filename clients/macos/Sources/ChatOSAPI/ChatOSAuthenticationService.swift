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
            let response: MeResponseDTO = try await client.request(
                "/auth/me",
                service: .userService
            )
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
            body: body,
            service: .userService
        )
        try await client.setAccessToken(response.accessToken)
        return AuthSession(user: response.user.domainModel)
    }

    public func sendRegistrationCode(
        email: String,
        inviteCode: String
    ) async throws -> RegistrationCodeDelivery {
        let email = email.trimmingCharacters(in: .whitespacesAndNewlines)
        let inviteCode = inviteCode.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !email.isEmpty, !inviteCode.isEmpty else {
            throw ChatOSAPIError.invalidRequest("请输入邮箱和邀请码。")
        }
        let body = try encoder.encode(SendRegistrationCodeRequestDTO(
            email: email,
            inviteCode: inviteCode
        ))
        let response: SendRegistrationCodeResponseDTO = try await client.request(
            "/auth/register/send-code",
            method: "POST",
            body: body,
            service: .userService
        )
        return RegistrationCodeDelivery(
            expiresInSeconds: response.expiresInSeconds,
            resendAfterSeconds: response.resendAfterSeconds
        )
    }

    public func register(
        email: String,
        password: String,
        inviteCode: String,
        verificationCode: String
    ) async throws -> AuthSession {
        let email = email.trimmingCharacters(in: .whitespacesAndNewlines)
        let inviteCode = inviteCode.trimmingCharacters(in: .whitespacesAndNewlines)
        let verificationCode = verificationCode.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !email.isEmpty, !password.isEmpty, !inviteCode.isEmpty, !verificationCode.isEmpty else {
            throw ChatOSAPIError.invalidRequest("请填写完整的注册信息。")
        }
        let body = try encoder.encode(RegisterRequestDTO(
            email: email,
            password: password,
            inviteCode: inviteCode,
            verificationCode: verificationCode
        ))
        let response: LoginResponseDTO = try await client.request(
            "/auth/register",
            method: "POST",
            body: body,
            service: .userService
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

private struct SendRegistrationCodeRequestDTO: Encodable {
    var email: String
    var inviteCode: String

    enum CodingKeys: String, CodingKey {
        case email
        case inviteCode = "invite_code"
    }
}

private struct SendRegistrationCodeResponseDTO: Decodable, Sendable {
    var expiresInSeconds: Int
    var resendAfterSeconds: Int

    enum CodingKeys: String, CodingKey {
        case expiresInSeconds = "expires_in_seconds"
        case resendAfterSeconds = "resend_after_seconds"
    }
}

private struct RegisterRequestDTO: Encodable {
    var email: String
    var password: String
    var inviteCode: String
    var verificationCode: String

    enum CodingKeys: String, CodingKey {
        case email, password
        case inviteCode = "invite_code"
        case verificationCode = "verification_code"
    }
}

private struct LoginResponseDTO: Decodable, Sendable {
    var accessToken: String
    var user: AuthUserDTO

    enum CodingKeys: String, CodingKey {
        case token
        case accessToken = "access_token"
        case user
    }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        accessToken = try values.decodeIfPresent(String.self, forKey: .token)
            ?? values.decode(String.self, forKey: .accessToken)
        user = try values.decode(AuthUserDTO.self, forKey: .user)
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
