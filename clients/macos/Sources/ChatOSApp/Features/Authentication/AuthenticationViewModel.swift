import ChatOSCore
import Foundation

@MainActor
final class AuthenticationViewModel: ObservableObject {
    enum Phase: Equatable {
        case restoring
        case signedOut
        case authenticating
        case authenticated(AuthSession)
    }

    @Published private(set) var phase: Phase = .restoring
    @Published private(set) var errorMessage: String?
    @Published var username = ""
    @Published var password = ""

    private let service: any AuthenticationServicing
    private var didStart = false

    init(service: any AuthenticationServicing) {
        self.service = service
    }

    var canLogin: Bool {
        !username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !password.isEmpty
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

    func logout() {
        password = ""
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
}
