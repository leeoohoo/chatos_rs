import ChatOSCore
import SwiftUI

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
    let activityScope: PetMessageActivityScope
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
        if let primaryActivity = scopedPrimaryActivity
            ?? interactionState.inspectedTaskActivity {
            let activity = interactionState.inspectedTaskActivity
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

    private var scopedPrimaryActivity: PetActivity? {
        switch activityScope {
        case .primary:
            return store.presentation.primaryActivity.flatMap {
                activityScope.contains($0) ? $0 : nil
            }
        case .running:
            return runningActivities().first
        }
    }

    private func compactCard(_ activity: PetActivity) -> some View {
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
                                Image(systemName: PetActivityPresentation.messageIcon(for: activity.kind))
                                    .foregroundStyle(PetActivityPresentation.messageTint(for: activity.kind))
                                Text(PetActivityPresentation.displayTitle(for: activity, model: model))
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
                Text(PetActivityPresentation.riskLabel(approval.risk, model: model))
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(PetActivityPresentation.riskColor(approval.risk))
                    .padding(.horizontal, 7)
                    .padding(.vertical, 3)
                    .background(PetActivityPresentation.riskColor(approval.risk).opacity(0.12), in: Capsule())
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
                    if let reason = PetActivityPresentation.displayText(approval.reason) {
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
            if let detail = PetActivityPresentation.displayText(activity.detail) {
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
            if let detail = PetActivityPresentation.displayText(activity.detail) {
                ScrollView {
                    Text(detail)
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                Text(PetActivityPresentation.genericMessage(for: activity, model: model))
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
            return PetActivityPresentation.displayText(activity.detail) == nil
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
                                Text(PetActivityPresentation.displayTitle(for: activity, model: model))
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
        PetActivityPresentation.displayText(activity.route.messageID) != nil && PetActivityPresentation.displayText(activity.route.runID) != nil
    }

    private func canLoadTask(_ activity: PetActivity) -> Bool {
        PetActivityPresentation.displayText(activity.route.messageID) != nil
            && PetActivityPresentation.displayText(activity.route.taskID) != nil
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
        if PetActivityPresentation.displayText(activity.route.messageID) != nil,
           PetActivityPresentation.displayText(activity.route.taskID) != nil {
            return true
        }
        if activity.source == .chat {
            return PetActivityPresentation.displayText(activity.route.conversationID) != nil
                && PetActivityPresentation.displayText(activity.route.turnID) != nil
        }
        return false
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
        return PetActivityPresentation.displayTitle(for: activity, model: model)
    }

    private func expandedPanelSubtitle(for activity: PetActivity) -> String {
        if activity.kind == .working || activity.kind == .reviewing {
            let count = max(1, store.presentation.activeWorkCount)
            return model.localized("\(count) 项任务正在执行", english: "\(count) tasks running")
        }
        return expandedSubtitle(for: activity)
    }

}
