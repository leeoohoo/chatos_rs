import AppKit
import ChatOSCore
import SwiftUI

struct RemoteConnectionBasicsSection: View {
    @ObservedObject var viewModel: RemoteConnectionEditorViewModel

    var body: some View {
        Section("连接") {
            TextField("名称（可选）", text: $viewModel.name)
            HStack {
                TextField("主机，例如 server.example.com", text: $viewModel.host)
                TextField("端口", text: $viewModel.port)
                    .frame(width: 90)
            }
            TextField("用户名", text: $viewModel.username)
            Picker("主机密钥", selection: $viewModel.hostKeyPolicy) {
                Text("严格校验 known_hosts").tag(RemoteHostKeyPolicy.strict)
                Text("首次连接时接受新密钥").tag(RemoteHostKeyPolicy.acceptNew)
            }
        }
    }
}

struct RemoteConnectionAuthenticationSection: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: RemoteConnectionEditorViewModel

    var body: some View {
        Section("认证") {
            Picker("认证方式", selection: $viewModel.authenticationType) {
                Text("私钥").tag(RemoteAuthenticationType.privateKey)
                Text("私钥 + 证书").tag(RemoteAuthenticationType.privateKeyCertificate)
                Text("密码").tag(RemoteAuthenticationType.password)
            }

            if viewModel.authenticationType == .password {
                SecureField(
                    viewModel.editingConnection?.hasPassword == true
                        ? model.localized("留空则继续使用已保存密码", english: "Leave blank to keep the saved password")
                        : model.localized("登录密码", english: "Login Password"),
                    text: $viewModel.password
                )
            } else {
                filePathRow(
                    title: model.localized("私钥路径", english: "Private Key Path"),
                    placeholder: viewModel.editingConnection?.hasPrivateKeyPath == true
                        ? model.localized("留空则继续使用已保存私钥", english: "Leave blank to keep the saved private key")
                        : "/Users/you/.ssh/id_ed25519",
                    text: $viewModel.privateKeyPath
                )
                if viewModel.authenticationType == .privateKeyCertificate {
                    filePathRow(
                        title: model.localized("证书路径", english: "Certificate Path"),
                        placeholder: viewModel.editingConnection?.hasCertificatePath == true
                            ? model.localized("留空则继续使用已保存证书", english: "Leave blank to keep the saved certificate")
                            : "/Users/you/.ssh/id_ed25519-cert.pub",
                        text: $viewModel.certificatePath
                    )
                }
            }
            TextField("默认远端目录，例如 /srv/app", text: $viewModel.defaultRemotePath)
        }
    }

    private func filePathRow(
        title: String,
        placeholder: String,
        text: Binding<String>
    ) -> some View {
        HStack {
            TextField(placeholder, text: text)
            Button("选择…") {
                if let path = selectFile(title: title) { text.wrappedValue = path }
            }
        }
    }
}

struct RemoteConnectionJumpSection: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: RemoteConnectionEditorViewModel

    var body: some View {
        Section {
            Toggle("通过跳板机连接", isOn: $viewModel.jumpEnabled)
            if viewModel.jumpEnabled {
                Picker("跳板机来源", selection: $viewModel.jumpMode) {
                    ForEach(RemoteJumpMode.allCases) { mode in
                        Text(mode.title(language: model.interfaceLanguage)).tag(mode)
                    }
                }
                .pickerStyle(.segmented)

                if viewModel.jumpMode == .existing {
                    Picker("已有连接", selection: $viewModel.jumpConnectionID) {
                        Text("请选择").tag("")
                        ForEach(viewModel.availableJumpConnections) { connection in
                            Text("\(connection.name) — \(connection.username)@\(connection.host):\(connection.port)")
                                .tag(connection.id)
                        }
                    }
                } else {
                    HStack {
                        TextField("跳板机地址", text: $viewModel.jumpHost)
                        TextField("端口", text: $viewModel.jumpPort).frame(width: 90)
                    }
                    TextField("跳板机用户名", text: $viewModel.jumpUsername)
                    jumpFileRow(
                        model.localized("跳板机私钥", english: "Jump Host Private Key"),
                        text: $viewModel.jumpPrivateKeyPath
                    )
                    jumpFileRow(
                        model.localized("跳板机证书（可选）", english: "Jump Host Certificate (Optional)"),
                        text: $viewModel.jumpCertificatePath
                    )
                    SecureField("跳板机密码（可选）", text: $viewModel.jumpPassword)
                }
            }
        } header: {
            Text("跳板机")
        } footer: {
            if viewModel.jumpEnabled {
                Text("可以复用已有远端连接，或单独保存这条连接的跳板机凭据。")
            }
        }
    }

    private func jumpFileRow(_ title: String, text: Binding<String>) -> some View {
        HStack {
            TextField(title, text: text)
            Button("选择…") {
                if let path = selectFile(title: title) { text.wrappedValue = path }
            }
        }
    }
}

@MainActor
private func selectFile(title: String) -> String? {
    let panel = NSOpenPanel()
    panel.title = title
    panel.canChooseFiles = true
    panel.canChooseDirectories = false
    panel.allowsMultipleSelection = false
    panel.showsHiddenFiles = true
    return panel.runModal() == .OK ? panel.url?.path : nil
}
