import AppKit
import ChatOSCore
import SwiftUI

struct RequirementExecutionStartSheet: View {
    let requirement: ProjectRequirement
    let isStarting: Bool
    let onStart: (String?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var feedback = ""

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            HSplitView {
                planningSidebar
                    .frame(minWidth: 330, idealWidth: 370, maxWidth: 430)
                graphPlaceholder
                    .frame(minWidth: 650)
            }
            footer
        }
        .frame(width: sheetSize.width, height: sheetSize.height)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var header: some View {
        HStack(alignment: .top, spacing: 14) {
            VStack(alignment: .leading, spacing: 5) {
                Text("执行计划工作台").appFont(.title3.weight(.semibold))
                Text(requirement.title)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            Spacer()
            StatusCapsule(title: "只生成计划，不会立即执行", color: .orange)
            Button("关闭", action: dismiss.callAsFunction)
        }
        .padding(16)
    }

    private var planningSidebar: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 8) {
                Label(
                    isStarting ? "正在准备执行计划" : "等待开始生成执行计划",
                    systemImage: isStarting ? "wand.and.stars" : "play.circle"
                )
                .appFont(.headline)
                Text(isStarting
                     ? "云端规划 Agent 正在读取需求、技术文档和项目任务，并创建完整依赖关系。"
                     : "开始后，规划 Agent 会先生成任务节点和依赖图；只有你检查并确认后才会真正执行。")
                    .appFont(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(16)

            Divider()

            VStack(alignment: .leading, spacing: 16) {
                PlanStep(number: 1, title: "读取需求与技术文档", detail: "整理本次执行需要的完整上下文", active: isStarting)
                PlanStep(number: 2, title: "创建任务与依赖", detail: "生成可检查的任务流程图", active: false)
                PlanStep(number: 3, title: "等待你的确认", detail: "确认前不会调用 Local Connector 执行", active: false)
            }
            .padding(16)

            Spacer()

            Divider()

            VStack(alignment: .leading, spacing: 8) {
                Label("调整本次计划", systemImage: "text.bubble")
                    .appFont(.subheadline.weight(.semibold))
                TextField(
                    "例如：先补测试，再拆分接口；把部署放到最后……",
                    text: $feedback,
                    axis: .vertical
                )
                .textFieldStyle(.roundedBorder)
                .lineLimit(4...8)
                .disabled(isStarting)
                Text("可以留空。这里的意见只影响本次执行计划。")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding(16)
        }
        .background(AppPalette.surfaceSubtle)
    }

    private var graphPlaceholder: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Label("实时任务流程图", systemImage: "point.3.connected.trianglepath.dotted")
                        .appFont(.headline)
                    Text("创建第一个任务后，流程图会立即在这里更新。")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            .padding(16)
            Divider()
            VStack(spacing: 12) {
                if isStarting {
                    ProgressView().controlSize(.large)
                } else {
                    Image(systemName: "point.3.connected.trianglepath.dotted")
                        .appFont(.system(size: 34))
                        .foregroundStyle(.secondary)
                }
                Text(isStarting ? "等待第一个任务节点" : "任务流程尚未开始生成")
                    .appFont(.headline)
                Text(isStarting
                     ? "这里只展示规划结果，不会提前启动任务。"
                     : "点击下方按钮后开始生成；生成完毕后可以逐个检查任务节点、过程和运行详情。")
                    .appFont(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 430)
            }
            .padding(30)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private var footer: some View {
        HStack {
            Text("生成执行计划和最终执行是两个独立操作。")
                .appFont(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            Button("取消", action: dismiss.callAsFunction)
                .disabled(isStarting)
            Button(isStarting ? "正在生成执行计划…" : "开始生成执行计划", systemImage: "play.fill") {
                let value = feedback.trimmingCharacters(in: .whitespacesAndNewlines)
                onStart(value.isEmpty ? nil : value)
            }
            .buttonStyle(.borderedProminent)
            .disabled(isStarting)
        }
        .padding(14)
        .overlay(alignment: .top) { Divider() }
    }

    private var sheetSize: CGSize {
        let visible = NSScreen.main?.visibleFrame.size ?? CGSize(width: 1440, height: 900)
        return CGSize(
            width: min(1320, max(1060, visible.width - 100)),
            height: min(820, max(700, visible.height - 120))
        )
    }
}

private struct PlanStep: View {
    let number: Int
    let title: String
    let detail: String
    let active: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 11) {
            ZStack {
                Circle()
                    .fill(active ? AppPalette.ai.opacity(0.14) : Color.secondary.opacity(0.10))
                if active {
                    ProgressView().controlSize(.mini)
                } else {
                    Text("\(number)").appFont(.caption2.weight(.bold)).foregroundStyle(.secondary)
                }
            }
            .frame(width: 24, height: 24)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).appFont(.subheadline.weight(.medium))
                Text(detail).appFont(.caption).foregroundStyle(.secondary)
            }
        }
    }
}
