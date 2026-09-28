import ChatOSCore
import Foundation

@MainActor
final class AuthenticationViewModel: ObservableObject {
    enum Mode: Equatable {
        case signIn
        case register
    }

    enum Phase: Equatable {
        case restoring
        case signedOut
        case authenticating
        case authenticated(AuthSession)
    }

    @Published private(set) var phase: Phase = .restoring
    @Published private(set) var errorMessage: String?
    @Published private(set) var registrationMessage: String?
    @Published private(set) var isSendingRegistrationCode = false
    @Published private(set) var registrationCodeCountdown = 0
    @Published var mode: Mode = .signIn
    @Published var username = ""
    @Published var password = ""
    @Published var inviteCode = ""
    @Published var verificationCode = ""
    @Published var confirmPassword = ""

    private let service: any AuthenticationServicing
    private var didStart = false
    private var registrationCountdownTask: Task<Void, Never>?

    init(service: any AuthenticationServicing) {
        self.service = service
    }

    var canLogin: Bool {
        !username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !password.isEmpty
            && phase != .authenticating
    }

    var canSendRegistrationCode: Bool {
        !username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !inviteCode.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !isSendingRegistrationCode
            && registrationCodeCountdown == 0
            && phase != .authenticating
    }

    var canRegister: Bool {
        !username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !password.isEmpty
            && !confirmPassword.isEmpty
            && !inviteCode.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !verificationCode.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && phase != .authenticating
    }

    func start() {
        guard !didStart else { return }
        didStart = true
        phase = .restoring

        Task {
            do {
                if let session = try await service.restoreSession() {
                    phase = .authenticated(session)
                } else {
                    phase = .signedOut
                }
            } catch {
                errorMessage = error.localizedDescription
                phase = .signedOut
            }
        }
    }

    func login() {
        guard canLogin else { return }
        phase = .authenticating
        errorMessage = nil
        let submittedUsername = username
        let submittedPassword = password

        Task {
            do {
                let session = try await service.login(
                    username: submittedUsername,
                    password: submittedPassword
                )
                password = ""
                phase = .authenticated(session)
            } catch {
                errorMessage = error.localizedDescription
                phase = .signedOut
            }
        }
    }

    func showRegistration() {
        mode = .register
        password = ""
        confirmPassword = ""
        errorMessage = nil
        registrationMessage = nil
    }

    func showLogin() {
        mode = .signIn
        password = ""
        confirmPassword = ""
        errorMessage = nil
        registrationMessage = nil
    }

    func sendRegistrationCode() {
        guard canSendRegistrationCode else {
            errorMessage = registrationValidationMessage(forCodeOnly: true)
            return
        }
        guard isLikelyEmail(username) else {
            errorMessage = registrationValidationMessage(forCodeOnly: true)
            return
        }
        isSendingRegistrationCode = true
        errorMessage = nil
        registrationMessage = nil
        let email = username
        let submittedInviteCode = inviteCode

        Task {
            do {
                let delivery = try await service.sendRegistrationCode(
                    email: email,
                    inviteCode: submittedInviteCode
                )
                registrationMessage = "验证码已发送，请查看邮箱。"
                startRegistrationCountdown(delivery.resendAfterSeconds)
            } catch {
                errorMessage = friendlyRegistrationError(error)
            }
            isSendingRegistrationCode = false
        }
    }

    func register() {
        guard canRegister else {
            errorMessage = registrationValidationMessage(forCodeOnly: false)
            return
        }
        let normalizedVerificationCode = verificationCode
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard isLikelyEmail(username),
              normalizedVerificationCode.count == 6,
              normalizedVerificationCode.allSatisfy(\.isNumber),
              password.count >= 6,
              password == confirmPassword else {
            errorMessage = registrationValidationMessage(forCodeOnly: false)
            return
        }
        phase = .authenticating
        errorMessage = nil
        registrationMessage = nil
        let submittedEmail = username
        let submittedPassword = password
        let submittedInviteCode = inviteCode
        let submittedVerificationCode = verificationCode

        Task {
            do {
                let session = try await service.register(
                    email: submittedEmail,
                    password: submittedPassword,
                    inviteCode: submittedInviteCode,
                    verificationCode: submittedVerificationCode
                )
                registrationCountdownTask?.cancel()
                password = ""
                confirmPassword = ""
                verificationCode = ""
                phase = .authenticated(session)
            } catch {
                errorMessage = friendlyRegistrationError(error)
                phase = .signedOut
            }
        }
    }

    func logout() {
        mode = .signIn
        password = ""
        confirmPassword = ""
        verificationCode = ""
        errorMessage = nil
        phase = .signedOut
        Task { await service.logout() }
    }

    func expireSession() {
        guard case .authenticated = phase else { return }
        password = ""
        errorMessage = "登录状态已失效，请重新登录。"
        phase = .signedOut
        Task { await service.logout() }
    }

    private func startRegistrationCountdown(_ seconds: Int) {
        registrationCountdownTask?.cancel()
        registrationCodeCountdown = max(1, seconds)
        registrationCountdownTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard !Task.isCancelled, let self else { return }
                if registrationCodeCountdown <= 1 {
                    registrationCodeCountdown = 0
                    return
                }
                registrationCodeCountdown -= 1
            }
        }
    }

    private func registrationValidationMessage(forCodeOnly: Bool) -> String {
        if !isLikelyEmail(username) { return "请输入有效的邮箱地址。" }
        if inviteCode.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return "请输入邀请码。"
        }
        if forCodeOnly { return "请稍后再试。" }
        let normalizedVerificationCode = verificationCode
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if normalizedVerificationCode.count != 6
            || !normalizedVerificationCode.allSatisfy(\.isNumber) {
            return "请输入 6 位邮箱验证码。"
        }
        if password.count < 6 { return "密码至少需要 6 个字符。" }
        if password != confirmPassword { return "两次输入的密码不一致。" }
        return "请检查注册信息。"
    }

    private func isLikelyEmail(_ value: String) -> Bool {
        let parts = value.trimmingCharacters(in: .whitespacesAndNewlines).split(separator: "@")
        return parts.count == 2 && parts[1].contains(".")
    }

    private func friendlyRegistrationError(_ error: Error) -> String {
        let message = error.localizedDescription
        let normalized = message.lowercased()
        let translations: [(String, String)] = [
            ("email already registered", "这个邮箱已经注册，可以直接登录。"),
            ("invite code is invalid", "邀请码无效或已经失效。"),
            ("verification code is invalid or expired", "邮箱验证码错误或已经过期。"),
            ("verification code was sent recently", "验证码刚刚发送，请稍后再试。"),
            ("too many verification emails", "验证码发送次数过多，请稍后再试。"),
            ("email format is invalid", "请输入有效的邮箱地址。"),
            ("temporarily unavailable", "注册服务暂时不可用，请稍后再试。"),
        ]
        return translations.first(where: { normalized.contains($0.0) })?.1 ?? message
    }
}
