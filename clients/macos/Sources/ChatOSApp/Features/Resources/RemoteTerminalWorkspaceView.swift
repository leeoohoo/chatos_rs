import ChatOSCore
import SwiftUI

struct RemoteTerminalWorkspaceView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject private var viewModel: NativeRemoteTerminalViewModel

    init(viewModel: NativeRemoteTerminalViewModel) {
        self.viewModel = viewModel
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Label(viewModel.workingDirectory, systemImage: "network")
                    .appFont(.caption.monospaced())
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
                StatusCapsule(
                    title: viewModel.connectionName,
                    color: viewModel.statusColor
                )
                Text(localizedStatus)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 18)
            .frame(height: 40)
            .background(AppPalette.canvas)

            Divider()

            NativeRemoteTerminalSurface(terminal: viewModel)
                .padding(TerminalLayout.contentInsets)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color(nsColor: .textBackgroundColor))

            if let message = viewModel.failureMessage {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(message).lineLimit(2)
                    Spacer()
                    Button(model.localized("重新连接", english: "Reconnect")) {
                        viewModel.reconnect()
                    }
                    .buttonStyle(.borderless)
                }
                .appFont(.caption)
                .foregroundStyle(.red)
                .padding(.horizontal, 14)
                .frame(minHeight: 34)
                .background(Color.red.opacity(0.08))
            }
        }
        .navigationTitle(viewModel.title)
        .toolbar { terminalToolbar }
        .onAppear {
            viewModel.startIfNeeded()
            viewModel.focus()
        }
        .sheet(isPresented: verificationPresented) {
            VStack(alignment: .leading, spacing: 16) {
                Text(model.localized("SSH 二次验证", english: "SSH Verification"))
                    .appFont(.title3.weight(.semibold))
                Text(viewModel.verificationPrompt
                    ?? model.localized("请输入服务器要求的验证码。", english: "Enter the verification code requested by the server."))
                    .appFont(.body)
                SecureField(
                    model.localized("验证码", english: "Verification code"),
                    text: $viewModel.verificationCode
                )
                .textFieldStyle(.roundedBorder)
                HStack {
                    Spacer()
                    Button(model.localized("取消", english: "Cancel")) {
                        viewModel.cancelVerification()
                    }
                    Button(model.localized("验证并连接", english: "Verify and Connect")) {
                        viewModel.submitVerificationCode()
                    }
                    .keyboardShortcut(.defaultAction)
                    .disabled(
                        viewModel.verificationCode
                            .trimmingCharacters(in: .whitespacesAndNewlines)
                            .isEmpty
                    )
                }
            }
            .padding(24)
            .frame(width: 420)
        }
    }

    @ToolbarContentBuilder
    private var terminalToolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            TextField(
                model.localized("搜索终端输出", english: "Search terminal output"),
                text: $viewModel.searchText
            )
            .textFieldStyle(.roundedBorder)
            .frame(width: 190)

            Button(
                model.localized("上一个匹配项", english: "Previous Match"),
                systemImage: "chevron.up",
                action: viewModel.findPrevious
            )
            .labelStyle(.iconOnly)
            .disabled(searchIsEmpty)

            Button(
                model.localized("下一个匹配项", english: "Next Match"),
                systemImage: "chevron.down",
                action: viewModel.findNext
            )
            .labelStyle(.iconOnly)
            .disabled(searchIsEmpty)

            Menu {
                Button(
                    model.localized("清屏", english: "Clear"),
                    systemImage: "eraser",
                    action: viewModel.clear
                )
                Button(
                    model.localized("中断", english: "Interrupt"),
                    systemImage: "stop.fill",
                    action: viewModel.interrupt
                )
                .disabled(!viewModel.isConnected)
                Divider()
                if viewModel.isConnected {
                    Button(
                        model.localized("断开", english: "Disconnect"),
                        systemImage: "stop.circle",
                        action: viewModel.disconnect
                    )
                } else {
                    Button(
                        model.localized("重新连接", english: "Reconnect"),
                        systemImage: "arrow.clockwise",
                        action: viewModel.reconnect
                    )
                }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
        }
    }

    private var searchIsEmpty: Bool {
        viewModel.searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var verificationPresented: Binding<Bool> {
        Binding(
            get: { viewModel.verificationPrompt != nil },
            set: { if !$0 { viewModel.cancelVerification() } }
        )
    }

    private var localizedStatus: String {
        switch viewModel.state {
        case .idle:
            model.localized("准备连接", english: "Ready")
        case .connecting:
            model.localized("连接中", english: "Connecting")
        case .running:
            model.localized("已连接", english: "Connected")
        case .exited:
            model.localized("连接已退出", english: "Connection Exited")
        case .failed:
            model.localized("连接失败", english: "Connection Failed")
        case .closed:
            model.localized("已断开", english: "Disconnected")
        }
    }
}
