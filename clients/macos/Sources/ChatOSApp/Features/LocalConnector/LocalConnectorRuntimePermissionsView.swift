import ChatOSCore
import SwiftUI

struct LocalConnectorRuntimePermissionsView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel

    var body: some View {
        SettingsGroupedPage {
            runtimeCard
            permissionsCard
        }
    }

    private var runtimeCard: some View {
        LocalConnectorCard(
            model.localized("运行配置", english: "Runtime Configuration"),
            subtitle: model.localized(
                "开发模式只改变服务端点，不改变本机权限边界",
                english: "Developer mode changes service endpoints, not local permission boundaries"
            ),
            systemImage: "slider.horizontal.3"
        ) {
            if let settings = viewModel.runtimeSettings {
                Toggle(
                    model.localized("开发者模式", english: "Developer mode"),
                    isOn: Binding(
                        get: { settings.developerMode },
                        set: { enabled in
                            viewModel.updateDeveloperMode(enabled)
                        }
                    )
                )
                .toggleStyle(.switch)
                Divider()
                LocalConnectorKeyValueRow(label: "Connector Gateway", value: settings.developerCloudBaseURL, monospaced: true)
                LocalConnectorKeyValueRow(label: "Account Service", value: settings.developerUserServiceBaseURL, monospaced: true)
                Text(model.localized(
                    "Swift Native Connector 直接连接这些服务端点，不启动本机 HTTP Core。",
                    english: "Swift Native Connector connects directly to these service endpoints without starting a local HTTP core."
                ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ProgressView(model.localized("正在读取运行配置…", english: "Loading runtime configuration…"))
                    .frame(maxWidth: .infinity, minHeight: 80)
            }
        }
    }

    private var permissionsCard: some View {
        LocalConnectorCard(
            model.localized("系统权限", english: "System Permissions"),
            subtitle: viewModel.systemPermissions.map {
                model.localized(
                    "\($0.platformLabel) 下 Skills 与 MCP 的系统访问状态",
                    english: "System access for skills and MCP servers on \($0.platformLabel)"
                )
            },
            systemImage: "hand.raised"
        ) {
            HStack {
                Text(model.localized(
                    "每项权限都会说明准确的授权目标和当前状态。",
                    english: "Each permission shows its exact authorization target and current status."
                ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    viewModel.refreshPermissions()
                } label: {
                    Label(model.localized("重新检测", english: "Check Again"), systemImage: "arrow.clockwise")
                }
                .buttonStyle(.borderless)
                .controlSize(.small)
                .disabled(viewModel.isPerformingAction)
            }
            Divider()
            if let permissions = viewModel.systemPermissions {
                VStack(spacing: 0) {
                    ForEach(permissions.items) { permission in
                        permissionRow(permission)
                        if permission.id != permissions.items.last?.id { Divider() }
                    }
                }
            } else {
                ProgressView(model.localized("正在检查系统权限…", english: "Checking system permissions…"))
                    .frame(maxWidth: .infinity, minHeight: 100)
            }
        }
    }

    private func permissionRow(_ permission: LocalConnectorSystemPermission) -> some View {
        HStack(alignment: .top, spacing: 13) {
            Image(systemName: permissionIcon(permission.id))
                .appFont(.system(size: 17))
                .foregroundStyle(permissionColor(permission.status))
                .frame(width: 28, height: 28)
                .background(permissionColor(permission.status).opacity(0.1), in: RoundedRectangle(cornerRadius: 8))
            VStack(alignment: .leading, spacing: 5) {
                HStack {
                    Text(permissionLabel(permission))
                        .appFont(.headline)
                    Text(permissionStatusLabel(permission))
                        .appFont(.caption2.weight(.semibold))
                        .foregroundStyle(permissionColor(permission.status))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(permissionColor(permission.status).opacity(0.1), in: Capsule())
                }
                Text(permissionSummary(permission))
                    .appFont(.callout)
                if !permission.note.isEmpty {
                    Text(permissionNote(permission))
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                if let error = permission.lastError {
                    Text(error)
                        .appFont(.caption)
                        .foregroundStyle(.red)
                }
            }
            Spacer(minLength: 12)
            if permission.canRequest && !isReady(permission.status) {
                Button(permissionRequestLabel(permission)) {
                    viewModel.requestPermission(id: permission.id)
                }
                .disabled(viewModel.isPerformingAction)
            }
        }
        .padding(.vertical, 11)
    }

    private func isReady(_ status: String) -> Bool {
        status == "ready" || status == "not_applicable" || status == "on_demand"
    }

    private func permissionColor(_ status: String) -> Color {
        switch status {
        case "ready", "not_applicable", "on_demand": .green
        case "needs_attention", "missing_dependency", "action_required": .orange
        default: .secondary
        }
    }

    private func permissionIcon(_ id: String) -> String {
        if id.contains("accessibility") { return "accessibility" }
        if id.contains("screen") { return "rectangle.inset.filled.and.person.filled" }
        if id.contains("automation") { return "app.badge.checkmark" }
        if id.contains("network") { return "network" }
        if id.contains("folder") || id.contains("file") { return "folder" }
        return "lock.shield"
    }

    private func permissionLabel(_ permission: LocalConnectorSystemPermission) -> String {
        switch permission.id {
        case "terminal": model.localized("本机终端执行", english: "Local Terminal Execution")
        case "accessibility": model.localized("辅助功能", english: "Accessibility")
        case "screen_recording": model.localized("屏幕与系统音频录制", english: "Screen & System Audio Recording")
        case "full_disk_access": model.localized("完全磁盘访问", english: "Full Disk Access")
        default: permission.label
        }
    }

    private func permissionSummary(_ permission: LocalConnectorSystemPermission) -> String {
        switch permission.id {
        case "terminal": model.localized(
            "由 Swift 客户端直接启动受控的本机进程",
            english: "The Swift client directly launches controlled local processes"
        )
        case "accessibility": model.localized(
            "允许 Computer Use 控制本机应用",
            english: "Allows Computer Use to control apps on this Mac"
        )
        case "screen_recording": model.localized(
            "允许 Computer Use 获取屏幕画面",
            english: "Allows Computer Use to capture the screen"
        )
        case "full_disk_access": model.localized(
            "访问受 macOS 隐私保护的目录",
            english: "Access folders protected by macOS privacy controls"
        )
        default: permission.summary
        }
    }

    private func permissionNote(_ permission: LocalConnectorSystemPermission) -> String {
        switch permission.id {
        case "terminal": model.localized(
            "命令仍受工作区边界与审批策略约束。",
            english: "Commands remain subject to workspace boundaries and approval policies."
        )
        case "accessibility": model.localized(
            "授权目标是当前 ChatOS App；引导窗口可直接打开设置或拖入应用列表。",
            english: "Authorization targets the current ChatOS app. The guide can open Settings or let you drag the app into the list."
        )
        case "screen_recording": model.localized(
            "授权目标是当前 ChatOS App；首次授权后可能需要重启 ChatOS。",
            english: "Authorization targets the current ChatOS app. ChatOS may need to restart after first approval."
        )
        case "full_disk_access": model.localized(
            "需要时可把引导窗口中的 ChatOS App 直接拖入系统设置应用列表。",
            english: "When needed, drag the ChatOS app from the guide directly into the System Settings app list."
        )
        default: permission.note
        }
    }

    private func permissionStatusLabel(_ permission: LocalConnectorSystemPermission) -> String {
        switch permission.status {
        case "ready", "not_applicable", "on_demand": model.localized("已就绪", english: "Ready")
        case "needs_attention", "missing_dependency", "action_required": model.localized("需要授权", english: "Action Required")
        default: permission.statusLabel
        }
    }

    private func permissionRequestLabel(_ permission: LocalConnectorSystemPermission) -> String {
        switch permission.id {
        case "terminal": model.localized("无需授权", english: "No Approval Needed")
        case "accessibility", "screen_recording", "full_disk_access":
            model.localized("授权引导", english: "Open Authorization Guide")
        default: permission.requestLabel
        }
    }
}
