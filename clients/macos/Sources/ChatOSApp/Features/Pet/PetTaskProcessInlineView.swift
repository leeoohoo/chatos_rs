import ChatOSCore
import SwiftUI

struct PetTaskProcessInlineView: View {
    @EnvironmentObject private var model: AppModel
    let activity: PetActivity
    let onLoadTask: (PetActivity) async throws -> MessageTask

    @State private var task: MessageTask?
    @State private var isLoading = true
    @State private var errorMessage: String?

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

                        if timelineItems.isEmpty {
                            processFallback(task)
                        } else {
                            TaskProcessTimelineView(
                                items: timelineItems,
                                allowsTextSelection: false
                            )
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
        .task(id: activity.id) {
            repeat {
                await refresh()
                guard !Task.isCancelled, shouldContinueRefreshing else { return }
                try? await Task.sleep(for: .seconds(5))
            } while !Task.isCancelled
        }
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
