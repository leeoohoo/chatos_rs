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

    private func fillValidRegistration(in viewModel: AuthenticationViewModel) {
        viewModel.username = "person@example.com"
        viewModel.inviteCode = "invite-123"
        viewModel.verificationCode = "123456"
        viewModel.password = "secret-value"
        viewModel.confirmPassword = "secret-value"
    }

    private func waitUntil(_ condition: @escaping @MainActor () -> Bool) async {
        for _ in 0..<50 {
            if condition() { return }
            try? await Task.sleep(for: .milliseconds(10))
        }
    }
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
