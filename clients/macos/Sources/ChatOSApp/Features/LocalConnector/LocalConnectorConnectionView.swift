import ChatOSCore
import SwiftUI

struct LocalConnectorConnectionView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel

    var body: some View {
        SettingsGroupedPage {
            if let status = viewModel.status {
                connectionCard(status)
                boundaryCard(status)
            } else {
                LocalConnectorCard(
                    model.localized("连接状态", english: "Connection Status"),
                    subtitle: model.localized(
                        "Swift Native Connector 正在读取设备身份并连接网关。",
                        english: "Swift Native Connector is loading this device identity and connecting to the gateway."
                    ),
                    systemImage: "externaldrive.badge.timemachine"
                ) {
                    ContentUnavailableView {
                        Label(model.localized("正在启动本机连接", english: "Starting native connection"), systemImage: "externaldrive.badge.timemachine")
                    } description: {
                        Text(model.localized(
                            "Swift Native Connector 正在读取设备身份并连接网关。",
                            english: "Swift Native Connector is loading this device identity and connecting to the gateway."
                        ))
                    } actions: {
                        Button(model.localized("重试", english: "Retry")) { viewModel.activate(pairIfNeeded: true) }
                    }
                    .frame(maxWidth: .infinity, minHeight: 300)
                }
            }
        }
    }

    private func connectionCard(_ status: LocalConnectorStatus) -> some View {
        LocalConnectorCard(
            model.localized("连接状态", english: "Connection Status"),
            subtitle: viewModel.isStarting
                ? model.localized("正在启动 Swift Native Connector", english: "Starting Swift Native Connector")
                : statusText(status),
            systemImage: "externaldrive.connected.to.line.below"
        ) {
            VStack(spacing: 12) {
                HStack(spacing: 10) {
                    Circle()
                        .fill(status.connectorRunning ? .green : .orange)
                        .frame(width: 10, height: 10)
                    Text(status.connectorRunning
                         ? model.localized("网关长连接正常", english: "Gateway connection is healthy")
                         : status.configured
                            ? model.localized("已配对，正在等待网关", english: "Paired, waiting for gateway")
                            : model.localized("尚未配对", english: "Not paired"))
                        .appFont(.headline)
                    Spacer()
                }
                Divider()
                LocalConnectorKeyValueRow(label: model.localized("用户", english: "User"), value: status.user?.username ?? "—")
                LocalConnectorKeyValueRow(label: model.localized("设备", english: "Device"), value: status.deviceName ?? "—")
                LocalConnectorKeyValueRow(label: "Device ID", value: status.deviceID ?? "—", monospaced: true)
                LocalConnectorKeyValueRow(label: model.localized("网关", english: "Gateway"), value: status.cloudBaseURL ?? "—", monospaced: true)
                HStack {
                    if status.configured {
                        Button(model.localized("断开配对", english: "Disconnect"), role: .destructive) { viewModel.disconnect() }
                    } else {
                        Button(model.localized("连接网关", english: "Connect Gateway")) { viewModel.reconnect() }
                            .buttonStyle(.borderedProminent)
                    }
                    Spacer()
                    Button(model.localized("刷新状态", english: "Refresh Status")) { viewModel.refreshStatus() }
                }
                .padding(.top, 4)
            }
        }
    }

    private func boundaryCard(_ status: LocalConnectorStatus) -> some View {
        LocalConnectorCard(
            model.localized("本机边界", english: "Local Boundaries"),
            subtitle: model.localized(
                "文件、终端和权限策略只在当前设备执行",
                english: "Files, terminals, and permission policies execute only on this device"
            ),
            systemImage: "lock.laptopcomputer"
        ) {
            VStack(alignment: .leading, spacing: 13) {
                boundaryRow(
                    model.localized("文件路由", english: "File routing"),
                    value: status.workspaces.isEmpty
                        ? model.localized("尚无工作区", english: "No workspaces")
                        : model.localized("\(status.workspaces.count) 个工作区", english: "\(status.workspaces.count) workspaces"),
                    image: "folder"
                )
                boundaryRow(model.localized("权限控制", english: "Access control"), value: model.localized("由 Swift Native Connector 强制执行", english: "Enforced by Swift Native Connector"), image: "lock.shield")
                boundaryRow(model.localized("运行方式", english: "Runtime"), value: model.localized("原生 Swift 本机进程", english: "Native Swift process"), image: "cpu")
                boundaryRow(model.localized("开发环境", english: "Developer mode"), value: status.developerMode ? model.localized("已开启", english: "Enabled") : model.localized("已关闭", english: "Disabled"), image: "hammer")
                Divider()
                Text(model.localized(
                    "云端请求必须先经过设备签名、工作区边界与命令审批；展示层不会直接访问文件或执行命令。",
                    english: "Cloud requests must pass device signing, workspace boundaries, and command approval. The presentation layer never accesses files or runs commands directly."
                ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func boundaryRow(_ label: String, value: String, image: String) -> some View {
        HStack(spacing: 10) {
            Image(systemName: image)
                .foregroundStyle(.green)
                .frame(width: 18)
            Text(label)
            Spacer()
            Text(value)
                .foregroundStyle(.secondary)
        }
        .appFont(.callout)
    }

    private func statusText(_ status: LocalConnectorStatus) -> String {
        if status.connectorRunning {
            return model.localized("已通过网关连接到 ChatOS", english: "Connected to ChatOS through the gateway")
        }
        return status.configured
            ? model.localized("本机配置有效，长连接暂未建立", english: "Local configuration is valid; the persistent connection is not established")
            : model.localized("需要使用当前 ChatOS 登录态完成配对", english: "Pairing requires the current ChatOS session")
    }
}
