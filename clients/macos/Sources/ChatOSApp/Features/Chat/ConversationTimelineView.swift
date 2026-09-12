import ChatOSCore
import SwiftUI

struct ConversationTimelineView: View {
    private static let bottomAnchorID = "conversation-timeline-bottom"
    private static let scrollCoordinateSpace = "conversation-timeline-scroll"

    @ObservedObject var conversation: ConversationSessionViewModel
    @EnvironmentObject private var model: AppModel
    let title: String
    var projectRootPath: String? = nil
    @State private var selectedProcessTurn: ConversationTurn?
    @State private var selectedTaskTurn: ConversationTurn?
    @State private var selectedTaskReply: TaskReplySelection?
    @State private var requestedTaskID: String?
    @State private var requestedRunID: String?
    @State private var hasPositionedInitialTimeline = false
    @State private var initialPositionTask: Task<Void, Never>?
    @State private var isPinnedToBottom = true

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            ConversationHistoryStatusView(conversation: conversation)
            timeline
                .workspaceFill()
            ComposerView(conversation: conversation)
                .padding(16)
        }
        .workspaceFill()
        .background(Color(nsColor: .textBackgroundColor))
        .sheet(item: $selectedProcessTurn) { turn in
            if let service = conversation.turnProcessService {
                TurnProcessSheet(turn: turn, service: service)
            }
        }
        .sheet(item: $selectedTaskTurn) { turn in
            if let graphService = conversation.messageTaskGraphService {
                MessageTaskWorkspaceSheet(
                    turn: turn,
                    graphService: graphService,
                    realtimeService: conversation.realtimeService,
                    initialTaskID: requestedTaskID,
                    initialRunID: requestedRunID
                )
            }
        }
    }

    private var header: some View {
        HStack {
            Text(title).appFont(.subheadline.weight(.semibold))
            Spacer()
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
    }

    private var timeline: some View {
        GeometryReader { viewport in
            ScrollViewReader { proxy in
                ZStack(alignment: .bottom) {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 0) {
                            if conversation.hasOlder {
                                if conversation.isLoadingOlder {
                                    ProgressView(model.localized(
                                        "正在加载更早消息…",
                                        english: "Loading earlier messages…"
                                    ))
                                        .controlSize(.small)
                                        .frame(maxWidth: .infinity)
                                } else {
                                    Button(
                                        model.localized("加载更早消息", english: "Load Earlier Messages"),
                                        systemImage: "arrow.up"
                                    ) {
                                        conversation.loadOlder()
                                    }
                                    .controlSize(.small)
                                    .frame(maxWidth: .infinity)
                                }
                            }

                            if conversation.turns.isEmpty {
                                ContentUnavailableView(
                                    model.localized(
                                        "开始一段新对话",
                                        english: "Start a new conversation"
                                    ),
                                    systemImage: "bubble.left.and.bubble.right",
                                    description: Text(model.localized(
                                        "这个会话还没有消息。",
                                        english: "This conversation has no messages yet."
                                    ))
                                )
                                .frame(maxWidth: .infinity, minHeight: 300)
                            }

                            ForEach(timelineItems) { item in
                                timelineRow(item)
                                    .padding(.top, item.spacingBefore)
                            }

                            Color.clear
                                .frame(height: 1)
                                .id(Self.bottomAnchorID)
                                .background {
                                    GeometryReader { bottom in
                                        Color.clear.preference(
                                            key: ConversationTimelineBottomPreferenceKey.self,
                                            value: bottom.frame(
                                                in: .named(Self.scrollCoordinateSpace)
                                            ).maxY
                                        )
                                    }
                                }
                        }
                        .padding(.horizontal, 28)
                        .padding(.vertical, 20)
                    }
                    .coordinateSpace(name: Self.scrollCoordinateSpace)
                    // Nested horizontal scroll views used by Markdown tables and code
                    // blocks can otherwise draw beyond the vertical timeline viewport
                    // on macOS, including across the conversation header and title bar.
                    .clipped()

                    if conversation.unreadNewerCount > 0 {
                        Button(
                            model.localized(
                                "\(conversation.unreadNewerCount) 条新任务动态",
                                english: "\(conversation.unreadNewerCount) new task updates"
                            ),
                            systemImage: "arrow.down"
                        ) {
                            conversation.markNewerContentRead()
                            scrollToBottom(using: proxy)
                        }
                        .buttonStyle(.borderedProminent)
                        .padding(.bottom, 10)
                    }

                    if selectedTaskReply != nil {
                        HStack {
                            Spacer()
                            Button(
                                model.localized("收起详情", english: "Collapse Details"),
                                systemImage: "chevron.up"
                            ) {
                                withAnimation(.easeInOut(duration: 0.18)) {
                                    selectedTaskReply = nil
                                }
                            }
                            .buttonStyle(.borderedProminent)
                            .tint(AppPalette.ai)
                            .help(model.localized(
                                "收起当前任务详情或执行过程",
                                english: "Collapse the current task details or execution process"
                            ))
                        }
                        .padding(.horizontal, 16)
                        .padding(.bottom, 10)
                    }
                }
                .onAppear {
                    positionInitialTimeline(using: proxy)
                    applyFocusRequest(using: proxy)
                }
                .onChange(of: timelineUpdateToken) {
                    guard hasPositionedInitialTimeline else {
                        positionInitialTimeline(using: proxy)
                        return
                    }
                    guard conversation.unreadNewerCount == 0 else { return }
                    scrollToBottomAfterLayout(using: proxy)
                }
                .onChange(of: conversation.selectedTurnID) { _, selectedTurnID in
                    guard let selectedTurnID else { return }
                    scrollToTurn(selectedTurnID, using: proxy)
                }
                .onChange(of: focusAvailabilityToken) {
                    applyFocusRequest(using: proxy)
                }
                .onPreferenceChange(ConversationTimelineBottomPreferenceKey.self) { bottomY in
                    let nextPinned = bottomY <= viewport.size.height + 28
                    guard nextPinned != isPinnedToBottom else { return }
                    isPinnedToBottom = nextPinned
                    conversation.setTimelinePinnedToBottom(nextPinned)
                }
                .onDisappear {
                    initialPositionTask?.cancel()
                }
            }
        }
    }

    private var timelineUpdateToken: String {
        guard let turn = conversation.turns.last else { return "empty" }
        let replyID = turn.assistantReplies.last?.id
            ?? turn.finalAssistantMessage?.id
            ?? "none"
        let taskToken = conversation.localAgentTasks.map {
            "\($0.id):\($0.run.version):\($0.lastAppliedEventSequence)"
        }.joined(separator: ",")
        return "\(turn.id)|\(turn.revision)|\(turn.assistantReplies.count)|\(replyID)|\(taskToken)"
    }

    private var timelineItems: [ConversationTimelineItem] {
        let promptsByTurnID = Dictionary(uniqueKeysWithValues: conversation.turns.map {
            ($0.id, conversation.prompts(for: $0.id))
        })
        let toolApprovalsByTurnID = Dictionary(uniqueKeysWithValues: conversation.turns.map {
            ($0.id, conversation.toolApprovals(for: $0.id))
        })
        let runControlsByTurnID = Dictionary(uniqueKeysWithValues: conversation.turns.map {
            ($0.id, conversation.runControls(for: $0.id))
        })
        let tasksByTurnID = Dictionary(uniqueKeysWithValues: conversation.turns.map {
            ($0.id, conversation.tasks(for: $0.id))
        })
        return ConversationTimelineItem.build(
            turns: conversation.turns,
            promptsByTurnID: promptsByTurnID,
            toolApprovalsByTurnID: toolApprovalsByTurnID,
            runControlsByTurnID: runControlsByTurnID,
            tasksByTurnID: tasksByTurnID,
            unattachedPrompts: conversation.unattachedPendingPrompts
        )
    }

    @ViewBuilder
    private func timelineRow(_ item: ConversationTimelineItem) -> some View {
        switch item {
        case let .user(turn, _):
            UserTurnMessageView(
                turn: turn,
                showsTaskGraph: conversation.hasTaskGraph(for: turn),
                onOpenProcess: { selectedProcessTurn = turn },
                onOpenTaskGraph: {
                    let task = conversation.tasks(for: turn.id).first
                    requestedTaskID = task?.task.taskID
                    requestedRunID = task?.run.runID
                    selectedTaskTurn = turn
                }
            )

        case let .reply(turn, reply):
            if reply.taskCallback != nil {
                VStack(alignment: .leading, spacing: 14) {
                    TaskAgentReplyView(
                        reply: reply,
                        expandedSection: expandedSection(for: reply),
                        projectRootPath: projectRootPath,
                        onToggleInspector: { section in
                            toggleTaskReply(turn, reply, section)
                        }
                    )
                    if let selection = expandedSelection(for: turn, reply: reply),
                       let taskGraphService = conversation.messageTaskGraphService {
                        TaskReplyInlineInspectorView(
                            selection: selection,
                            requestedSection: selection.initialSection,
                            service: taskGraphService
                        )
                        .padding(.leading, 30)
                        .transition(.opacity.combined(with: .move(edge: .top)))
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                AssistantReplyView(reply: reply, projectRootPath: projectRootPath)
            }

        case let .prompt(prompt):
            AskUserPromptCardView(conversation: conversation, prompt: prompt)
                .padding(.leading, 30)

        case let .toolApproval(approval):
            LocalAgentToolApprovalCardView(
                conversation: conversation,
                approval: approval
            )
            .padding(.leading, 30)

        case let .runControl(control):
            LocalAgentRunControlCardView(
                conversation: conversation,
                control: control
            )
            .padding(.leading, 30)

        case let .task(task):
            LocalAgentTaskCardView(state: task)
                .padding(.leading, 30)
        }
    }

    private func toggleTaskReply(
        _ turn: ConversationTurn,
        _ reply: ConversationAssistantReply,
        _ section: TaskReplyInspectorSection
    ) {
        withAnimation(.easeInOut(duration: 0.18)) {
            if selectedTaskReply?.reply.id == reply.id,
               selectedTaskReply?.initialSection == section {
                selectedTaskReply = nil
            } else {
                selectedTaskReply = TaskReplySelection(
                    turn: turn,
                    reply: reply,
                    initialSection: section
                )
            }
        }
    }

    private func expandedSelection(
        for turn: ConversationTurn,
        reply: ConversationAssistantReply
    ) -> TaskReplySelection? {
        guard selectedTaskReply?.reply.id == reply.id else { return nil }
        return TaskReplySelection(
            turn: turn,
            reply: reply,
            initialSection: selectedTaskReply?.initialSection ?? .detail
        )
    }

    private func expandedSection(
        for reply: ConversationAssistantReply
    ) -> TaskReplyInspectorSection? {
        guard selectedTaskReply?.reply.id == reply.id else { return nil }
        return selectedTaskReply?.initialSection
    }

    private func positionInitialTimeline(using proxy: ScrollViewProxy) {
        guard !hasPositionedInitialTimeline, !conversation.turns.isEmpty else { return }
        hasPositionedInitialTimeline = true
        initialPositionTask?.cancel()
        initialPositionTask = Task { @MainActor in
            await Task.yield()
            guard !Task.isCancelled else { return }
            proxy.scrollTo(Self.bottomAnchorID, anchor: .bottom)
        }
    }

    private func scrollToTurn(_ turnID: String, using proxy: ScrollViewProxy) {
        withAnimation(.easeOut(duration: 0.18)) {
            proxy.scrollTo(turnID, anchor: .bottom)
        }
    }

    private var focusAvailabilityToken: String {
        let requestID = conversation.focusRequest?.id.uuidString ?? "none"
        let turnIDs = conversation.turns.map(\.id).joined(separator: ",")
        let promptIDs = conversation.askUserPrompts.map(\.id).joined(separator: ",")
        return "\(requestID)|\(turnIDs)|\(promptIDs)|\(conversation.hasOlder)|\(conversation.isLoadingOlder)"
    }

    private func applyFocusRequest(using proxy: ScrollViewProxy) {
        guard let request = conversation.focusRequest else { return }

        if let promptID = request.promptID,
           timelineItems.contains(where: { $0.id == promptID }) {
            withAnimation(.easeOut(duration: 0.18)) {
                proxy.scrollTo(promptID, anchor: .center)
            }
            conversation.consumeFocusRequest(id: request.id)
            return
        }

        let targetTurn = request.turnID.flatMap { turnID in
            conversation.turns.first(where: { $0.id == turnID })
        } ?? request.taskID.flatMap { taskID in
            conversation.localAgentTasks
                .first(where: { $0.task.taskID == taskID })
                .flatMap { task in
                    conversation.turns.first(where: { $0.id == task.task.sourceTurnID })
                }
        }

        if let targetTurn {
            scrollToTurn(targetTurn.id, using: proxy)
            if request.taskID != nil || request.runID != nil {
                requestedTaskID = request.taskID
                requestedRunID = request.runID
                selectedTaskTurn = targetTurn
            }
            conversation.consumeFocusRequest(id: request.id)
            return
        }

        if conversation.hasOlder, !conversation.isLoadingOlder {
            conversation.loadOlder()
        } else if request.turnID == nil,
                  request.promptID == nil,
                  request.taskID == nil,
                  request.runID == nil {
            conversation.consumeFocusRequest(id: request.id)
        }
    }

    private func scrollToBottom(using proxy: ScrollViewProxy) {
        withAnimation(.easeOut(duration: 0.18)) {
            proxy.scrollTo(Self.bottomAnchorID, anchor: .bottom)
        }
    }

    private func scrollToBottomAfterLayout(using proxy: ScrollViewProxy) {
        initialPositionTask?.cancel()
        initialPositionTask = Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(80))
            guard !Task.isCancelled else { return }
            proxy.scrollTo(Self.bottomAnchorID, anchor: .bottom)
        }
    }
}

private struct ConversationTimelineBottomPreferenceKey: PreferenceKey {
    static let defaultValue = CGFloat.greatestFiniteMagnitude

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}
