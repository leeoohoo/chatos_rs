import ChatOSCore
import SwiftUI

struct ProjectExecutionStatusBanner: View {
    @ObservedObject var viewModel: MessageTaskWorkspaceViewModel
    @State private var showsAbandonConfirmation = false

    var body: some View {
        if viewModel.executionState.isProjectExecution {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: presentation.icon)
                    .appFont(.body.weight(.semibold))
                    .foregroundStyle(presentation.tint)
                    .frame(width: 22)
                VStack(alignment: .leading, spacing: 4) {
                    Text(presentation.title).appFont(.subheadline.weight(.semibold))
                    Text(presentation.detail)
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 16)
                actions
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .background(presentation.tint.opacity(0.08))
            .overlay(alignment: .bottom) { Divider() }
            .confirmationDialog(
                "确定放弃这份执行计划？",
                isPresented: $showsAbandonConfirmation,
                titleVisibility: .visible
            ) {
                Button("放弃计划并清理任务", role: .destructive, action: viewModel.abandonPlan)
                Button("取消", role: .cancel) {}
            } message: {
                Text("尚未运行的任务节点会被清理；此操作不会启动任何任务。")
            }
        }
    }

    @ViewBuilder
    private var actions: some View {
        let state = viewModel.executionState
        if state.phase == .awaitingConfirmation {
            Button("放弃计划", role: .destructive) { showsAbandonConfirmation = true }
                .disabled(viewModel.isMutatingPlan || state.identity == nil)
            Button("确认执行", systemImage: "play.fill", action: viewModel.confirmExecution)
                .buttonStyle(.borderedProminent)
                .disabled(viewModel.isMutatingPlan || !state.canConfirm)
        } else if [.blocked, .failed].contains(state.phase), state.identity != nil {
            Button("清理失败计划", role: .destructive) { showsAbandonConfirmation = true }
                .disabled(viewModel.isMutatingPlan)
        }
        if viewModel.isMutatingPlan { ProgressView().controlSize(.small) }
    }

    private var presentation: Presentation {
        if viewModel.isPlanStopped {
            return Presentation(
                icon: "stop.circle.fill",
                title: "执行计划已放弃",
                detail: "任务没有启动，可以回到需求中重新生成计划。",
                tint: .secondary
            )
        }
        if let message = viewModel.planActionMessage,
           viewModel.executionState.phase == .running {
            return Presentation(icon: "checkmark.circle.fill", title: "已开始执行", detail: message, tint: .green)
        }
        switch viewModel.executionState.phase {
        case .planning:
            return Presentation(
                icon: "wand.and.stars",
                title: "正在生成完整任务图",
                detail: "生成完成前不会启动任务；你可以先查看已出现的节点。",
                tint: .blue
            )
        case .awaitingConfirmation:
            let detail = viewModel.executionState.identity == nil
                ? "任务图已生成，但消息缺少完整执行标识，已禁用启动操作。"
                : "请检查每个节点和依赖关系。只有确认后，任务才会开始运行。"
            return Presentation(icon: "clock.badge.exclamationmark", title: "任务图已生成，等待确认", detail: detail, tint: .orange)
        case .running:
            return Presentation(icon: "play.circle.fill", title: "任务正在执行", detail: "节点会按照依赖顺序运行，状态将自动刷新。", tint: .green)
        case .completed:
            return Presentation(icon: "checkmark.circle.fill", title: "任务已完成", detail: "这批任务的所有节点均已完成。", tint: .green)
        case .blocked:
            return Presentation(
                icon: "exclamationmark.octagon.fill",
                title: "执行存在阻塞",
                detail: viewModel.executionFailureReason
                    ?? "选择阻塞节点查看详情、补充指令并重试。",
                tint: .red
            )
        case .failed:
            return Presentation(
                icon: "xmark.octagon.fill",
                title: "执行失败或已取消",
                detail: viewModel.executionFailureReason
                    ?? "选择节点检查运行详情，或清理本次失败计划。",
                tint: .red
            )
        case .stopped:
            return Presentation(icon: "stop.circle.fill", title: "执行计划已停止", detail: "任务未继续执行。", tint: .secondary)
        case .graphUnavailable:
            return Presentation(icon: "link.badge.plus", title: "未找到关联任务图", detail: "消息记录了执行计划，但网关没有返回对应节点。请刷新或检查执行批次关联。", tint: .orange)
        case .unavailable:
            return Presentation(icon: "circle", title: "", detail: "", tint: .secondary)
        }
    }
}

private struct Presentation {
    var icon: String
    var title: String
    var detail: String
    var tint: Color
}
