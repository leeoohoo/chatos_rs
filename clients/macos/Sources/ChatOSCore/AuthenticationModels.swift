import Foundation

public extension Notification.Name {
    /// Posted after an authenticated ChatOS or Local Connector request proves
    /// that the currently stored user token is no longer valid.
    static let chatOSAuthenticationDidExpire = Notification.Name(
        "com.chatos.swift.authentication-did-expire"
    )
}

public struct AuthUser: Codable, Sendable, Equatable {
    public var id: String
    public var username: String
    public var displayName: String?
    public var role: String

    public init(
        id: String,
        username: String,
        displayName: String? = nil,
        role: String
    ) {
        self.id = id
        self.username = username
        self.displayName = displayName
        self.role = role
    }
}

public struct AuthSession: Sendable, Equatable {
    public var user: AuthUser

    public init(user: AuthUser) {
        self.user = user
    }
}

public struct RegistrationCodeDelivery: Sendable, Equatable {
    public var expiresInSeconds: Int
    public var resendAfterSeconds: Int

    public init(expiresInSeconds: Int, resendAfterSeconds: Int) {
        self.expiresInSeconds = expiresInSeconds
        self.resendAfterSeconds = resendAfterSeconds
    }
}

public protocol AuthenticationServicing: Sendable {
    func restoreSession() async throws -> AuthSession?
    func login(username: String, password: String) async throws -> AuthSession
    func sendRegistrationCode(email: String, inviteCode: String) async throws -> RegistrationCodeDelivery
    func register(
        email: String,
        password: String,
        inviteCode: String,
        verificationCode: String
    ) async throws -> AuthSession
    func logout() async
}
