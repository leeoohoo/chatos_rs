import SwiftUI

struct LocalConnectorSandboxView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel

    var body: some View {
        SettingsGroupedPage {
            policyCard
            backendCard
        }
    }

    private var policyCard: some View {
        LocalConnectorCard(
            model.localized("默认任务权限", english: "Default Task Permissions"),
            subtitle: model.localized(
                "新任务在没有项目级覆盖时使用这套策略",
                english: "New tasks use this policy unless the project overrides it"
            ),
            systemImage: "lock.shield"
        ) {
            if let settings = viewModel.sandboxSettings {
                Toggle(
                    model.localized("启用本机权限控制", english: "Enable local access control"),
                    isOn: Binding(
                        get: { settings.enabled },
                        set: { viewModel.updateSandbox(enabled: $0) }
                    )
                )
                .toggleStyle(.switch)

                if let error = settings.permissionConfigurationError {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .appFont(.caption)
                        .foregroundStyle(.red)
                }

                Divider()
                picker(
                    model.localized("文件权限", english: "File access"),
                    selection: settings.defaultPermissionProfileID,
                    values: [
                        ("read_only", model.localized("只读", english: "Read only")),
                        ("workspace_write", model.localized("工作区可写", english: "Workspace write")),
                        ("full_access", model.localized("完全访问", english: "Full access")),
                    ]
                ) { viewModel.updateSandbox(permissionProfileID: $0) }
                picker(
                    model.localized("审批策略", english: "Approval policy"),
                    selection: settings.defaultApprovalPolicy,
                    values: [
                        ("on_request", model.localized("按需审批", english: "On request")),
                        ("never", model.localized("不请求审批", english: "Never ask")),
                    ]
                ) { viewModel.updateSandbox(approvalPolicy: $0) }
                picker(
                    model.localized("审批人", english: "Reviewer"),
                    selection: settings.defaultApprovalReviewer,
                    values: [
                        ("user", model.localized("用户", english: "User")),
                        ("auto_review", model.localized("审批模型", english: "Approval model")),
                    ]
                ) { viewModel.updateSandbox(approvalReviewer: $0) }
                picker(
                    model.localized("网络访问", english: "Network access"),
                    selection: settings.defaultNetworkAccess,
                    values: [
                        ("disabled", model.localized("禁用", english: "Disabled")),
                        ("controlled", model.localized("受控访问", english: "Controlled")),
                        ("host", model.localized("跟随本机", english: "Host access")),
                    ]
                ) { viewModel.updateSandbox(networkAccess: $0) }
                Divider()
                LocalConnectorKeyValueRow(label: model.localized("当前 Profile", english: "Current profile"), value: settings.defaultPermissionProfileName)
                LocalConnectorKeyValueRow(label: model.localized("策略版本", english: "Policy revision"), value: settings.policyRevision ?? "—", monospaced: true)
            } else {
                ProgressView(model.localized("正在读取权限策略…", english: "Loading access policy…"))
                    .frame(maxWidth: .infinity, minHeight: 100)
            }
        }
    }

    private var backendCard: some View {
        LocalConnectorCard(
            model.localized("运行后端", english: "Execution Backend"),
            subtitle: model.localized(
                "当前版本只允许使用本机进程后端",
                english: "This version supports only the local process backend"
            ),
            systemImage: "cpu"
        ) {
            if viewModel.sandboxBackends.isEmpty {
                Text(model.localized("没有可用的权限执行后端。", english: "No permission backend is available."))
                    .foregroundStyle(.secondary)
            } else {
                VStack(spacing: 0) {
                    ForEach(viewModel.sandboxBackends) { backend in
                        HStack(alignment: .top, spacing: 12) {
                            Image(systemName: backend.status == "ready" ? "checkmark.seal.fill" : "exclamationmark.triangle.fill")
                                .foregroundStyle(backend.status == "ready" ? .green : .orange)
                            VStack(alignment: .leading, spacing: 5) {
                                HStack {
                                    Text(backend.backend == "local_process"
                                         ? model.localized("本机进程", english: "Local process")
                                         : backend.backend)
                                        .appFont(.headline)
                                    Text(backend.status)
                                        .appFont(.caption2.monospaced())
                                        .foregroundStyle(.secondary)
                                }
                                Text(backend.message)
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                                HStack(spacing: 8) {
                                    capability(model.localized("文件隔离", english: "File isolation"), enabled: backend.filesystemIsolation)
                                    capability(model.localized("网络隔离", english: "Network isolation"), enabled: backend.networkIsolation)
                                    capability(model.localized("进程树控制", english: "Process tree control"), enabled: backend.processTreeControl)
                                }
                            }
                            Spacer()
                        }
                        .padding(.vertical, 10)
                    }
                }
            }
        }
    }

    private func picker(
        _ label: String,
        selection: String,
        values: [(String, String)],
        onChange: @escaping (String) -> Void
    ) -> some View {
        HStack {
            Text(label)
            Spacer()
            Picker(label, selection: Binding(
                get: { selection },
                set: { value in onChange(value) }
            )) {
                ForEach(values, id: \.0) { value, title in
                    Text(title).tag(value)
                }
            }
            .labelsHidden()
            .frame(width: 180)
        }
    }

    private func capability(_ label: String, enabled: Bool) -> some View {
        Label(label, systemImage: enabled ? "checkmark" : "minus")
            .appFont(.caption2)
            .foregroundStyle(enabled ? .green : .secondary)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(.quaternary, in: Capsule())
    }
}
