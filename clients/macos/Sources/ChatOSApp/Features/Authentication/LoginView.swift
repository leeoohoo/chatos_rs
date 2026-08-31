import SwiftUI

struct LoginView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var authentication: AuthenticationViewModel
    @FocusState private var focusedField: Field?

    private enum Field {
        case username
        case password
    }

    var body: some View {
        HStack(spacing: 0) {
            introduction
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color.accentColor.opacity(0.055))

            loginForm
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color(nsColor: .windowBackgroundColor))
        }
        .onAppear { focusedField = .username }
    }

    private var introduction: some View {
        VStack(alignment: .leading, spacing: 30) {
            Image(systemName: "sparkles")
                .appFont(.system(size: 34, weight: .semibold))
                .foregroundStyle(AppPalette.ai)
                .frame(width: 74, height: 74)
                .background(AppPalette.ai.opacity(0.1), in: Circle())

            VStack(alignment: .leading, spacing: 12) {
                Text(model.localized(
                    "把长期项目，变成\n可以持续推进的协作。",
                    english: "Turn long-term projects into\ncollaboration that keeps moving."
                ))
                    .appFont(.system(size: 32, weight: .bold, design: .rounded))
                    .lineSpacing(5)
                Text(model.localized(
                    "工作区、聊天、任务过程和本机能力全部原生呈现。",
                    english: "Workspaces, conversations, task progress, and local capabilities—all native."
                ))
                    .appFont(.body)
                    .foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 22) {
                feature(model.localized("联系人和项目保持上下文", english: "Keep context across contacts and projects"), "bubble.left.and.bubble.right")
                feature(model.localized("任务流程和阻塞状态可追踪", english: "Track task progress and blockers"), "point.3.connected.trianglepath.dotted")
                feature(model.localized("敏感能力留在本机安全边界", english: "Keep sensitive capabilities within local boundaries"), "lock.shield")
            }
        }
        .frame(maxWidth: 510, alignment: .leading)
        .padding(70)
    }

    private var loginForm: some View {
        VStack(alignment: .leading, spacing: 26) {
            VStack(alignment: .leading, spacing: 7) {
                Text(model.localized("登录", english: "Sign In"))
                    .appFont(.system(size: 29, weight: .bold))
                Text(model.localized("使用 ChatOS 平台账号继续", english: "Continue with your ChatOS account"))
                    .foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 18) {
                fieldLabel(model.localized("账号", english: "Account"))
                TextField("name@example.com", text: $authentication.username)
                    .textFieldStyle(.roundedBorder)
                    .focused($focusedField, equals: .username)
                    .onSubmit { focusedField = .password }

                fieldLabel(model.localized("密码", english: "Password"))
                SecureField(model.localized("密码", english: "Password"), text: $authentication.password)
                    .textFieldStyle(.roundedBorder)
                    .focused($focusedField, equals: .password)
                    .onSubmit(authentication.login)
            }

            if let errorMessage = authentication.errorMessage {
                Label(errorMessage, systemImage: "exclamationmark.circle.fill")
                    .appFont(.callout)
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Button(action: authentication.login) {
                HStack {
                    Spacer()
                    if authentication.phase == .authenticating {
                        ProgressView().controlSize(.small)
                    } else {
                        Text(model.localized("登录", english: "Sign In")).fontWeight(.semibold)
                    }
                    Spacer()
                }
                .frame(height: 28)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .disabled(!authentication.canLogin)

            Label(
                "\(model.localized("ChatOS 服务", english: "ChatOS Service")) · \(RuntimeConfiguration.apiBaseURL.host ?? RuntimeConfiguration.apiBaseURL.absoluteString)",
                systemImage: "network"
            )
                .appFont(.caption)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
                .background(.quaternary, in: Capsule())
        }
        .frame(width: 410)
        .padding(70)
    }

    private func feature(_ title: String, _ systemImage: String) -> some View {
        Label(title, systemImage: systemImage)
            .appFont(.headline)
            .symbolRenderingMode(.hierarchical)
            .foregroundStyle(.primary)
    }

    private func fieldLabel(_ title: String) -> some View {
        Text(title)
            .appFont(.caption.weight(.semibold))
            .padding(.bottom, -11)
    }
}
