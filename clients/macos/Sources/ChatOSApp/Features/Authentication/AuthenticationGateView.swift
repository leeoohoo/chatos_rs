import SwiftUI

struct AuthenticationGateView<Content: View>: View {
    @ObservedObject var authentication: AuthenticationViewModel
    @ViewBuilder var content: () -> Content

    var body: some View {
        switch authentication.phase {
        case .restoring:
            VStack(spacing: 14) {
                Image(systemName: "sparkles")
                    .appFont(.system(size: 36))
                    .foregroundStyle(AppPalette.ai)
                ProgressView("正在恢复登录状态…")
                    .controlSize(.small)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .signedOut, .authenticating:
            LoginView(authentication: authentication)
        case .authenticated:
            content()
        }
    }
}
