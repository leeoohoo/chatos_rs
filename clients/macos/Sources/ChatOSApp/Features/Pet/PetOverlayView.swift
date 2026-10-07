import ChatOSCore
import SwiftUI

struct PetMessageView: View {
    enum Layout {
        static let compactWidth: CGFloat = 310
        static let compactHeight: CGFloat = 112
        static let expandedWidth: CGFloat = 400
        static let minimumExpandedHeight: CGFloat = 140
    }

    @EnvironmentObject var model: AppModel
    @ObservedObject var store: PetOverlayStore
    @ObservedObject var interactionState: PetOverlayInteractionState
    @ObservedObject var approvalViewModel: LocalConnectorControlCenterViewModel
    let activityScope: PetMessageActivityScope
    let onOpen: (PetActivity) -> Void
    let onRetry: (PetActivity, String) async throws -> Void
    let onCancel: (PetActivity) async throws -> Void
    let onLoadTask: (PetActivity) async throws -> MessageTask
    let onLoadPrompt: (PetActivity) async throws -> AskUserPrompt
    let onSubmitPrompt: (AskUserPrompt, AskUserSubmission) async throws -> Void
    let onCancelPrompt: (AskUserPrompt) async throws -> Void

    @State var retryInstruction = ""
    @State var isRetrying = false
    @State var actionMessage: String?
    @State var actionSucceeded = false
    @State var cancellingActivityIDs: Set<String> = []
    @State var cancellationErrors: [String: String] = [:]

    var body: some View {
        let inspectedActivity = interactionState.inspectedTaskActivity.map { inspected in
            store.activities.first(where: { $0.id == inspected.id }) ?? inspected
        }
        if let primaryActivity = scopedPrimaryActivity ?? inspectedActivity {
            let activity = inspectedActivity
                ?? (interactionState.isMessageExpanded
                ? interactionState.selectedActivityID.flatMap { selectedID in
                    store.activities.first(where: {
                        $0.id == selectedID && activityScope.contains($0)
                    })
                } ?? primaryActivity
                : primaryActivity)
            Group {
                if interactionState.isMessageExpanded {
                    expandedCard(activity)
                } else {
                    compactCard(activity)
                }
            }
            .frame(
                minWidth: interactionState.isMessageExpanded
                    ? Layout.expandedWidth
                    : Layout.compactWidth,
                maxWidth: .infinity,
                minHeight: interactionState.isMessageExpanded
                    ? Layout.minimumExpandedHeight
                    : Layout.compactHeight,
                maxHeight: .infinity,
                alignment: .topLeading
            )
            .background(
                Color(nsColor: .windowBackgroundColor),
                in: RoundedRectangle(cornerRadius: 16)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 16)
                    .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
            }
            .shadow(color: .black.opacity(0.28), radius: 18, y: 9)
            .onChange(of: activity.id) {
                retryInstruction = ""
                actionMessage = nil
                actionSucceeded = false
            }
            .onChange(of: store.activities.map(\.id)) {
                guard interactionState.isMessageExpanded else { return }
                guard interactionState.inspectedTaskActivity == nil else { return }
                let selectedActivity = interactionState.selectedActivityID.flatMap { selectedID in
                    store.activities.first(where: {
                        $0.id == selectedID && activityScope.contains($0)
                    })
                }
                if selectedActivity == nil {
                    interactionState.selectedActivityID = scopedPrimaryActivity?.id
                    return
                }
                guard activityScope == .primary else { return }
                guard selectedActivity?.kind != .waitingForApproval,
                      selectedActivity?.kind != .waitingForUser,
                      let pendingActivity = store.activities.first(where: {
                          $0.kind == .waitingForApproval || $0.kind == .waitingForUser
                      }) else {
                    return
                }
                interactionState.selectedActivityID = pendingActivity.id
            }
            .onChange(of: interactionState.isMessageExpanded) {
                if !interactionState.isMessageExpanded {
                    interactionState.selectedActivityID = nil
                    interactionState.inspectedTaskActivity = nil
                }
            }
        }
    }

    var scopedPrimaryActivity: PetActivity? {
        switch activityScope {
        case .primary:
            return store.presentation.primaryActivity.flatMap {
                activityScope.contains($0) ? $0 : nil
            }
        case .running:
            return runningActivities().first
        }
    }

    func compactCard(_ activity: PetActivity) -> some View {
        Button {
            interactionState.selectedActivityID = activity.id
            interactionState.isMessageExpanded = true
        } label: {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    Image(systemName: PetActivityPresentation.messageIcon(for: activity.kind))
                        .foregroundStyle(PetActivityPresentation.messageTint(for: activity.kind))
                    Text(PetActivityPresentation.displayTitle(for: activity, model: model))
                        .font(.system(size: 13, weight: .semibold))
                        .lineLimit(2)
                    Spacer(minLength: 4)
                    Image(systemName: "chevron.up")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.secondary)
                }

                if let detail = PetActivityPresentation.displayText(activity.detail) {
                    Text(detail)
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }

                HStack {
                    if activityScope == .running,
                       shouldShowActiveWorkSummary(for: activity) {
                        Text("\(store.presentation.activeWorkCount) 项正在执行")
                    } else {
                        Text(compactHint(for: activity))
                    }
                    Spacer()
                    Text(activity.kind.requiresAttention
                         ? model.localized("展开处理", english: "Review")
                         : model.localized("展开查看", english: "Expand"))
                        .foregroundStyle(Color.accentColor)
                }
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
            }
            .padding(12)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .frame(
            minWidth: Layout.compactWidth,
            minHeight: Layout.compactHeight,
            alignment: .topLeading
        )
    }

    func expandedCard(_ activity: PetActivity) -> some View {
        let isInspectingTask = interactionState.inspectedTaskActivity?.id == activity.id
        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 9) {
                if isInspectingTask {
                    Button {
                        interactionState.inspectedTaskActivity = nil
                    } label: {
                        Image(systemName: "chevron.left")
                    }
                    .buttonStyle(.plain)
                    .help("返回任务列表")
                }
                Image(systemName: PetActivityPresentation.messageIcon(for: activity.kind))
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(PetActivityPresentation.messageTint(for: activity.kind))
                    .frame(width: 28, height: 28)
                    .background(PetActivityPresentation.messageTint(for: activity.kind).opacity(0.11), in: Circle())
                VStack(alignment: .leading, spacing: 2) {
                    Text(isInspectingTask ? PetActivityPresentation.displayTitle(for: activity, model: model) : expandedPanelTitle(for: activity))
                        .font(.system(size: 14, weight: .semibold))
                        .lineLimit(2)
                    HStack(spacing: 4) {
                        Text(isInspectingTask
                             ? model.localized("执行过程", english: "Execution Process")
                             : expandedPanelSubtitle(for: activity))
                        Text("·")
                        Text(activity.updatedAt, style: .relative)
                    }
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                }
                Spacer()
                Button {
                    interactionState.selectedActivityID = nil
                    interactionState.inspectedTaskActivity = nil
                    interactionState.isMessageExpanded = false
                } label: {
                    Image(systemName: "chevron.down")
                }
                .buttonStyle(.plain)
                .help("收起")
                Button {
                    onOpen(activity)
                } label: {
                    Image(systemName: "arrow.up.right.square")
                }
                .buttonStyle(.plain)
                .help("在 ChatOS 中打开")
            }
            .padding(13)

            Divider()

            if isInspectingTask {
                PetTaskProcessInlineView(activity: activity, onLoadTask: onLoadTask)
            } else if activity.kind == .working {
                runningActivitiesSection(runningActivities(), showsHeader: false)
            } else if let approval = approval(for: activity) {
                approvalContent(approval)
            } else if activity.kind == .waitingForUser,
                      activity.source == .askUserPrompt {
                PetAskUserInlineView(
                    activity: activity,
                    onLoadPrompt: onLoadPrompt,
                    onSubmitPrompt: onSubmitPrompt,
                    onCancelPrompt: onCancelPrompt,
                    onResolved: { store.dismiss(activity, disposition: .handled) }
                )
            } else if activity.kind == .blocked || activity.kind == .failed {
                retryContent(activity)
            } else {
                genericContent(activity)
            }

            let otherAttention = attentionActivities(excluding: activity.id)
            if activityScope == .primary,
               !isInspectingTask,
               !otherAttention.isEmpty {
                Divider()
                attentionActivitiesSection(otherAttention)
            }

            let otherCompleted = completedTaskActivities(excluding: activity.id)
            if activityScope == .primary,
               !isInspectingTask,
               !otherCompleted.isEmpty {
                Divider()
                completedActivitiesSection(otherCompleted)
            }
        }
    }

    func completedActivitiesSection(_ activities: [PetActivity]) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack {
                Label("已完成", systemImage: "checkmark.circle.fill")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.green)
                Spacer()
                Text("\(activities.count) 项")
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }

            ScrollView {
                VStack(spacing: 6) {
                    ForEach(activities) { completed in
                        HStack(spacing: 8) {
                            Button {
                                interactionState.selectedActivityID = completed.id
                            } label: {
                                HStack(spacing: 8) {
                                    Image(systemName: "checkmark.circle.fill")
                                        .foregroundStyle(.green)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(PetActivityPresentation.displayTitle(for: completed, model: model))
                                            .font(.system(size: 11, weight: .medium))
                                            .lineLimit(1)
                                        Text(completed.updatedAt, style: .relative)
                                            .font(.system(size: 10))
                                            .foregroundStyle(.secondary)
                                    }
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)

                            if canLoadTask(completed) {
                                Button("过程") { showTaskProcess(completed) }
                                    .controlSize(.mini)
                            }
                            Button {
                                store.dismiss(completed, disposition: .acknowledged)
                            } label: {
                                Image(systemName: "checkmark")
                            }
                            .buttonStyle(.plain)
                            .help("知道了")
                        }
                        .padding(8)
                        .background(
                            Color(nsColor: .controlBackgroundColor).opacity(0.58),
                            in: RoundedRectangle(cornerRadius: 9)
                        )
                    }
                }
            }
            .frame(maxHeight: 125)
        }
        .padding(12)
    }

}
