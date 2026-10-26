import ChatOSCore
import SwiftUI

enum PetTaskLiveActivityPollingPolicy {
    static let activeInterval: Duration = .seconds(2)
}

enum PetTaskLiveActivitySummary {
    static func operationLabel(_ operation: String?) -> String? {
        MessageTaskRunActivityPresentation.operationLabel(operation)
    }

    static func recentProgress(in task: MessageTask) -> TaskProcessTimelineItem? {
        TaskProcessTimelineBuilder.build(
            processLog: task.processLog,
            taskStatus: task.status
        ).last
    }
}

struct PetTaskLiveActivityBubble: View {
    @EnvironmentObject private var model: AppModel
    let activity: PetActivity
    let onLoadTask: (PetActivity) async throws -> MessageTask

    @State private var task: MessageTask?

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    if taskStatusOverrideTitle != nil {
                        Image(systemName: "pause.circle.fill")
                            .foregroundStyle(.orange)
                    } else {
                        ProgressView()
                            .controlSize(.small)
                            .tint(.accentColor)
                    }
                    VStack(alignment: .leading, spacing: 2) {
                        Text(currentStageTitle)
                            .font(.system(size: 13, weight: .semibold))
                            .lineLimit(1)
                        HStack(spacing: 4) {
                            if let operation = currentOperation {
                                Text(operation).lineLimit(1)
                            } else {
                                Text(model.localized("正在获取实时阶段", english: "Loading live stage"))
                            }
                            if let startedAt = currentActivity?.startedAt {
                                Text("·")
                                Text(elapsedText(since: startedAt, now: context.date))
                                    .monospacedDigit()
                            }
                        }
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 4)
                    Image(systemName: "chevron.up")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.secondary)
                }

                HStack(spacing: 5) {
                    if let task, MessageTaskRecoveryPolicy.shouldOfferDiagnosis(task, now: context.date) {
                        Image(systemName: "clock.badge.exclamationmark")
                        Text(model.localized(
                            "工具较久没有报告新进展，展开可诊断或停止",
                            english: "The tool has not reported progress for a while; expand to diagnose or stop"
                        ))
                    } else if let detail = currentActivity?.detail,
                              !detail.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        Image(systemName: "waveform.path.ecg")
                        Text(detail).lineLimit(1)
                    } else if let recent = task.flatMap(PetTaskLiveActivitySummary.recentProgress) {
                        Image(systemName: "checkmark.circle.fill")
                            .foregroundStyle(.green)
                        Text(model.localized("最近进展：", english: "Latest: ") + recent.title)
                            .lineLimit(1)
                        if let date = recent.occurredAtDate {
                            Text("·")
                            Text(date, style: .relative)
                        }
                    } else {
                        Image(systemName: "waveform.path.ecg")
                        Text(model.localized("任务正在继续执行", english: "The task is still running"))
                    }
                    Spacer(minLength: 0)
                }
                .font(.system(size: 10))
                .foregroundStyle(stalled(at: context.date) ? Color.orange : Color.secondary)

                if let progressAt = currentActivity?.lastProgressAt ?? currentActivity?.startedAt {
                    HStack(spacing: 4) {
                        Image(systemName: "clock.arrow.circlepath")
                        Text(model.localized("最近真实进展", english: "Last real progress"))
                        Text(progressAt, style: .relative)
                        Spacer(minLength: 0)
                    }
                    .font(.system(size: 9))
                    .foregroundStyle(.tertiary)
                }
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .contentShape(Rectangle())
        .task(id: refreshIdentity) {
            repeat {
                await refresh()
                guard !Task.isCancelled, shouldContinueRefreshing else { return }
                try? await Task.sleep(for: PetTaskLiveActivityPollingPolicy.activeInterval)
            } while !Task.isCancelled
        }
    }

    private var currentActivity: MessageTaskRunActivity? {
        task?.lastRun?.activity
    }

    private var currentStageTitle: String {
        if let title = MessageTaskRunActivityPresentation.title(
            forTaskStatus: task?.status,
            fallback: currentActivity?.kind,
            localized: { model.localized($0, english: $1) }
        ) {
            return title
        }
        return PetActivityPresentation.displayTitle(for: activity, model: model)
    }

    private var taskStatusOverrideTitle: String? {
        MessageTaskRunActivityPresentation.taskStatusTitleOverride(for: task?.status) {
            model.localized($0, english: $1)
        }
    }

    private var currentOperation: String? {
        if let detail = MessageTaskRunActivityPresentation.taskStatusDetailOverride(
            for: task?.status,
            localized: { model.localized($0, english: $1) }
        ) {
            return detail
        }
        return PetTaskLiveActivitySummary.operationLabel(currentActivity?.operation)
    }

    private var refreshIdentity: String {
        [
            activity.id,
            activity.activityVersion,
            activity.eventID,
            activity.eventSequence.map(String.init),
        ].compactMap { $0 }.joined(separator: "|")
    }

    private var shouldContinueRefreshing: Bool {
        guard let task else { return true }
        return !["completed", "succeeded", "success", "done", "failed", "blocked", "cancelled", "canceled"]
            .contains(task.normalizedStatus)
    }

    private func refresh() async {
        guard let loaded = try? await onLoadTask(activity), !Task.isCancelled else { return }
        if task != loaded { task = loaded }
    }

    private func stalled(at now: Date) -> Bool {
        task.map { MessageTaskRecoveryPolicy.shouldOfferDiagnosis($0, now: now) } ?? false
    }

    private func elapsedText(since start: Date, now: Date) -> String {
        let seconds = max(0, Int(now.timeIntervalSince(start)))
        if seconds < 60 { return "\(seconds) 秒" }
        let minutes = seconds / 60
        let remainder = seconds % 60
        return remainder == 0 ? "\(minutes) 分钟" : "\(minutes)分 \(remainder)秒"
    }
}
