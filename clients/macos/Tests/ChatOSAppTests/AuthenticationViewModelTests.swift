import ChatOSCore
import Foundation
import Testing
@testable import ChatOSApp

@Suite("Authentication registration")
@MainActor
struct AuthenticationViewModelTests {
    @Test("session restore retries after a transient startup failure")
    func retriesFailedSessionRestore() async {
        let service = AuthenticationRestoreTestService()
        let viewModel = AuthenticationViewModel(service: service)

        viewModel.start()
        await waitUntil { viewModel.phase == .signedOut }
        #expect(await service.restoreCallCount() == 1)

        viewModel.retrySessionRestoreIfNeeded()
        await waitUntil {
            if case .authenticated = viewModel.phase { return true }
            return false
        }

        guard case let .authenticated(session) = viewModel.phase else {
            Issue.record("Expected the retry to restore the session")
            return
        }
        #expect(session.user.id == "restored-user")
        #expect(await service.restoreCallCount() == 2)
    }

    @Test("registration validates matching passwords before calling the service")
    func rejectsMismatchedPasswords() async {
        let service = AuthenticationRegistrationTestService()
        let viewModel = AuthenticationViewModel(service: service)
        viewModel.showRegistration()
        fillValidRegistration(in: viewModel)
        viewModel.confirmPassword = "different-password"

        viewModel.register()

        #expect(viewModel.errorMessage == "两次输入的密码不一致。")
        let calls = await service.registerCallCount()
        #expect(calls == 0)
    }

    @Test("sending a code starts the server-provided resend countdown")
    func sendsRegistrationCode() async {
        let service = AuthenticationRegistrationTestService()
        let viewModel = AuthenticationViewModel(service: service)
        viewModel.username = "person@example.com"
        viewModel.inviteCode = "invite-123"

        viewModel.sendRegistrationCode()
        await waitUntil { !viewModel.isSendingRegistrationCode }

        #expect(viewModel.registrationMessage == "验证码已发送，请查看邮箱。")
        #expect(viewModel.registrationCodeCountdown == 60)
        let calls = await service.codeCallCount()
        #expect(calls == 1)
    }

    @Test("successful registration authenticates the new account")
    func registrationAuthenticates() async {
        let service = AuthenticationRegistrationTestService()
        let viewModel = AuthenticationViewModel(service: service)
        viewModel.showRegistration()
        fillValidRegistration(in: viewModel)

        viewModel.register()
        await waitUntil {
            if case .authenticated = viewModel.phase { return true }
            return false
        }

        guard case let .authenticated(session) = viewModel.phase else {
            Issue.record("Expected authenticated phase")
            return
        }
        #expect(session.user.username == "person@example.com")
        #expect(viewModel.password.isEmpty)
        #expect(viewModel.confirmPassword.isEmpty)
    }

    @Test("a late login response cannot authenticate after logout")
    func ignoresLateLoginResponseAfterLogout() async {
        let service = DelayedAuthenticationTestService()
        let viewModel = AuthenticationViewModel(service: service)
        viewModel.start()
        await waitUntil { viewModel.phase == .signedOut }
        viewModel.username = "person@example.com"
        viewModel.password = "secret-value"

        viewModel.login()
        await waitUntil {
            let hasPendingLogin = await service.hasPendingLogin()
            return viewModel.phase == .authenticating && hasPendingLogin
        }
        viewModel.logout()
        try? await Task.sleep(for: .milliseconds(20))
        #expect(await service.logoutCallCount() == 0)
        await service.resumeLogin()
        await waitUntil { await service.loginDidReturn() }
        await waitUntil { await service.logoutCallCount() == 1 }

        #expect(viewModel.phase == .signedOut)
        #expect(viewModel.password.isEmpty)
        #expect(await service.hasActiveCredential() == false)
    }

    @Test("a new login waits for the previous logout to clear credentials")
    func loginWaitsForPendingLogout() async {
        let service = LogoutOrderingAuthenticationTestService()
        let viewModel = AuthenticationViewModel(service: service)
        viewModel.start()
        await waitUntil { viewModel.phase == .signedOut }

        viewModel.logout()
        await waitUntil { await service.hasPendingLogout() }
        viewModel.username = "person@example.com"
        viewModel.password = "secret-value"
        viewModel.login()
        try? await Task.sleep(for: .milliseconds(20))

        #expect(await service.loginCallCount() == 0)
        await service.resumeLogout()
        await waitUntil {
            if case .authenticated = viewModel.phase { return true }
            return false
        }

        #expect(await service.loginCallCount() == 1)
        #expect(await service.hasActiveCredential())
    }

    private func fillValidRegistration(in viewModel: AuthenticationViewModel) {
        viewModel.username = "person@example.com"
        viewModel.inviteCode = "invite-123"
        viewModel.verificationCode = "123456"
        viewModel.password = "secret-value"
        viewModel.confirmPassword = "secret-value"
    }

    private func waitUntil(_ condition: @escaping @MainActor () async -> Bool) async {
        for _ in 0..<50 {
            if await condition() { return }
            try? await Task.sleep(for: .milliseconds(10))
        }
    }
}

private actor LogoutOrderingAuthenticationTestService: AuthenticationServicing {
    private var logoutContinuation: CheckedContinuation<Void, Never>?
    private var loginCalls = 0
    private var credentialActive = false

    func restoreSession() async throws -> AuthSession? { nil }

    func login(username: String, password: String) async throws -> AuthSession {
        loginCalls += 1
        credentialActive = true
        return .init(user: .init(id: "new-user", username: username, role: "user"))
    }

    func sendRegistrationCode(
        email: String,
        inviteCode: String
    ) async throws -> RegistrationCodeDelivery {
        .init(expiresInSeconds: 600, resendAfterSeconds: 60)
    }

    func register(
        email: String,
        password: String,
        inviteCode: String,
        verificationCode: String
    ) async throws -> AuthSession {
        credentialActive = true
        return .init(user: .init(id: "new-user", username: email, role: "user"))
    }

    func logout() async {
        await withCheckedContinuation { continuation in
            logoutContinuation = continuation
        }
        credentialActive = false
    }

    func hasPendingLogout() -> Bool { logoutContinuation != nil }
    func loginCallCount() -> Int { loginCalls }
    func hasActiveCredential() -> Bool { credentialActive }

    func resumeLogout() {
        logoutContinuation?.resume()
        logoutContinuation = nil
    }
}

private actor DelayedAuthenticationTestService: AuthenticationServicing {
    private var loginContinuation: CheckedContinuation<Void, Never>?
    private var didReturnLogin = false
    private var logoutCalls = 0
    private var credentialActive = false

    func restoreSession() async throws -> AuthSession? { nil }

    func login(username: String, password: String) async throws -> AuthSession {
        await withCheckedContinuation { continuation in
            loginContinuation = continuation
        }
        credentialActive = true
        didReturnLogin = true
        return .init(user: .init(
            id: "late-user",
            username: username,
            role: "user"
        ))
    }

    func sendRegistrationCode(
        email: String,
        inviteCode: String
    ) async throws -> RegistrationCodeDelivery {
        .init(expiresInSeconds: 600, resendAfterSeconds: 60)
    }

    func register(
        email: String,
        password: String,
        inviteCode: String,
        verificationCode: String
    ) async throws -> AuthSession {
        .init(user: .init(id: "registered-user", username: email, role: "user"))
    }

    func logout() async {
        logoutCalls += 1
        credentialActive = false
    }

    func resumeLogin() {
        loginContinuation?.resume()
        loginContinuation = nil
    }

    func hasPendingLogin() -> Bool { loginContinuation != nil }
    func loginDidReturn() -> Bool { didReturnLogin }
    func logoutCallCount() -> Int { logoutCalls }
    func hasActiveCredential() -> Bool { credentialActive }
}

private actor AuthenticationRestoreTestService: AuthenticationServicing {
    private var restoreCalls = 0

    func restoreSession() async throws -> AuthSession? {
        restoreCalls += 1
        if restoreCalls == 1 {
            throw AuthenticationRestoreTestError.temporarilyUnavailable
        }
        return .init(user: .init(
            id: "restored-user",
            username: "restored@example.com",
            role: "user"
        ))
    }

    func login(username: String, password: String) async throws -> AuthSession {
        throw AuthenticationRestoreTestError.temporarilyUnavailable
    }

    func sendRegistrationCode(
        email: String,
        inviteCode: String
    ) async throws -> RegistrationCodeDelivery {
        throw AuthenticationRestoreTestError.temporarilyUnavailable
    }

    func register(
        email: String,
        password: String,
        inviteCode: String,
        verificationCode: String
    ) async throws -> AuthSession {
        throw AuthenticationRestoreTestError.temporarilyUnavailable
    }

    func logout() async {}

    func restoreCallCount() -> Int { restoreCalls }
}

private enum AuthenticationRestoreTestError: Error {
    case temporarilyUnavailable
}

private actor AuthenticationRegistrationTestService: AuthenticationServicing {
    private var codeCalls = 0
    private var registerCalls = 0

    func restoreSession() async throws -> AuthSession? { nil }

    func login(username: String, password: String) async throws -> AuthSession {
        session(username: username)
    }

    func sendRegistrationCode(
        email: String,
        inviteCode: String
    ) async throws -> RegistrationCodeDelivery {
        codeCalls += 1
        return .init(expiresInSeconds: 600, resendAfterSeconds: 60)
    }

    func register(
        email: String,
        password: String,
        inviteCode: String,
        verificationCode: String
    ) async throws -> AuthSession {
        registerCalls += 1
        return session(username: email)
    }

    func logout() async {}

    func codeCallCount() -> Int { codeCalls }
    func registerCallCount() -> Int { registerCalls }

    private func session(username: String) -> AuthSession {
        .init(user: .init(
            id: "user-new",
            username: username,
            displayName: nil,
            role: "user"
        ))
    }
}
