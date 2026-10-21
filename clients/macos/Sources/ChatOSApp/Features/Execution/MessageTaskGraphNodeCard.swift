import ChatOSCore
import SwiftUI

struct MessageTaskGraphNodeCard: View {
    @EnvironmentObject private var model: AppModel
    let node: MessageTaskGraphNode
    let renderScale: Double
    let isSelected: Bool
    let isDimmed: Bool
    let onSelect: () -> Void
    let onOpenProcess: () -> Void
    let onOpenDetail: () -> Void
    let onOpenRun: () -> Void
    let onStop: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: scaled(9)) {
            HStack(spacing: scaled(7)) {
                Circle()
                    .fill(statusColor)
                    .frame(width: scaled(8), height: scaled(8))
                Text(statusTitle)
                    .appFont(.system(size: scaledFont(11), weight: .medium))
                    .foregroundStyle(statusColor)
                if node.isCurrentMessage {
                    Text("当前消息")
                        .appFont(.system(size: scaledFont(10), weight: .semibold))
                        .foregroundStyle(AppPalette.ai)
                }
                Spacer()
                if let startedAt = currentActivity?.startedAt {
                    Text(startedAt, style: .timer)
                        .appFont(.system(size: scaledFont(9), design: .monospaced))
                        .foregroundStyle(.secondary)
                }
                if node.groupedTasks.count > 1 {
                    Text("\(node.groupedTasks.count) 阶段")
                        .appFont(.system(size: scaledFont(10)))
                        .foregroundStyle(.secondary)
                }
            }
            Text(node.task.title)
                .appFont(.system(size: scaledFont(13), weight: .semibold))
                .lineLimit(2)
            Text(currentActivity?.detail
                 ?? node.task.description
                 ?? node.task.objective
                 ?? model.localized("暂无任务描述", english: "No task description"))
                .appFont(.system(size: scaledFont(11)))
                .foregroundStyle(.secondary)
                .lineLimit(2)
            HStack(spacing: scaled(6)) {
                actionButton("过程", action: onOpenProcess)
                actionButton(
                    node.task.normalizedStatus == "blocked" ? "处理阻塞" : "详情",
                    action: onOpenDetail
                )
                actionButton("运行", action: onOpenRun)
                    .disabled(node.task.lastRunID == nil)
                if node.task.isCancellableFromGraph {
                    Menu {
                        Button(role: .destructive, action: onStop) {
                            Label("停止任务", systemImage: "stop.circle")
                        }
                    } label: {
                        Image(systemName: "ellipsis")
                            .appFont(.system(size: scaledFont(10), weight: .semibold))
                            .padding(.horizontal, scaled(7))
                            .padding(.vertical, scaled(4))
                            .background(
                                Color.secondary.opacity(0.08),
                                in: RoundedRectangle(cornerRadius: scaled(5))
                            )
                    }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
                    .help("任务操作")
                }
            }
        }
        .padding(scaled(12))
        .frame(
            width: MessageTaskGraphLayout.nodeSize.width * renderScale,
            height: MessageTaskGraphLayout.nodeSize.height * renderScale,
            alignment: .topLeading
        )
        .background(
            Color(nsColor: .windowBackgroundColor),
            in: RoundedRectangle(cornerRadius: scaled(11))
        )
        .overlay {
            RoundedRectangle(cornerRadius: scaled(11)).stroke(
                isSelected ? statusColor : Color(nsColor: .separatorColor),
                lineWidth: scaled(isSelected ? 2 : 1)
            )
        }
        .shadow(
            color: .black.opacity(isSelected ? 0.12 : 0.04),
            radius: scaled(isSelected ? 8 : 3),
            y: scaled(2)
        )
        .opacity(isDimmed ? 0.42 : 1)
        .contentShape(RoundedRectangle(cornerRadius: scaled(11)))
        .onTapGesture(perform: onSelect)
    }

    private func actionButton(
        _ title: String,
        role: ButtonRole? = nil,
        action: @escaping () -> Void
    ) -> some View {
        Button(role: role, action: action) {
            Text(LocalizedStringKey(title))
                .appFont(.system(size: scaledFont(10), weight: .medium))
                .padding(.horizontal, scaled(7))
                .padding(.vertical, scaled(3))
                .background(
                    (role == .destructive ? Color.red : Color.accentColor).opacity(0.08),
                    in: RoundedRectangle(cornerRadius: scaled(5))
                )
        }
        .buttonStyle(.plain)
    }

    private func scaled(_ value: CGFloat) -> CGFloat {
        value * renderScale
    }

    private func scaledFont(_ value: CGFloat) -> CGFloat {
        max(4, scaled(value))
    }

    private var statusTitle: String {
        if let disposition = node.task.resultDisposition {
            return switch disposition {
            case .executionFailed: model.localized("执行失败", english: "Execution Failed")
            case .blocked: model.localized("执行完成，结果受阻", english: "Execution Complete, Result Blocked")
            case .evidenceIncomplete: model.localized("核验不完整", english: "Verification Incomplete")
            case .acceptanceFailed: model.localized("未通过验收", english: "Acceptance Not Met")
            case .acceptancePassed: model.localized("验收通过", english: "Acceptance Passed")
            }
        }
        if let activity = currentActivity {
            return MessageTaskRunActivityPresentation.title(for: activity.kind) {
                model.localized($0, english: $1)
            }
        }
        return switch node.task.normalizedStatus {
        case "completed", "succeeded", "success", "done": model.localized("已完成", english: "Completed")
        case "running", "processing", "in_progress", "doing": model.localized("运行中", english: "Running")
        case "blocked": model.localized("被阻塞", english: "Blocked")
        case "result_blocked": model.localized("执行完成，结果受阻", english: "Execution Complete, Result Blocked")
        case "reviewing_followup": model.localized("审查追加要求", english: "Reviewing Follow-up")
        case "waiting_confirmation": model.localized("等待确认", english: "Awaiting Confirmation")
        case "failed", "error": model.localized("任务未完成", english: "Task Incomplete")
        case "cancelled", "canceled": model.localized("已取消", english: "Cancelled")
        default: model.localized("等待执行", english: "Waiting")
        }
    }

    private var currentActivity: MessageTaskRunActivity? {
        if let activity = node.task.lastRun?.activity { return activity }
        let waiting = node.task.prerequisiteTasks.filter { prerequisite in
            !["completed", "succeeded", "success", "done"].contains(
                prerequisite.status?.lowercased() ?? ""
            )
        }
        guard !waiting.isEmpty,
              ["pending", "ready", "queued"].contains(node.task.normalizedStatus) else {
            return nil
        }
        let names = waiting.prefix(2).map { $0.title ?? $0.id }.joined(separator: "、")
        return .init(
            kind: .waitingDependency,
            detail: names,
            startedAt: node.task.updatedAt
        )
    }

    private var statusColor: Color {
        if let disposition = node.task.resultDisposition {
            return switch disposition {
            case .blocked, .evidenceIncomplete: .orange
            case .executionFailed, .acceptanceFailed: .red
            case .acceptancePassed: .green
            }
        }
        if let activity = currentActivity {
            return switch activity.kind {
            case .waitingDependency, .waitingApproval, .waitingUser, .retryScheduled, .paused:
                .orange
            case .waitingModel, .runningModel, .waitingTool, .startingProcess,
                 .readingLargeResult, .restoringCapability:
                AppPalette.ai
            }
        }
        return switch node.task.normalizedStatus {
        case "completed", "succeeded", "success", "done": .green
        case "running", "processing", "in_progress", "doing": AppPalette.ai
        case "blocked": .orange
        case "reviewing_followup", "waiting_confirmation": .orange
        case "failed", "error", "cancelled", "canceled": .red
        default: .secondary
        }
    }
}

private extension MessageTask {
    var isCancellableFromGraph: Bool {
        isActive
    }
}

struct DotGridSurface: View {
    var body: some View {
        Canvas { context, size in
            for x in stride(from: CGFloat(10), to: size.width, by: 20) {
                for y in stride(from: CGFloat(10), to: size.height, by: 20) {
                    let dot = Path(ellipseIn: CGRect(x: x, y: y, width: 1.5, height: 1.5))
                    context.fill(dot, with: .color(.secondary.opacity(0.16)))
                }
            }
        }
    }
}
