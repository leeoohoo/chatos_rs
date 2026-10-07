import ChatOSCore
import SwiftUI

extension PetMessageView {
    func attentionActivitiesSection(_ activities: [PetActivity]) -> some View {
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

    func approvalContent(_ approval: LocalConnectorPendingApproval) -> some View {
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

    func retryContent(_ activity: PetActivity) -> some View {
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

    func genericContent(_ activity: PetActivity) -> some View {
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

    func shouldShowGenericDetailButton(_ activity: PetActivity) -> Bool {
        if canLoadTask(activity) {
            return true
        }
        if activity.kind == .succeeded, activity.source == .chat {
            return PetActivityPresentation.displayText(activity.detail) == nil
        }
        return true
    }

    func runningActivitiesSection(
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

    func retry(_ activity: PetActivity) {
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

    func cancel(_ activity: PetActivity) {
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

    func approval(for activity: PetActivity) -> LocalConnectorPendingApproval? {
        guard activity.source == .localApproval else { return nil }
        let prefix = "local-approval:"
        let approvalID = activity.id.hasPrefix(prefix)
            ? String(activity.id.dropFirst(prefix.count))
            : activity.id
        return approvalViewModel.pendingApprovals.first(where: { $0.id == approvalID })
    }

    func canRetry(_ activity: PetActivity) -> Bool {
        PetActivityPresentation.displayText(activity.route.messageID) != nil && PetActivityPresentation.displayText(activity.route.runID) != nil
    }

    func canLoadTask(_ activity: PetActivity) -> Bool {
        PetActivityPresentation.displayText(activity.route.messageID) != nil
            && PetActivityPresentation.displayText(activity.route.taskID) != nil
    }

    func showTaskProcess(_ activity: PetActivity) {
        guard canLoadTask(activity) else {
            onOpen(activity)
            return
        }
        interactionState.selectedActivityID = activity.id
        interactionState.inspectedTaskActivity = activity
        interactionState.isMessageExpanded = true
    }

    func canCancel(_ activity: PetActivity) -> Bool {
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

    func runningActivities() -> [PetActivity] {
        store.activities.filter {
            $0.kind == .working
        }
    }

    func attentionActivities(excluding activityID: String) -> [PetActivity] {
        store.activities.filter {
            $0.id != activityID
                && ($0.kind == .waitingForApproval || $0.kind == .waitingForUser)
        }
    }

    func completedTaskActivities(excluding activityID: String) -> [PetActivity] {
        store.activities.filter {
            $0.id != activityID
                && $0.kind == .succeeded
                && ($0.source == .taskExecution || $0.source == .taskBoard)
        }
    }

    func shouldShowActiveWorkSummary(for activity: PetActivity) -> Bool {
        guard store.presentation.activeWorkCount > 0 else { return false }
        if activity.kind == .working {
            return store.presentation.activeWorkCount > 1
        }
        return true
    }

    func isStale(_ activity: PetActivity) -> Bool {
        Date().timeIntervalSince(activity.updatedAt) > 10 * 60
    }

    func compactHint(for activity: PetActivity) -> String {
        switch activity.kind {
        case .waitingForApproval: model.localized("点击查看命令并审批", english: "Review the command and decide")
        case .waitingForUser: model.localized("点击查看需要填写的内容", english: "View the requested input")
        case .failed, .blocked: model.localized("点击查看并重新处理", english: "Review and retry")
        case .working: model.localized("点击查看执行详情", english: "View execution details")
        case .reviewing: model.localized("任务已暂停，需要检查后处理", english: "The task is paused and needs review")
        case .succeeded, .cancelled: model.localized("点击查看结果", english: "View result")
        }
    }

    func expandedSubtitle(for activity: PetActivity) -> String {
        switch activity.kind {
        case .waitingForApproval: model.localized("可直接在此完成审批", english: "Approve or decline here")
        case .waitingForUser: model.localized("任务正在等待你的答复", english: "The task is waiting for your reply")
        case .failed, .blocked: model.localized("检查原因并重新处理", english: "Review the cause and retry")
        case .working: model.localized("实时执行状态", english: "Live execution status")
        case .reviewing: model.localized("任务已暂停，需要检查或取消", english: "The task is paused; review or cancel it")
        case .succeeded: model.localized("执行结果", english: "Execution result")
        case .cancelled: model.localized("任务状态", english: "Task status")
        }
    }

    func expandedPanelTitle(for activity: PetActivity) -> String {
        if activity.kind == .working {
            return model.localized("任务动态", english: "Task Activity")
        }
        return PetActivityPresentation.displayTitle(for: activity, model: model)
    }

    func expandedPanelSubtitle(for activity: PetActivity) -> String {
        if activity.kind == .working {
            let count = max(1, store.presentation.activeWorkCount)
            return model.localized("\(count) 项任务正在执行", english: "\(count) tasks running")
        }
        return expandedSubtitle(for: activity)
    }

}
