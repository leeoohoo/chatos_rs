import ChatOSCore
import SwiftUI

struct LocalConnectorPluginRuntimeBadge: View {
    @EnvironmentObject private var model: AppModel
    let status: LocalConnectorPluginRuntimeStatus

    var body: some View {
        let presentation: (String, Color, String) = switch status {
        case .ready: (
            model.localized("MCP 已可用", english: "MCP Available"),
            .green,
            "checkmark.circle.fill"
        )
        case .unavailable: (
            model.localized("MCP 启动失败", english: "MCP Unavailable"),
            .red,
            "xmark.octagon.fill"
        )
        case .disabled: (
            model.localized("已停用", english: "Disabled"),
            .secondary,
            "pause.circle.fill"
        )
        case .notChecked: (
            model.localized("等待健康检查", english: "Health Check Pending"),
            .secondary,
            "questionmark.circle.fill"
        )
        }
        Label(presentation.0, systemImage: presentation.2)
            .appFont(.caption2.weight(.semibold))
            .foregroundStyle(presentation.1)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(presentation.1.opacity(0.1), in: Capsule())
    }
}

struct LocalConnectorPluginRuntimeSummary: View {
    @EnvironmentObject private var model: AppModel
    let plugin: LocalConnectorPlugin

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Label(
                model.localized("运行状态", english: "Runtime"),
                systemImage: "bolt.horizontal.circle"
            )
            .appFont(.caption.weight(.semibold))
            detail
            Spacer()
            if let checkedAt = plugin.runtimeLastCheckedAt {
                Text(checkedAt, style: .relative)
                    .appFont(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    @ViewBuilder
    private var detail: some View {
        switch plugin.runtimeStatus {
        case .ready:
            if let toolCount = plugin.runtimeToolCount {
                Text(model.localized(
                    "已完成 MCP 握手，发现 \(toolCount) 个工具",
                    english: "MCP handshake succeeded with \(toolCount) tools"
                ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            } else {
                Text(model.localized(
                    "MCP 已通过运行检查",
                    english: "MCP runtime check passed"
                ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
        case .notChecked:
            Text(model.localized(
                "点击“检查更新”以启动 MCP 并验证工具清单",
                english: "Use Check for Updates to start the MCP and verify its tool list"
            ))
                .appFont(.caption)
                .foregroundStyle(.secondary)
        case .unavailable:
            let error = plugin.runtimeError ?? model.localized(
                "MCP 未能完成启动检查",
                english: "MCP could not complete its startup check"
            )
            Text(error)
                .appFont(.caption)
                .foregroundStyle(.red)
                .lineLimit(3)
                .help(error)
        case .disabled:
            Text(model.localized(
                "启用 Plugin 后可重新检查 MCP",
                english: "Enable the plugin to check its MCP again"
            ))
                .appFont(.caption)
                .foregroundStyle(.secondary)
        case nil:
            EmptyView()
        }
    }
}
