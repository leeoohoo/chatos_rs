import SwiftUI

struct LoginView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var authentication: AuthenticationViewModel
    @FocusState private var focusedField: Field?

    private enum Field {
        case username
        case inviteCode
        case verificationCode
        case password
        case confirmPassword
    }

    var body: some View {
        HStack(spacing: 0) {
            introduction
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color.accentColor.opacity(0.055))

            authenticationForm
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color(nsColor: .windowBackgroundColor))
        }
        .onAppear { focusedField = .username }
        .onChange(of: authentication.mode) { _, _ in
            focusedField = .username
        }
    }

    private var introduction: some View {
        VStack(alignment: .leading, spacing: 22) {
            PetSpriteAnimationView(
                animationState: .idle,
                isDragging: false,
                dragDirection: .right,
                isAnimationActive: false
            )
            .frame(width: 132, height: 143)
            .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 3) {
                Text(model.localized("路还很长，", english: "The road ahead is long,"))
                    .appFont(.system(size: 37, weight: .bold, design: .rounded))
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                Text(model.localized(
                    "我记得我们走到了哪里。",
                    english: "and I remember how far we’ve come."
                ))
                    .appFont(.system(size: 37, weight: .bold, design: .rounded))
                    .foregroundStyle(Color.orange)
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
            }
            .lineSpacing(5)

            Text(model.localized(
                "以岁月打磨，致敬每一份认真。",
                english: "Refined over time, in honor of every earnest effort."
            ))
                .appFont(.system(size: 17, weight: .semibold))
                .tracking(2.2)
                .foregroundStyle(.secondary)
                .padding(.top, 4)
        }
        .frame(maxWidth: 510, alignment: .leading)
        .padding(70)
        .offset(y: -44)
    }

    @ViewBuilder
    private var authenticationForm: some View {
        switch authentication.mode {
        case .signIn:
            loginForm
        case .register:
            registrationForm
        }
    }

    private var loginForm: some View {
        VStack(alignment: .leading, spacing: 26) {
            formHeader(
                model.localized("登录", english: "Sign In"),
                subtitle: model.localized(
                    "使用 ChatOS 平台账号继续",
                    english: "Continue with your ChatOS account"
                )
            )

            VStack(alignment: .leading, spacing: 18) {
                fieldLabel(model.localized("账号", english: "Account"))
                TextField("name@example.com", text: $authentication.username)
                    .textFieldStyle(.roundedBorder)
                    .focused($focusedField, equals: .username)
                    .onSubmit { focusedField = .password }

                fieldLabel(model.localized("密码", english: "Password"))
                SecureField(
                    model.localized("密码", english: "Password"),
                    text: $authentication.password
                )
                    .textFieldStyle(.roundedBorder)
                    .focused($focusedField, equals: .password)
                    .onSubmit(authentication.login)
            }

            statusMessages

            primaryButton(
                title: model.localized("登录", english: "Sign In"),
                action: authentication.login,
                disabled: !authentication.canLogin
            )

            modeSwitch(
                prompt: model.localized("还没有账号？", english: "New to ChatOS?"),
                title: model.localized("创建账号", english: "Create Account"),
                action: authentication.showRegistration
            )
        }
        .frame(width: 410)
        .padding(70)
    }

    private var registrationForm: some View {
        VStack(alignment: .leading, spacing: 18) {
            formHeader(
                model.localized("创建账号", english: "Create Account"),
                subtitle: model.localized(
                    "邀请测试 · 邮箱验证",
                    english: "Invitation beta · Email verification"
                )
            )

            VStack(alignment: .leading, spacing: 15) {
                fieldLabel(model.localized("邮箱", english: "Email"))
                TextField("name@example.com", text: $authentication.username)
                    .textFieldStyle(.roundedBorder)
                    .focused($focusedField, equals: .username)
                    .onSubmit { focusedField = .inviteCode }

                fieldLabel(model.localized("邀请码", english: "Invitation Code"))
                TextField(
                    model.localized("输入邀请测试码", english: "Enter invitation code"),
                    text: $authentication.inviteCode
                )
                    .textFieldStyle(.roundedBorder)
                    .focused($focusedField, equals: .inviteCode)
                    .onSubmit(authentication.sendRegistrationCode)

                fieldLabel(model.localized("邮箱验证码", english: "Email Verification Code"))
                HStack(spacing: 10) {
                    TextField(
                        model.localized("6 位验证码", english: "6-digit code"),
                        text: $authentication.verificationCode
                    )
                        .textFieldStyle(.roundedBorder)
                        .focused($focusedField, equals: .verificationCode)
                        .onSubmit { focusedField = .password }

                    Button(codeButtonTitle, action: authentication.sendRegistrationCode)
                        .buttonStyle(.bordered)
                        .disabled(!authentication.canSendRegistrationCode)
                        .frame(minWidth: 94)
                }

                fieldLabel(model.localized("密码", english: "Password"))
                SecureField(
                    model.localized("至少 6 个字符", english: "At least 6 characters"),
                    text: $authentication.password
                )
                    .textFieldStyle(.roundedBorder)
                    .focused($focusedField, equals: .password)
                    .onSubmit { focusedField = .confirmPassword }

                fieldLabel(model.localized("确认密码", english: "Confirm Password"))
                SecureField(
                    model.localized("再次输入密码", english: "Enter password again"),
                    text: $authentication.confirmPassword
                )
                    .textFieldStyle(.roundedBorder)
                    .focused($focusedField, equals: .confirmPassword)
                    .onSubmit(authentication.register)
            }

            statusMessages

            primaryButton(
                title: model.localized("创建账号", english: "Create Account"),
                action: authentication.register,
                disabled: !authentication.canRegister
            )

            modeSwitch(
                prompt: model.localized("已有账号？", english: "Already have an account?"),
                title: model.localized("返回登录", english: "Back to Sign In"),
                action: authentication.showLogin
            )
        }
        .frame(width: 410)
        .padding(54)
    }

    @ViewBuilder
    private var statusMessages: some View {
        if let errorMessage = authentication.errorMessage {
            Label(errorMessage, systemImage: "exclamationmark.circle.fill")
                .appFont(.callout)
                .foregroundStyle(.red)
                .fixedSize(horizontal: false, vertical: true)
        }
        if let message = authentication.registrationMessage {
            Label(message, systemImage: "checkmark.circle.fill")
                .appFont(.callout)
                .foregroundStyle(.green)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var codeButtonTitle: String {
        if authentication.isSendingRegistrationCode {
            return model.localized("发送中", english: "Sending")
        }
        if authentication.registrationCodeCountdown > 0 {
            return "\(authentication.registrationCodeCountdown)s"
        }
        return model.localized("发送验证码", english: "Send Code")
    }

    private func formHeader(_ title: String, subtitle: String) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(title)
                .appFont(.system(size: 29, weight: .bold))
            Text(subtitle)
                .foregroundStyle(.secondary)
        }
    }

    private func primaryButton(
        title: String,
        action: @escaping () -> Void,
        disabled: Bool
    ) -> some View {
        Button(action: action) {
            HStack {
                Spacer()
                if authentication.phase == .authenticating {
                    ProgressView().controlSize(.small)
                } else {
                    Text(title).fontWeight(.semibold)
                }
                Spacer()
            }
            .frame(height: 28)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .disabled(disabled)
    }

    private func modeSwitch(
        prompt: String,
        title: String,
        action: @escaping () -> Void
    ) -> some View {
        HStack(spacing: 5) {
            Text(prompt).foregroundStyle(.secondary)
            Button(title, action: action)
                .buttonStyle(.plain)
                .foregroundStyle(Color.accentColor)
                .fontWeight(.semibold)
        }
        .appFont(.callout)
        .frame(maxWidth: .infinity)
    }

    private func fieldLabel(_ title: String) -> some View {
        Text(title)
            .appFont(.caption.weight(.semibold))
            .padding(.bottom, -9)
    }
}
