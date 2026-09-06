import ChatOSCore
import SwiftUI

enum PetDragDirection: Equatable, Sendable {
    case left
    case right
}

@MainActor
final class PetOverlayInteractionState: ObservableObject {
    @Published var isDragging = false
    @Published var dragDirection: PetDragDirection = .right
    @Published var isMessageExpanded = false
    @Published var selectedActivityID: String?
    @Published var inspectedTaskActivity: PetActivity?
    @Published var isQuickChatPresented = false
    @Published var selectedQuickChatResourceID: String?
}

struct PetCharacterView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var store: PetOverlayStore
    @ObservedObject var interactionState: PetOverlayInteractionState

    var body: some View {
        PetSpriteAnimationView(
            animationState: store.presentation.animationState,
            isDragging: interactionState.isDragging,
            dragDirection: interactionState.dragDirection
        )
        .contentShape(Rectangle())
        .accessibilityLabel(accessibilityText)
        .help(accessibilityText)
    }

    private var accessibilityText: String {
        store.presentation.primaryActivity?.title
            ?? model.localized("ChatOS 宠物空闲中", english: "ChatOS pet is idle")
    }
}

struct PetMessageView: View {
    private enum Layout {
        static let compactWidth: CGFloat = 310
        static let compactHeight: CGFloat = 112
        static let expandedWidth: CGFloat = 400
        static let minimumExpandedHeight: CGFloat = 140
    }

    @EnvironmentObject private var model: AppModel
    @ObservedObject var store: PetOverlayStore
    @ObservedObject var interactionState: PetOverlayInteractionState
    @ObservedObject var approvalViewModel: LocalConnectorControlCenterViewModel
    let onOpen: (PetActivity) -> Void
    let onRetry: (PetActivity, String) async throws -> Void
    let onCancel: (PetActivity) async throws -> Void
    let onLoadTask: (PetActivity) async throws -> MessageTask
    let onLoadPrompt: (PetActivity) async throws -> AskUserPrompt
    let onSubmitPrompt: (AskUserPrompt, AskUserSubmission) async throws -> Void
    let onCancelPrompt: (AskUserPrompt) async throws -> Void

    @State private var retryInstruction = ""
    @State private var isRetrying = false
    @State private var actionMessage: String?
    @State private var actionSucceeded = false
    @State private var cancellingActivityIDs: Set<String> = []
    @State private var cancellationErrors: [String: String] = [:]

    var body: some View {
        if let primaryActivity = store.presentation.primaryActivity
            ?? interactionState.inspectedTaskActivity {
            let activity = interactionState.inspectedTaskActivity
                ?? (interactionState.isMessageExpanded
                ? interactionState.selectedActivityID.flatMap { selectedID in
                    store.activities.first(where: { $0.id == selectedID })
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
                    store.activities.first(where: { $0.id == selectedID })
                }
                if selectedActivity == nil {
                    interactionState.selectedActivityID = store.presentation.primaryActivity?.id
                    return
                }
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

    private func compactCard(_ activity: PetActivity) -> some View {
        Button {
            interactionState.selectedActivityID = activity.id
            interactionState.isMessageExpanded = true
        } label: {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    Image(systemName: messageIcon(for: activity.kind))
                        .foregroundStyle(messageTint(for: activity.kind))
                    Text(displayTitle(for: activity))
                        .font(.system(size: 13, weight: .semibold))
                        .lineLimit(2)
                    Spacer(minLength: 4)
                    Image(systemName: "chevron.up")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.secondary)
                }

                if let detail = displayText(activity.detail) {
                    Text(detail)
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }

                HStack {
                    if shouldShowActiveWorkSummary(for: activity) {
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

    private func expandedCard(_ activity: PetActivity) -> some View {
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
                Image(systemName: messageIcon(for: activity.kind))
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(messageTint(for: activity.kind))
                    .frame(width: 28, height: 28)
                    .background(messageTint(for: activity.kind).opacity(0.11), in: Circle())
                VStack(alignment: .leading, spacing: 2) {
                    Text(isInspectingTask ? displayTitle(for: activity) : expandedPanelTitle(for: activity))
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
            } else if activity.kind == .working || activity.kind == .reviewing {
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
            if !isInspectingTask, !otherAttention.isEmpty {
                Divider()
                attentionActivitiesSection(otherAttention)
            }

            let otherRunning = runningActivities(excluding: activity.id)
            if !isInspectingTask,
               activity.kind != .working,
               activity.kind != .reviewing,
               !otherRunning.isEmpty {
                Divider()
                runningActivitiesSection(otherRunning, showsHeader: true)
            }

            let otherCompleted = completedTaskActivities(excluding: activity.id)
            if !isInspectingTask, !otherCompleted.isEmpty {
                Divider()
                completedActivitiesSection(otherCompleted)
            }
        }
    }

    private func completedActivitiesSection(_ activities: [PetActivity]) -> some View {
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
                                        Text(displayTitle(for: completed))
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

    private func attentionActivitiesSection(_ activities: [PetActivity]) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack {
                Text("待你处理")
                    .font(.system(size: 12, weight: .semibold))
                Spacer()
                Text("\(activities.count) 项")
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }
            ScrollView {
                VStack(spacing: 6) {
                    ForEach(activities) { activity in
                        Button {
                            interactionState.selectedActivityID = activity.id
                        } label: {
                            HStack(spacing: 8) {
                                Image(systemName: messageIcon(for: activity.kind))
                                    .foregroundStyle(messageTint(for: activity.kind))
                                Text(displayTitle(for: activity))
                                    .font(.system(size: 11, weight: .medium))
                                    .lineLimit(1)
                                Spacer()
                                Text("处理")
                                    .font(.system(size: 10))
                                    .foregroundStyle(Color.accentColor)
                            }
                            .padding(8)
                            .background(
                                Color(nsColor: .controlBackgroundColor).opacity(0.58),
                                in: RoundedRectangle(cornerRadius: 9)
                            )
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            .frame(maxHeight: 110)
        }
        .padding(12)
    }

    private func approvalContent(_ approval: LocalConnectorPendingApproval) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Text(riskLabel(approval.risk))
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(riskColor(approval.risk))
                    .padding(.horizontal, 7)
                    .padding(.vertical, 3)
                    .background(riskColor(approval.risk).opacity(0.12), in: Capsule())
                Text(approval.source)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                Spacer()
            }

            ScrollView {
                VStack(alignment: .leading, spacing: 9) {
                    Label("审批内容", systemImage: "terminal")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(.secondary)
                    Text(approval.command)
                        .font(.system(size: 12, design: .monospaced))
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(9)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(
                            Color(nsColor: .textBackgroundColor).opacity(0.72),
                            in: RoundedRectangle(cornerRadius: 8)
                        )
                    Label(approval.cwd, systemImage: "folder")
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary)
                    if let reason = displayText(approval.reason) {
                        Text(reason)
                            .font(.system(size: 11))
                            .foregroundStyle(.secondary)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            Divider()

            HStack(spacing: 8) {
                if approval.availableDecisions.contains("decline") {
                    Button("拒绝", role: .destructive) {
                        approvalViewModel.resolveApproval(id: approval.id, decision: "decline")
                    }
                }
                Spacer()
                if approval.availableDecisions.contains("accept") {
                    Button("仅本次允许") {
                        approvalViewModel.resolveApproval(id: approval.id, decision: "accept")
                    }
                }
                if approval.availableDecisions.contains("acceptForSession") {
                    Button("本会话允许") {
                        approvalViewModel.resolveApproval(id: approval.id, decision: "acceptForSession")
                    }
                    .buttonStyle(.borderedProminent)
                }
            }
            .controlSize(.small)
            .disabled(approvalViewModel.isPerformingAction)
        }
        .padding(13)
    }

    private func retryContent(_ activity: PetActivity) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            if let detail = displayText(activity.detail) {
                ScrollView {
                    Text(detail)
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(minHeight: 44, maxHeight: .infinity)
                .layoutPriority(1)
            }

            if canRetry(activity) {
                Text("补充重试要求（可选）")
                    .font(.system(size: 11, weight: .medium))
                TextEditor(text: $retryInstruction)
                    .font(.system(size: 12))
                    .scrollContentBackground(.hidden)
                    .padding(5)
                    .frame(height: 72)
                    .background(Color(nsColor: .textBackgroundColor).opacity(0.78), in: RoundedRectangle(cornerRadius: 8))
                    .overlay { RoundedRectangle(cornerRadius: 8).stroke(.separator) }
            } else {
                Text("当前通知缺少直接重试所需的运行信息，请打开任务详情处理。")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }

            if let actionMessage {
                Text(actionMessage)
                    .font(.system(size: 11))
                    .foregroundStyle(actionSucceeded ? Color.green : Color.red)
                    .lineLimit(2)
            }

            HStack {
                Button("忽略") {
                    interactionState.isMessageExpanded = false
                    store.dismiss(activity, disposition: .ignored)
                }
                Spacer()
                Button(canLoadTask(activity)
                       ? model.localized("查看执行过程", english: "View Execution Process")
                       : model.localized("打开详情", english: "Open Details")) {
                    if canLoadTask(activity) {
                        showTaskProcess(activity)
                    } else {
                        onOpen(activity)
                    }
                }
                if canRetry(activity) {
                    Button {
                        retry(activity)
                    } label: {
                        if isRetrying {
                            ProgressView().controlSize(.small)
                        } else {
                            Label("重新处理", systemImage: "arrow.clockwise")
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(isRetrying)
                }
            }
            .controlSize(.small)
        }
        .padding(13)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private func genericContent(_ activity: PetActivity) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            if let detail = displayText(activity.detail) {
                ScrollView {
                    Text(detail)
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                Text(genericMessage(for: activity))
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }
            HStack {
                if activity.kind == .succeeded || activity.kind == .cancelled {
                    Button("知道了") {
                        interactionState.isMessageExpanded = false
                        store.dismiss(activity, disposition: .acknowledged)
                    }
                }
                Spacer()
                if (activity.kind == .working || activity.kind == .reviewing), canCancel(activity) {
                    Button(role: .destructive) {
                        cancel(activity)
                    } label: {
                        if cancellingActivityIDs.contains(activity.id) {
                            ProgressView()
                                .controlSize(.small)
                        } else {
                            Text("取消任务")
                        }
                    }
                    .buttonStyle(.bordered)
                    .tint(.red)
                    .disabled(cancellingActivityIDs.contains(activity.id))
                }
                if shouldShowGenericDetailButton(activity) {
                    Button(activity.kind == .waitingForUser
                           ? model.localized("打开并填写", english: "Open and Reply")
                           : (canLoadTask(activity)
                              ? model.localized("查看执行过程", english: "View Execution Process")
                              : model.localized("打开详情", english: "Open Details"))) {
                        if canLoadTask(activity) {
                            showTaskProcess(activity)
                        } else {
                            onOpen(activity)
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                }
            }
            .controlSize(.small)
        }
        .padding(13)
    }

    private func shouldShowGenericDetailButton(_ activity: PetActivity) -> Bool {
        if canLoadTask(activity) {
            return true
        }
        if activity.kind == .succeeded, activity.source == .chat {
            return displayText(activity.detail) == nil
        }
        return true
    }

    private func runningActivitiesSection(
        _ activities: [PetActivity],
        showsHeader: Bool
    ) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            if showsHeader {
                HStack {
                    Label("正在执行", systemImage: "rectangle.stack.fill")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(.indigo)
                    Spacer()
                    Text("\(activities.count) 项")
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
            }

            ScrollView {
                VStack(spacing: 6) {
                    ForEach(activities) { activity in
                        HStack(spacing: 9) {
                            ProgressView()
                                .controlSize(.small)
                                .tint(.indigo)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(displayTitle(for: activity))
                                    .font(.system(size: 11, weight: .medium))
                                    .lineLimit(1)
                                HStack(spacing: 3) {
                                    if isStale(activity) {
                                        Image(systemName: "clock.badge.exclamationmark")
                                        Text("长时间未更新")
                                    } else {
                                        Text("更新于")
                                        Text(activity.updatedAt, style: .relative)
                                    }
                                }
                                .font(.system(size: 10))
                                .foregroundStyle(isStale(activity) ? Color.orange : Color.secondary)
                                if let error = cancellationErrors[activity.id] {
                                    Text(error)
                                        .font(.system(size: 10))
                                        .foregroundStyle(.red)
                                        .lineLimit(2)
                                }
                            }
                            Spacer()
                            Button("查看") { showTaskProcess(activity) }
                                .controlSize(.mini)
                            if canCancel(activity) {
                                Button(role: .destructive) {
                                    cancel(activity)
                                } label: {
                                    if cancellingActivityIDs.contains(activity.id) {
                                        ProgressView()
                                            .controlSize(.mini)
                                    } else {
                                        Text("取消")
                                    }
                                }
                                .controlSize(.mini)
                                .disabled(cancellingActivityIDs.contains(activity.id))
                            }
                        }
                        .padding(8)
                        .background(
                            Color(nsColor: .controlBackgroundColor).opacity(0.58),
                            in: RoundedRectangle(cornerRadius: 9)
                        )
                    }
                }
            }
            .frame(maxHeight: 145)
        }
        .padding(12)
    }

    private func retry(_ activity: PetActivity) {
        guard !isRetrying else { return }
        isRetrying = true
        actionMessage = nil
        actionSucceeded = false
        Task {
            do {
                try await onRetry(activity, retryInstruction)
                retryInstruction = ""
                actionMessage = model.localized("已提交重新处理", english: "Retry submitted")
                actionSucceeded = true
                store.dismiss(activity, disposition: .handled)
            } catch {
                actionMessage = error.localizedDescription
                actionSucceeded = false
            }
            isRetrying = false
        }
    }

    private func cancel(_ activity: PetActivity) {
        guard !cancellingActivityIDs.contains(activity.id) else { return }
        cancellingActivityIDs.insert(activity.id)
        cancellationErrors[activity.id] = nil
        Task {
            do {
                try await onCancel(activity)
                store.dismiss(activity, disposition: .handled)
            } catch {
                cancellationErrors[activity.id] = error.localizedDescription
            }
            cancellingActivityIDs.remove(activity.id)
        }
    }

    private func approval(for activity: PetActivity) -> LocalConnectorPendingApproval? {
        guard activity.source == .localApproval else { return nil }
        let prefix = "local-approval:"
        let approvalID = activity.id.hasPrefix(prefix)
            ? String(activity.id.dropFirst(prefix.count))
            : activity.id
        return approvalViewModel.pendingApprovals.first(where: { $0.id == approvalID })
    }

    private func canRetry(_ activity: PetActivity) -> Bool {
        displayText(activity.route.messageID) != nil && displayText(activity.route.runID) != nil
    }

    private func canLoadTask(_ activity: PetActivity) -> Bool {
        displayText(activity.route.messageID) != nil
            && displayText(activity.route.taskID) != nil
    }

    private func showTaskProcess(_ activity: PetActivity) {
        guard canLoadTask(activity) else {
            onOpen(activity)
            return
        }
        interactionState.selectedActivityID = activity.id
        interactionState.inspectedTaskActivity = activity
        interactionState.isMessageExpanded = true
    }

    private func canCancel(_ activity: PetActivity) -> Bool {
        guard activity.kind == .working || activity.kind == .reviewing else { return false }
        if displayText(activity.route.messageID) != nil,
           displayText(activity.route.taskID) != nil {
            return true
        }
        if activity.source == .chat {
            return displayText(activity.route.conversationID) != nil
                && displayText(activity.route.turnID) != nil
        }
        if activity.source == .projectExecution {
            return displayText(activity.route.conversationID) != nil
                && displayText(activity.route.runID ?? activity.route.turnID) != nil
        }
        return false
    }

    private func runningActivities(excluding activityID: String) -> [PetActivity] {
        runningActivities().filter { $0.id != activityID }
    }

    private func runningActivities() -> [PetActivity] {
        store.activities.filter {
            $0.kind == .working || $0.kind == .reviewing
        }
    }

    private func attentionActivities(excluding activityID: String) -> [PetActivity] {
        store.activities.filter {
            $0.id != activityID
                && ($0.kind == .waitingForApproval || $0.kind == .waitingForUser)
        }
    }

    private func completedTaskActivities(excluding activityID: String) -> [PetActivity] {
        store.activities.filter {
            $0.id != activityID
                && $0.kind == .succeeded
                && ($0.source == .taskRunner || $0.source == .taskBoard)
        }
    }

    private func shouldShowActiveWorkSummary(for activity: PetActivity) -> Bool {
        guard store.presentation.activeWorkCount > 0 else { return false }
        if activity.kind == .working || activity.kind == .reviewing {
            return store.presentation.activeWorkCount > 1
        }
        return true
    }

    private func isStale(_ activity: PetActivity) -> Bool {
        Date().timeIntervalSince(activity.updatedAt) > 10 * 60
    }

    private func compactHint(for activity: PetActivity) -> String {
        switch activity.kind {
        case .waitingForApproval: model.localized("点击查看命令并审批", english: "Review the command and decide")
        case .waitingForUser: model.localized("点击查看需要填写的内容", english: "View the requested input")
        case .failed, .blocked: model.localized("点击查看并重新处理", english: "Review and retry")
        case .working, .reviewing: model.localized("点击查看执行详情", english: "View execution details")
        case .succeeded, .cancelled: model.localized("点击查看结果", english: "View result")
        }
    }

    private func expandedSubtitle(for activity: PetActivity) -> String {
        switch activity.kind {
        case .waitingForApproval: model.localized("可直接在此完成审批", english: "Approve or decline here")
        case .waitingForUser: model.localized("任务正在等待你的答复", english: "The task is waiting for your reply")
        case .failed, .blocked: model.localized("检查原因并重新处理", english: "Review the cause and retry")
        case .working, .reviewing: model.localized("实时执行状态", english: "Live execution status")
        case .succeeded: model.localized("执行结果", english: "Execution result")
        case .cancelled: model.localized("任务状态", english: "Task status")
        }
    }

    private func expandedPanelTitle(for activity: PetActivity) -> String {
        if activity.kind == .working || activity.kind == .reviewing {
            return model.localized("任务动态", english: "Task Activity")
        }
        return displayTitle(for: activity)
    }

    private func expandedPanelSubtitle(for activity: PetActivity) -> String {
        if activity.kind == .working || activity.kind == .reviewing {
            let count = max(1, store.presentation.activeWorkCount)
            return model.localized("\(count) 项任务正在执行", english: "\(count) tasks running")
        }
        return expandedSubtitle(for: activity)
    }

    private func genericMessage(for activity: PetActivity) -> String {
        switch activity.kind {
        case .waitingForUser: model.localized("打开对应输入表单后即可继续任务。", english: "Open the input form to continue the task.")
        case .working, .reviewing: model.localized("任务仍在执行，可以打开查看完整过程。", english: "The task is still running. Open it to view the full process.")
        case .succeeded: model.localized("任务已经完成，可以打开查看结果。", english: "The task is complete. Open it to view the result.")
        case .cancelled: model.localized("任务已经取消。", english: "The task was cancelled.")
        case .waitingForApproval: model.localized("打开审批详情进行处理。", english: "Open approval details to decide.")
        case .failed, .blocked: model.localized("打开任务详情进行处理。", english: "Open task details to resolve it.")
        }
    }

    private func riskLabel(_ risk: String) -> String {
        switch risk.lowercased() {
        case "high", "critical": model.localized("高风险", english: "High Risk")
        case "medium": model.localized("中风险", english: "Medium Risk")
        default: model.localized("低风险", english: "Low Risk")
        }
    }

    private func riskColor(_ risk: String) -> Color {
        switch risk.lowercased() {
        case "high", "critical": .red
        case "medium": .orange
        default: .green
        }
    }

    private func messageIcon(for kind: PetActivityKind) -> String {
        switch kind {
        case .waitingForApproval, .waitingForUser: "bell.badge.fill"
        case .failed, .blocked: "exclamationmark.triangle.fill"
        case .succeeded: "checkmark.seal.fill"
        case .reviewing: "eye.fill"
        case .working: "sparkles"
        case .cancelled: "xmark.circle.fill"
        }
    }

    private func messageTint(for kind: PetActivityKind) -> Color {
        switch kind {
        case .waitingForApproval, .waitingForUser: .orange
        case .failed, .blocked: .red
        case .succeeded: .green
        case .reviewing: .purple
        case .working: .accentColor
        case .cancelled: .secondary
        }
    }

    private func displayTitle(for activity: PetActivity) -> String {
        if let title = displayText(activity.title) {
            return title
        }
        return switch activity.kind {
        case .waitingForApproval: model.localized("有操作等待审批", english: "An Operation Needs Approval")
        case .waitingForUser: model.localized("AI 正在等待你的输入", english: "AI Is Waiting for Your Input")
        case .failed: model.localized("任务执行失败", english: "Task Failed")
        case .blocked: model.localized("任务执行被阻塞", english: "Task Blocked")
        case .succeeded: model.localized("任务已完成", english: "Task Completed")
        case .reviewing: model.localized("AI 正在检查结果", english: "AI Is Reviewing the Result")
        case .working: model.localized("AI 正在处理任务", english: "AI Is Working on the Task")
        case .cancelled: model.localized("任务已取消", english: "Task Cancelled")
        }
    }

    private func displayText(_ value: String?) -> String? {
        guard let value else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty,
              trimmed.unicodeScalars.contains(where: CharacterSet.alphanumerics.contains) else {
            return nil
        }
        return trimmed
    }
}

private struct PetTaskProcessInlineView: View {
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

private struct PetAskUserInlineView: View {
    @EnvironmentObject private var model: AppModel
    let activity: PetActivity
    let onLoadPrompt: (PetActivity) async throws -> AskUserPrompt
    let onSubmitPrompt: (AskUserPrompt, AskUserSubmission) async throws -> Void
    let onCancelPrompt: (AskUserPrompt) async throws -> Void
    let onResolved: () -> Void

    @State private var prompt: AskUserPrompt?
    @State private var values: [String: String] = [:]
    @State private var selection: Set<String> = []
    @State private var isLoading = true
    @State private var isSubmitting = false
    @State private var errorMessage: String?

    var body: some View {
        Group {
            if isLoading {
                VStack(spacing: 10) {
                    ProgressView()
                    Text("正在加载输入内容…")
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(20)
            } else if let prompt {
                promptContent(prompt)
            } else {
                VStack(alignment: .leading, spacing: 10) {
                    Label(errorMessage ?? model.localized("这个输入请求已结束。", english: "This input request has ended."), systemImage: "exclamationmark.triangle.fill")
                        .font(.system(size: 12))
                        .foregroundStyle(.orange)
                    HStack {
                        Spacer()
                        Button("重新加载") { load() }
                            .controlSize(.small)
                    }
                }
                .padding(13)
            }
        }
        .task(id: activity.id) {
            await loadPrompt()
        }
    }

    private func promptContent(_ prompt: AskUserPrompt) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    if !prompt.message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        Text(prompt.message)
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    ForEach(prompt.fields) { field in
                        VStack(alignment: .leading, spacing: 5) {
                            HStack(spacing: 3) {
                                Text(field.label)
                                    .font(.system(size: 11, weight: .medium))
                                if field.isRequired {
                                    Text("*").foregroundStyle(.red)
                                }
                            }
                            if let description = field.description,
                               !description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                                Text(description)
                                    .font(.system(size: 10))
                                    .foregroundStyle(.secondary)
                            }
                            fieldControl(field)
                        }
                    }

                    if let choice = prompt.choice {
                        choiceControl(choice)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            if let errorMessage {
                Text(errorMessage)
                    .font(.system(size: 11))
                    .foregroundStyle(.red)
                    .lineLimit(3)
            }

            Divider()
            HStack(spacing: 8) {
                if prompt.allowsCancel {
                    Button("取消请求", role: .destructive) { cancel(prompt) }
                }
                Spacer()
                Button {
                    submit(prompt)
                } label: {
                    if isSubmitting {
                        ProgressView().controlSize(.small)
                    } else {
                        Label("提交", systemImage: "checkmark")
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(!isValid(prompt) || isSubmitting)
            }
            .controlSize(.small)
        }
        .padding(13)
    }

    @ViewBuilder
    private func fieldControl(_ field: AskUserField) -> some View {
        if field.isSecret {
            SecureField(field.placeholder ?? "", text: binding(for: field.key))
                .textFieldStyle(.roundedBorder)
        } else if field.isMultiline {
            TextEditor(text: binding(for: field.key))
                .font(.system(size: 11))
                .scrollContentBackground(.hidden)
                .padding(4)
                .frame(minHeight: 62)
                .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 7))
                .overlay { RoundedRectangle(cornerRadius: 7).stroke(.separator) }
        } else {
            TextField(field.placeholder ?? "", text: binding(for: field.key))
                .textFieldStyle(.roundedBorder)
        }
    }

    private func choiceControl(_ choice: AskUserChoice) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(choice.allowsMultiple
                 ? model.localized("请选择（可多选）", english: "Select one or more")
                 : model.localized("请选择", english: "Select"))
                .font(.system(size: 11, weight: .medium))
            ForEach(choice.options) { option in
                let selected = selection.contains(option.value)
                Button {
                    toggle(option.value, in: choice)
                } label: {
                    HStack(alignment: .top, spacing: 7) {
                        Image(systemName: choice.allowsMultiple
                              ? (selected ? "checkmark.square.fill" : "square")
                              : (selected ? "circle.inset.filled" : "circle"))
                            .foregroundStyle(selected ? Color.accentColor : Color.secondary)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(option.label)
                                .font(.system(size: 11, weight: .medium))
                            if let description = option.description {
                                Text(description)
                                    .font(.system(size: 10))
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                    }
                    .padding(7)
                    .background(
                        selected ? Color.accentColor.opacity(0.09) : Color(nsColor: .controlBackgroundColor),
                        in: RoundedRectangle(cornerRadius: 8)
                    )
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func load() {
        Task { await loadPrompt() }
    }

    private func loadPrompt() async {
        isLoading = true
        errorMessage = nil
        do {
            let loaded = try await onLoadPrompt(activity)
            guard !Task.isCancelled else { return }
            prompt = loaded
            values = Dictionary(uniqueKeysWithValues: loaded.fields.map { ($0.key, $0.defaultValue) })
            selection = Set(loaded.choice?.defaultSelection ?? [])
        } catch {
            guard !Task.isCancelled else { return }
            prompt = nil
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    private func submit(_ prompt: AskUserPrompt) {
        guard !isSubmitting, isValid(prompt) else { return }
        isSubmitting = true
        errorMessage = nil
        Task {
            do {
                try await onSubmitPrompt(prompt, submission(prompt))
                clearSecrets(prompt)
                onResolved()
            } catch {
                errorMessage = error.localizedDescription
            }
            isSubmitting = false
        }
    }

    private func cancel(_ prompt: AskUserPrompt) {
        guard !isSubmitting else { return }
        isSubmitting = true
        errorMessage = nil
        Task {
            do {
                try await onCancelPrompt(prompt)
                clearSecrets(prompt)
                onResolved()
            } catch {
                errorMessage = error.localizedDescription
            }
            isSubmitting = false
        }
    }

    private func submission(_ prompt: AskUserPrompt) -> AskUserSubmission {
        let selectedValues = prompt.choice?.options.map(\.value).filter(selection.contains) ?? []
        let answer: AskUserSelection?
        if let choice = prompt.choice {
            answer = choice.allowsMultiple
                ? .multiple(selectedValues)
                : .single(selectedValues.first ?? "")
        } else {
            answer = nil
        }
        return AskUserSubmission(values: values, selection: answer)
    }

    private func isValid(_ prompt: AskUserPrompt) -> Bool {
        for field in prompt.fields where field.isRequired {
            if values[field.key, default: ""].trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                return false
            }
        }
        if let choice = prompt.choice {
            return selection.count >= choice.minimumSelectionCount
                && selection.count <= choice.maximumSelectionCount
        }
        return true
    }

    private func binding(for key: String) -> Binding<String> {
        Binding(
            get: { values[key, default: ""] },
            set: { values[key] = $0 }
        )
    }

    private func toggle(_ value: String, in choice: AskUserChoice) {
        if !choice.allowsMultiple {
            selection = [value]
        } else if selection.contains(value) {
            selection.remove(value)
        } else if selection.count < choice.maximumSelectionCount {
            selection.insert(value)
        }
    }

    private func clearSecrets(_ prompt: AskUserPrompt) {
        for field in prompt.fields where field.isSecret {
            values[field.key] = ""
        }
    }
}
