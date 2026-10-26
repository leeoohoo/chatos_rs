import ChatOSCore
import SwiftUI

enum PetTaskProcessRefreshPolicy {
    static func fallbackInterval(hasRealtimeIdentity: Bool) -> Duration {
        // Pet activity events describe durable task changes, but an in-flight MCP
        // progress notification does not increment that durable version. Poll the
        // projection briefly while the task is active so tool progress stays live.
        _ = hasRealtimeIdentity
        return .seconds(2)
    }
}

struct PetTaskProcessInlineView: View {
    @EnvironmentObject private var model: AppModel
    let activity: PetActivity
    let onLoadTask: (PetActivity) async throws -> MessageTask

    @State private var task: MessageTask?
    @State private var isLoading = true
    @State private var errorMessage: String?
    @State private var isRecovering = false
    @State private var isCancelling = false
    @State private var confirmsRecovery = false
    @State private var confirmsCancellation = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if isLoading, task == nil {
                ProgressView("正在加载执行过程…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let task {
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        HStack(alignment: .firstTextBaseline) {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(task.title)
                                    .font(.system(size: 13, weight: .semibold))
                                Text(task.id)
                                    .font(.system(size: 10, design: .monospaced))
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Text(taskStatusTitle(task.normalizedStatus))
                                .font(.system(size: 10, weight: .semibold))
                                .foregroundStyle(taskStatusColor(task.normalizedStatus))
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(
                                    taskStatusColor(task.normalizedStatus).opacity(0.10),
                                    in: Capsule()
                                )
                        }

                        if timelineItems.isEmpty, task.lastRun?.activity == nil {
                            processFallback(task)
                        } else {
                            TaskProcessTimelineView(
                                items: timelineItems,
                                allowsTextSelection: false,
                                liveActivity: task.lastRun?.activity,
                                taskStatus: task.status
                            )
                        }

                        stalledToolNotice(task)

                        if task.isActive
                            || MessageTaskRecoveryPolicy.canForceRestart(task) {
                            HStack {
                                Spacer()
                                Menu {
                                    if MessageTaskRecoveryPolicy.canForceRestart(task) {
                                        Button {
                                            confirmsRecovery = true
                                        } label: {
                                            Label("恢复执行", systemImage: "arrow.clockwise.circle")
                                        }
                                    }
                                    if task.isActive {
                                        Button(role: .destructive) {
                                            confirmsCancellation = true
                                        } label: {
                                            Label("停止任务", systemImage: "stop.circle")
                                        }
                                    }
                                } label: {
                                    if isRecovering || isCancelling {
                                        ProgressView().controlSize(.small)
                                    } else {
                                        Image(systemName: "ellipsis.circle")
                                    }
                                }
                                .menuStyle(.borderlessButton)
                                .fixedSize()
                                .disabled(isRecovering || isCancelling)
                                .help(model.localized("任务操作", english: "Task Actions"))
                            }
                            .controlSize(.small)
                        }

                        if let errorMessage {
                            Label(errorMessage, systemImage: "exclamationmark.triangle")
                                .font(.system(size: 10))
                                .foregroundStyle(.orange)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                VStack(alignment: .leading, spacing: 10) {
                    Label("执行过程加载失败", systemImage: "exclamationmark.triangle")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(.orange)
                    Text(errorMessage ?? model.localized("没有读取到任务详情。", english: "Task details were not returned."))
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                    Button("重试") {
                        Task { await refresh() }
                    }
                    .controlSize(.small)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
        }
        .padding(13)
        .task(id: activityRefreshIdentity) {
            repeat {
                await refresh()
                guard !Task.isCancelled, shouldContinueRefreshing else { return }
                try? await Task.sleep(for: PetTaskProcessRefreshPolicy.fallbackInterval(
                    hasRealtimeIdentity: activity.activityVersion != nil
                        || activity.eventSequence != nil
                ))
            } while !Task.isCancelled
        }
        .confirmationDialog(
            "恢复执行这个任务？",
            isPresented: $confirmsRecovery,
            titleVisibility: .visible
        ) {
            Button("终止当前运行并重新执行", role: .destructive) {
                recover()
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("之前的浏览器点击、导出、授权或文件修改可能已经发生，请确认重复执行是安全的。")
        }
        .confirmationDialog(
            "停止这个任务？",
            isPresented: $confirmsCancellation,
            titleVisibility: .visible
        ) {
            Button("停止任务", role: .destructive) {
                cancel()
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("任务和正在运行的插件进程会被终止；已经发生的外部操作不会自动撤销。")
        }
    }

    private var activityRefreshIdentity: String {
        [
            activity.id,
            activity.activityVersion,
            activity.eventID,
            activity.eventSequence.map(String.init),
            String(activity.updatedAt.timeIntervalSince1970),
        ]
        .compactMap { $0 }
        .joined(separator: "|")
    }

    private var shouldContinueRefreshing: Bool {
        guard let task else { return true }
        return !["completed", "succeeded", "success", "done", "failed", "blocked", "cancelled", "canceled"]
            .contains(task.normalizedStatus)
    }

    private func refresh() async {
        if task == nil { isLoading = true }
        do {
            let loaded = try await onLoadTask(activity)
            guard !Task.isCancelled else { return }
            if task != loaded {
                task = loaded
            }
            if errorMessage != nil {
                errorMessage = nil
            }
        } catch {
            guard !Task.isCancelled else { return }
            let nextError = error.localizedDescription
            if errorMessage != nextError {
                errorMessage = nextError
            }
        }
        if isLoading {
            isLoading = false
        }
    }

    private func recover() {
        guard !isRecovering, !isCancelling else { return }
        isRecovering = true
        errorMessage = nil
        Task {
            do {
                try await model.recoverPetActivity(activity)
                await refresh()
            } catch {
                errorMessage = error.localizedDescription
            }
            isRecovering = false
        }
    }

    private func cancel() {
        guard !isRecovering, !isCancelling else { return }
        isCancelling = true
        errorMessage = nil
        Task {
            do {
                try await model.cancelPetActivity(activity)
                await refresh()
            } catch {
                errorMessage = error.localizedDescription
            }
            isCancelling = false
        }
    }

    private var timelineItems: [TaskProcessTimelineItem] {
        guard let task else { return [] }
        return TaskProcessTimelineBuilder.build(
            processLog: task.processLog,
            taskStatus: task.status
        )
    }

    private func processFallback(_ task: MessageTask) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(
                isTerminal(task.normalizedStatus)
                    ? model.localized("任务结果", english: "Task Result")
                    : model.localized("等待过程更新", english: "Waiting for Process Updates"),
                systemImage: isTerminal(task.normalizedStatus)
                    ? "checkmark.circle"
                    : "arrow.triangle.2.circlepath"
            )
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(isTerminal(task.normalizedStatus) ? Color.green : Color.indigo)

            Text(fallbackDetail(task))
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            if !isTerminal(task.normalizedStatus) {
                Text("正在自动刷新")
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(Color.indigo.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
    }

    @ViewBuilder
    private func stalledToolNotice(_ task: MessageTask) -> some View {
        if let activity = task.lastRun?.activity,
           let lastProgressAt = activity.lastProgressAt ?? activity.startedAt,
           activity.kind == .waitingTool {
            TimelineView(.periodic(from: .now, by: 5)) { context in
                if MessageTaskRecoveryPolicy.shouldOfferDiagnosis(task, now: context.date) {
                    VStack(alignment: .leading, spacing: 5) {
                        Label(
                            model.localized("工具耗时较长", english: "Tool Is Taking Longer Than Expected"),
                            systemImage: "clock.badge.exclamationmark"
                        )
                        .font(.system(size: 11, weight: .semibold))
                        Text(model.localized(
                            "最近一次实际进展在 \(lastProgressAt.formatted(date: .omitted, time: .standard))。这不等于任务失败；你可以继续等待，或从右侧菜单停止后检查已经发生的操作。",
                            english: "The last real progress was at \(lastProgressAt.formatted(date: .omitted, time: .standard)). This does not mean the task failed; you can keep waiting or stop it from the menu and inspect completed side effects."
                        ))
                        .font(.system(size: 10))
                    }
                    .foregroundStyle(.orange)
                    .padding(9)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 8))
                }
            }
        }
    }

    private func fallbackDetail(_ task: MessageTask) -> String {
        let candidates = [
            task.lastRun?.errorMessage,
            task.lastRun?.resultSummary,
            task.resultSummary,
            task.lastRun?.reportContent,
            activity.detail,
            task.objective,
            task.description,
        ]
        for candidate in candidates {
            let value = candidate?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if !value.isEmpty {
                return value
            }
        }
        return isTerminal(task.normalizedStatus)
            ? model.localized("任务已结束，但后端没有返回过程说明。", english: "The task ended, but the backend returned no process details.")
            : model.localized("任务已经开始，后端尚未写入过程节点。", english: "The task has started, but the backend has not recorded process nodes yet.")
    }

    private func isTerminal(_ status: String) -> Bool {
        [
            "completed", "succeeded", "success", "done",
            "failed", "error", "blocked", "cancelled", "canceled",
        ].contains(status)
    }

    private func taskStatusTitle(_ status: String) -> String {
        switch status {
        case "completed", "succeeded", "success", "done": model.localized("已完成", english: "Completed")
        case "running", "processing", "in_progress", "doing": model.localized("执行中", english: "Running")
        case "blocked": model.localized("阻塞", english: "Blocked")
        case "failed", "error": model.localized("失败", english: "Failed")
        case "cancelled", "canceled": model.localized("已取消", english: "Cancelled")
        default: model.localized("等待中", english: "Waiting")
        }
    }

    private func taskStatusColor(_ status: String) -> Color {
        switch status {
        case "completed", "succeeded", "success", "done": .green
        case "running", "processing", "in_progress", "doing": .indigo
        case "blocked": .orange
        case "failed", "error", "cancelled", "canceled": .red
        default: .secondary
        }
    }
}
