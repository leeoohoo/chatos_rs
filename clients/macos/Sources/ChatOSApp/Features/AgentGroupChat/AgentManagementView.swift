import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import SwiftUI

enum AgentProfileEditorTarget: Identifiable {
    case create
    case edit(LocalAgentProfile)

    var id: String {
        switch self {
        case .create: "new-agent"
        case let .edit(profile): profile.id
        }
    }
}
struct AgentManagementView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: AgentGroupChatWorkspaceViewModel
    let ownerUserID: String
    let skillLibrary: LocalAgentSkillLibrary
    let openDirect: (LocalAgentProfile) -> Void
    @State private var editorTarget: AgentProfileEditorTarget?
    @State private var showsSkillManager = false
    @State private var skillRevision = 0
    @State private var runToAbandon: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation?

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Agent 管理")
                        .font(.title3.weight(.semibold))
                    Text("管理 Agent、模型和权限。")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("Skill 管理", systemImage: "books.vertical") {
                    showsSkillManager = true
                }
                .buttonStyle(.bordered)
                Button {
                    openEditor(.create)
                } label: {
                    if viewModel.isLoadingModels {
                        ProgressView().controlSize(.small)
                    } else {
                        Label("创建 Agent", systemImage: "person.badge.plus")
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(viewModel.isLoadingModels)
            }
            .padding(18)

            Divider()

            if viewModel.agents.isEmpty {
                ContentUnavailableView {
                    Label("还没有 Agent", systemImage: "person.crop.rectangle.stack")
                } description: {
                    Text("创建后可以直接私聊，也可以加入项目团队。")
                } actions: {
                    Button {
                        openEditor(.create)
                    } label: {
                        if viewModel.isLoadingModels {
                            ProgressView().controlSize(.small)
                        } else {
                            Text("创建 Agent")
                        }
                    }
                        .buttonStyle(.borderedProminent)
                        .disabled(viewModel.isLoadingModels)
                }
            } else {
                VStack(spacing: 0) {
                    ScrollView {
                        LazyVGrid(
                            columns: [GridItem(.adaptive(minimum: 280, maximum: 420), spacing: 14)],
                            alignment: .leading,
                            spacing: 14
                        ) {
                            ForEach(viewModel.agents) { agent in
                                agentCard(agent)
                            }
                        }
                        .padding(18)
                    }
                    .frame(maxHeight: viewModel.selectedAgentID == nil ? .infinity : 320)

                    if viewModel.selectedAgentID != nil {
                        Spacer(minLength: 24)
                        Divider()
                        triggerRunsPanel
                            .frame(height: 340)
                    } else {
                        Spacer(minLength: 0)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .sheet(item: $editorTarget) { target in
            AgentProfileEditorSheet(
                viewModel: viewModel,
                target: target,
                professions: professions
            )
        }
        .sheet(isPresented: $showsSkillManager) {
            AgentSkillManagementSheet(
                ownerUserID: ownerUserID,
                skillLibrary: skillLibrary,
                initialLanguage: model.contextLanguage,
                onLanguageChange: { model.contextLanguage = $0 }
            )
        }
        .onReceive(NotificationCenter.default.publisher(for: .agentSkillLibraryDidChange)) { _ in
            skillRevision += 1
        }
        .onReceive(NotificationCenter.default.publisher(for: .agentGroupChatRoomsDidChange)) { _ in
            Task { await viewModel.loadTriggerRuns() }
        }
        .confirmationDialog(
            "结束这次运行？",
            isPresented: Binding(
                get: { runToAbandon != nil },
                set: { if !$0 { runToAbandon = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("结束运行", role: .destructive) {
                guard let item = runToAbandon,
                      let deliveryID = item.delivery?.id else { return }
                runToAbandon = nil
                Task {
                    await viewModel.abandonRun(
                        deliveryID: deliveryID,
                        projectID: item.run.context.projectID
                    )
                }
            }
            Button("取消", role: .cancel) { runToAbandon = nil }
        } message: {
            Text("运行会标记为失败并释放 Agent 队列；检查点和事件记录仍会保留。")
        }
    }

    private func agentCard(_ agent: LocalAgentProfile) -> some View {
        let canManageStaff = LocalAgentPermission.canManageStaff(agent.draft.defaultSkillIDs)
        let canAccessLocalProjects = LocalAgentPermission.canAccessLocalProjects(
            agent.draft.defaultSkillIDs
        )
        return VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: canManageStaff ? "person.crop.circle.badge.checkmark" : "person.crop.circle")
                    .font(.title2)
                    .foregroundStyle(canManageStaff ? Color.accentColor : .secondary)
                VStack(alignment: .leading, spacing: 3) {
                    Text(agent.draft.name)
                        .font(.headline)
                    Text(canManageStaff ? "可招募和解雇成员" : "无人员管理权限")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            if !agent.draft.description.isEmpty {
                Text(agent.draft.description)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            HStack(spacing: 10) {
                Button("私聊", systemImage: "bubble.left.and.bubble.right") {
                    openDirect(agent)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
                .fixedSize()
                Spacer(minLength: 0)
                Button("编辑") { openEditor(.edit(agent)) }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .fixedSize()
                    .disabled(viewModel.isLoadingModels)
            }
            Divider()
            LabeledContent("模型") {
                Text(modelName(agent.draft.modelConfigID))
                    .lineLimit(1)
            }
            .font(.caption)
            LabeledContent("思考等级") {
                Text(agent.draft.thinkingLevel ?? "跟随模型默认")
            }
            .font(.caption)
            LabeledContent("职业") {
                Text(professions.first(where: { $0.key == agent.draft.professionKey })?.label
                    ?? agent.draft.professionKey)
            }
            .font(.caption)
            LabeledContent("工具与 Plugin") {
                Text("按任务自主发现")
            }
            .font(.caption)
            LabeledContent("主动巡检") {
                Text(
                    agent.draft.heartbeatEnabled
                        ? Self.heartbeatIntervalLabel(agent.draft.heartbeatIntervalSeconds)
                        : "关闭"
                )
            }
            .font(.caption)
            if canManageStaff || canAccessLocalProjects {
                HStack(spacing: 6) {
                    if canManageStaff {
                        Label("人员管理", systemImage: "person.2.badge.gearshape")
                    }
                    if canAccessLocalProjects {
                        Label("项目与团队", systemImage: "folder.badge.gearshape")
                    }
                }
                .font(.caption2)
                .foregroundStyle(.secondary)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(
                    viewModel.selectedAgentID == agent.id
                        ? Color.accentColor : Color.primary.opacity(0.08),
                    lineWidth: viewModel.selectedAgentID == agent.id ? 2 : 1
                )
        }
        .contentShape(RoundedRectangle(cornerRadius: 12))
        .onTapGesture {
            Task { await viewModel.selectAgent(agent.id) }
        }
    }

    @ViewBuilder
    private var triggerRunsPanel: some View {
        if let selectedAgentID = viewModel.selectedAgentID,
           let agent = viewModel.agents.first(where: { $0.id == selectedAgentID }) {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(agent.draft.name) · 运行动态")
                            .font(.headline)
                        Text("最近的消息处理、主动巡检和任务执行")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if viewModel.isLoadingTriggerRuns {
                        ProgressView().controlSize(.small)
                    }
                    Button("刷新", systemImage: "arrow.clockwise") {
                        Task { await viewModel.loadTriggerRuns() }
                    }
                    .buttonStyle(.borderless)
                }

                HStack(spacing: 10) {
                    laneOverviewCard(
                        lane: .manager,
                        title: "沟通层",
                        subtitle: "处理消息、记忆与任务调度",
                        systemImage: "bubble.left.and.bubble.right"
                    )
                    laneOverviewCard(
                        lane: .executor,
                        title: "执行层",
                        subtitle: "执行 Todo、工具与 Plugin",
                        systemImage: "hammer"
                    )
                }

                if viewModel.triggerRuns.isEmpty, !viewModel.isLoadingTriggerRuns {
                    ContentUnavailableView(
                        "还没有运行记录",
                        systemImage: "bolt.horizontal.circle",
                        description: Text("Agent 开始处理消息或任务后，会显示在这里。")
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ScrollView {
                        LazyVStack(spacing: 10) {
                            ForEach(viewModel.triggerRuns) { item in
                                triggerRunCard(item)
                            }
                        }
                    }
                }
            }
            .padding(18)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        } else {
            ContentUnavailableView(
                "选择一个 Agent",
                systemImage: "person.crop.circle.badge.questionmark",
                description: Text("点击上方 Agent 卡片查看它的运行动态。")
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func triggerRunCard(
        _ item: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation
    ) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 18) {
                    runFact("运行通道", laneLabel(item.run.context.lane))
                    runFact("状态", runStatusLabel(item.run.checkpoint.status))
                    runFact("唤醒来源", triggerLabel(item))
                    runFact("耗时", durationLabel(item.run.checkpoint.elapsedSeconds))
                    runFact("模型请求", "\(item.run.checkpoint.modelCalls) 次")
                    Spacer(minLength: 0)
                }
                if let content = item.triggerMessage?.content,
                   !content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    GroupBox("收到的内容") {
                        Text(content)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .lineLimit(3)
                            .textSelection(.enabled)
                    }
                }
                if let reason = displayedStopReason(item) {
                    Label(userFacingStopReason(reason), systemImage: "exclamationmark.circle")
                        .font(.callout)
                        .foregroundStyle(triggerColor(item.run.checkpoint.status))
                        .textSelection(.enabled)
                }
                if let result = item.run.checkpoint.result ?? item.run.checkpoint.completionResult,
                   !result.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    GroupBox("处理结果") {
                        Text(LocalizedStringKey(result))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .lineLimit(5)
                            .textSelection(.enabled)
                    }
                }
                if !item.run.events.isEmpty {
                    DisclosureGroup("技术诊断 · \(item.run.events.count) 条") {
                        VStack(alignment: .leading, spacing: 8) {
                            ForEach(item.run.events.suffix(12)) { event in
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(diagnosticEventLabel(event.kind))
                                        .font(.caption.weight(.medium))
                                    Text(event.detail)
                                        .font(.caption2.monospaced())
                                        .foregroundStyle(.secondary)
                                        .lineLimit(3)
                                        .textSelection(.enabled)
                                }
                            }
                        }
                        .padding(.top, 6)
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                }
                .padding(.top, 8)
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: triggerIcon(item.delivery?.triggerKind))
                        .foregroundStyle(triggerColor(item.run.checkpoint.status))
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(laneLabel(item.run.context.lane)) · \(runStatusLabel(item.run.checkpoint.status))")
                            .font(.callout.weight(.medium))
                        Text("\(triggerLabel(item)) · \(item.room?.draft.name ?? "来源会话已移除") · \(formattedTime(item.run.updatedAtUnixMs))")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text(runSummary(item))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
            if isActionable(item) {
                Divider()
                HStack(spacing: 8) {
                    if let reason = displayedStopReason(item) {
                        Text(userFacingStopReason(reason))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                    Spacer()
                    if let deliveryID = item.delivery?.id,
                       viewModel.runActionDeliveryIDs.contains(deliveryID) {
                        ProgressView().controlSize(.small)
                    } else if let deliveryID = item.delivery?.id {
                        Button(item.run.checkpoint.status == .needsReview
                               ? "重试中断步骤" : "继续运行") {
                            Task {
                                if item.run.checkpoint.status == .needsReview {
                                    await viewModel.retryInterruptedRun(
                                        deliveryID: deliveryID,
                                        projectID: item.run.context.projectID
                                    )
                                } else {
                                    await viewModel.resumeRun(
                                        deliveryID: deliveryID,
                                        projectID: item.run.context.projectID
                                    )
                                }
                            }
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        Button("结束", role: .destructive) {
                            runToAbandon = item
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                    }
                }
            }
        }
        .padding(12)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
    }

    private func triggerLabel(
        _ item: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation
    ) -> String {
        switch item.delivery?.triggerKind {
        case .mention:
            item.room?.conversationKind.isDirect == true ? "私聊消息" : "群聊提及"
        case .defaultAgent:
            switch item.room?.conversationKind {
            case .humanAgentDirect: "私聊消息"
            case .agentAgentDirect: "Agent 私聊"
            case .projectTeam: "群聊消息"
            case nil: "消息唤醒"
            }
        case .agentMention:
            item.room?.conversationKind.isDirect == true ? "Agent 私聊" : "Agent 提及"
        case .heartbeat: "主动巡检"
        case .todo: "Todo 执行"
        case .todoStatus: "Todo 状态"
        case nil: "系统唤醒"
        }
    }

    private func laneLabel(_ lane: LocalAgentRunLane) -> String {
        switch lane {
        case .manager: "沟通层"
        case .executor: "执行层"
        }
    }

    private func laneOverviewCard(
        lane: LocalAgentRunLane,
        title: String,
        subtitle: String,
        systemImage: String
    ) -> some View {
        let latest = viewModel.triggerRuns.first(where: { $0.run.context.lane == lane })
        return HStack(spacing: 10) {
            Image(systemName: systemImage)
                .font(.title3)
                .foregroundStyle(latest.map { triggerColor($0.run.checkpoint.status) } ?? .secondary)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(title).font(.callout.weight(.semibold))
                    Text(latest.map { runStatusLabel($0.run.checkpoint.status) } ?? "暂无运行")
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.secondary)
                }
                Text(latest.map {
                    "\(subtitle) · \(formattedTime($0.run.updatedAtUnixMs))"
                } ?? subtitle)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 9))
        .overlay {
            RoundedRectangle(cornerRadius: 9)
                .stroke(Color.primary.opacity(0.07), lineWidth: 1)
        }
    }

    private func runSummary(
        _ item: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation
    ) -> String {
        if let result = item.run.checkpoint.result ?? item.run.checkpoint.completionResult,
           !result.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return "已有处理结果"
        }
        switch item.run.checkpoint.status {
        case .running: return "正在处理"
        case .paused, .needsReview, .limitReached: return "等待处理"
        case .completed: return "处理完成"
        case .failed: return "未能完成"
        case .ready: return "等待开始"
        }
    }

    private func runFact(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.caption2).foregroundStyle(.tertiary)
            Text(value).font(.caption.weight(.medium))
        }
    }

    private func durationLabel(_ seconds: Double) -> String {
        guard seconds >= 1 else { return "不足 1 秒" }
        if seconds < 60 { return "\(Int(seconds)) 秒" }
        return "\(Int(seconds) / 60) 分 \(Int(seconds) % 60) 秒"
    }

    private func userFacingStopReason(_ reason: String) -> String {
        if reason.localizedCaseInsensitiveContains("no such column") {
            return "当时本地数据升级未完成，运行已中断；现在可以重试。"
        }
        if reason.localizedCaseInsensitiveContains("cancel") || reason.contains("取消") {
            return "这次运行已取消。"
        }
        if reason.localizedCaseInsensitiveContains("timeout") || reason.contains("超时") {
            return "处理时间较长，这次运行已暂停，可以稍后恢复。"
        }
        return reason
    }

    private func displayedStopReason(
        _ item: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation
    ) -> String? {
        if let reason = item.run.checkpoint.stopReason?.trimmingCharacters(
            in: .whitespacesAndNewlines
        ), !reason.isEmpty {
            return reason
        }
        return item.run.events.last(where: {
            $0.kind == "needs_review" || $0.kind == "resume_failed"
        })?.detail
    }

    private func diagnosticEventLabel(_ kind: String) -> String {
        switch kind {
        case "memory_connecting": "准备历史上下文"
        case "memory_connected", "memory_reconnected", "memory_bound": "历史上下文已就绪"
        case "memory_syncing": "保存运行记录"
        case "memory_synced": "运行记录已保存"
        case "model_request": "请求模型"
        case "model_response": "模型已响应"
        case "tool_started": "开始使用工具"
        case "tool_completed": "工具执行完成"
        case "tool_rejected": "工具未执行"
        case "needs_review": "运行中断"
        case "retry_authorized": "已确认重试"
        case "resume_failed": "恢复失败"
        case "memory_unavailable": "历史服务暂不可用"
        default: kind.replacingOccurrences(of: "_", with: " ")
        }
    }

    private func isActionable(
        _ item: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation
    ) -> Bool {
        guard item.delivery?.status == .running else { return false }
        return switch item.run.checkpoint.status {
        case .paused, .needsReview, .limitReached:
            true
        case .ready, .running, .completed, .failed:
            false
        }
    }

    private func triggerIcon(_ kind: ProjectAgentDeliveryTriggerKind?) -> String {
        switch kind {
        case .heartbeat: "heart.circle"
        case .todo, .todoStatus: "checklist"
        case .mention, .defaultAgent, .agentMention: "bubble.left.and.bubble.right"
        case nil: "bolt.horizontal.circle"
        }
    }

    private func runStatusLabel(_ status: AgentRunCheckpoint.Status) -> String {
        switch status {
        case .ready: "等待"
        case .running: "运行中"
        case .paused: "已暂停"
        case .needsReview: "运行中断"
        case .limitReached: "达到限制"
        case .completed: "已完成"
        case .failed: "失败"
        }
    }

    private func triggerColor(_ status: AgentRunCheckpoint.Status) -> Color {
        switch status {
        case .completed: .green
        case .running: .accentColor
        case .paused, .needsReview, .limitReached: .orange
        case .failed: .red
        case .ready: .secondary
        }
    }

    private func formattedTime(_ unixMs: Int64) -> String {
        Date(timeIntervalSince1970: Double(unixMs) / 1_000).formatted(
            date: .abbreviated,
            time: .shortened
        )
    }

    private static func heartbeatIntervalLabel(_ seconds: Int) -> String {
        switch seconds {
        case 60: "每分钟"
        case 300: "每 5 分钟"
        case 900: "每 15 分钟"
        case 1_800: "每 30 分钟"
        case 3_600: "每小时"
        default: "每 \(seconds / 60) 分钟"
        }
    }

    private func modelName(_ id: String) -> String {
        guard let model = viewModel.availableModels.first(where: { $0.id == id }) else {
            return "已配置"
        }
        return "\(model.name) · \(model.modelName)"
    }

    private func openEditor(_ target: AgentProfileEditorTarget) {
        Task {
            guard await viewModel.prepareAgentEditor() else { return }
            editorTarget = target
        }
    }

    private var professions: [LocalAgentProfessionDefinition] {
        _ = skillRevision
        return skillLibrary.professions(ownerUserID: ownerUserID)
    }
}
