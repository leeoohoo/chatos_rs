import ChatOSCore
import SwiftUI

struct PetQuickChatView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var interactionState: PetOverlayInteractionState
    let onInspectTaskReply: (TaskReplySelection, any MessageTaskGraphServicing) -> Void

    var body: some View {
        Group {
            if let selectedResource {
                PetQuickChatConversationView(
                    resource: selectedResource,
                    conversation: model.petConversation(for: selectedResource),
                    onBack: { interactionState.selectedQuickChatResourceID = nil },
                    onClose: close,
                    onInspectTaskReply: onInspectTaskReply
                )
            } else {
                resourceList
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 16))
        .overlay {
            RoundedRectangle(cornerRadius: 16)
                .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.28), radius: 18, y: 9)
    }

    private var selectedResource: PetQuickChatResource? {
        interactionState.selectedQuickChatResourceID.flatMap { selectedID in
            resources.first(where: { $0.id == selectedID })
        }
    }

    private var resources: [PetQuickChatResource] {
        model.petQuickChatResources
    }

    private var resourceList: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "message.fill")
                    .foregroundStyle(.tint)
                    .frame(width: 30, height: 30)
                    .background(Color.accentColor.opacity(0.11), in: Circle())
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.localized("快捷聊天", english: "Quick Chat"))
                        .font(.system(size: 14, weight: .semibold))
                    Text(model.localized("选择联系人或常用项目", english: "Choose a contact or favorite project"))
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
                Spacer()
                closeButton
            }
            .padding(14)

            Divider()

            ScrollView {
                LazyVStack(spacing: 8) {
                    ForEach(resources) { resource in
                        resourceButton(resource)
                    }

                    if resources.isEmpty {
                        ContentUnavailableView(
                            model.localized("暂无快捷会话", english: "No Quick Conversations"),
                            systemImage: "message.badge",
                            description: Text(model.localized(
                                "请先在项目设置中添加常用项目。",
                                english: "Add a favorite project in Project Settings first."
                            ))
                        )
                        .frame(minHeight: 260)
                    } else if resources.allSatisfy({ $0.kind == .contact }) {
                        Text(model.localized(
                            "可在项目设置中开启“设为常用项目”。",
                            english: "Enable “Add to Favorite Projects” in Project Settings."
                        ))
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                        .padding(.top, 10)
                    }
                }
                .padding(12)
            }
        }
    }

    private func resourceButton(_ resource: PetQuickChatResource) -> some View {
        Button {
            interactionState.selectedQuickChatResourceID = resource.id
        } label: {
            HStack(spacing: 11) {
                Image(systemName: resource.kind == .contact ? "person.crop.circle.fill" : "folder.fill")
                    .font(.system(size: 15))
                    .foregroundStyle(resource.kind == .contact ? Color.orange : Color.accentColor)
                    .frame(width: 34, height: 34)
                    .background(
                        (resource.kind == .contact ? Color.orange : Color.accentColor).opacity(0.1),
                        in: RoundedRectangle(cornerRadius: 9)
                    )
                VStack(alignment: .leading, spacing: 3) {
                    Text(resource.title)
                        .font(.system(size: 13, weight: .semibold))
                        .lineLimit(1)
                    Text(resource.subtitle ?? model.localized("最近会话", english: "Recent conversation"))
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer()
                if resource.conversationID == nil {
                    ProgressView().controlSize(.small)
                } else {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.tertiary)
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color(nsColor: .controlBackgroundColor).opacity(0.66), in: RoundedRectangle(cornerRadius: 11))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private var closeButton: some View {
        Button(action: close) {
            Image(systemName: "xmark")
                .font(.system(size: 11, weight: .semibold))
                .frame(width: 26, height: 26)
                .background(Color(nsColor: .controlBackgroundColor), in: Circle())
        }
        .buttonStyle(.plain)
        .help(model.localized("关闭", english: "Close"))
    }

    private func close() {
        interactionState.selectedQuickChatResourceID = nil
        interactionState.isQuickChatPresented = false
    }
}

private struct PetQuickChatConversationView: View {
    @EnvironmentObject private var model: AppModel
    let resource: PetQuickChatResource
    let conversation: ConversationSessionViewModel?
    let onBack: () -> Void
    let onClose: () -> Void
    let onInspectTaskReply: (TaskReplySelection, any MessageTaskGraphServicing) -> Void

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if let conversation {
                PetQuickChatTimeline(
                    conversation: conversation,
                    onInspectTaskReply: onInspectTaskReply
                )
                Divider()
                PetQuickChatComposer(conversation: conversation)
            } else {
                ContentUnavailableView(
                    model.localized("会话准备中", english: "Preparing Conversation"),
                    systemImage: "ellipsis.message",
                    description: Text(model.localized(
                        "项目会话创建完成后即可在这里发送消息。",
                        english: "You can send messages here once the project conversation is ready."
                    ))
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Button(action: onBack) {
                Image(systemName: "chevron.left")
                    .frame(width: 26, height: 26)
                    .background(Color(nsColor: .controlBackgroundColor), in: Circle())
            }
            .buttonStyle(.plain)
            VStack(alignment: .leading, spacing: 2) {
                Text(resource.title)
                    .font(.system(size: 14, weight: .semibold))
                    .lineLimit(1)
                Text(resource.kind == .contact
                     ? model.localized("联系人会话", english: "Contact Conversation")
                     : model.localized("项目会话", english: "Project Conversation"))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if conversation?.isRefreshing == true {
                ProgressView().controlSize(.small)
            }
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.system(size: 11, weight: .semibold))
                    .frame(width: 26, height: 26)
                    .background(Color(nsColor: .controlBackgroundColor), in: Circle())
            }
            .buttonStyle(.plain)
        }
        .padding(13)
    }
}

private struct PetQuickChatTimeline: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var conversation: ConversationSessionViewModel
    let onInspectTaskReply: (TaskReplySelection, any MessageTaskGraphServicing) -> Void

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 10) {
                    if conversation.turns.isEmpty, !conversation.isRefreshing {
                        Text(model.localized("还没有聊天记录", english: "No messages yet"))
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                            .padding(.top, 80)
                    }
                    ForEach(Array(conversation.turns.suffix(6))) { turn in
                        messageBubble(turn.userMessage, isUser: true)
                        if let assistantReply = turn.assistantReplies.last {
                            messageBubble(
                                assistantReply.message,
                                isUser: false,
                                taskSelection: assistantReply.taskCallback == nil
                                    ? nil
                                    : TaskReplySelection(
                                        turn: turn,
                                        reply: assistantReply,
                                        initialSection: .detail
                                    )
                            )
                        } else if let finalAssistantMessage = turn.finalAssistantMessage {
                            messageBubble(finalAssistantMessage, isUser: false)
                        } else if turn.status == .streaming {
                            HStack(spacing: 7) {
                                ProgressView().controlSize(.small)
                                Text(model.localized("正在回复…", english: "Replying…"))
                                    .font(.system(size: 11))
                                    .foregroundStyle(.secondary)
                                Spacer()
                            }
                            .id("assistant-\(turn.id)")
                        }
                    }
                    Color.clear.frame(height: 1).id("pet-chat-bottom")
                }
                .padding(12)
            }
            .onAppear {
                conversation.refreshLatest()
                scrollToBottom(proxy, animated: false)
            }
            .onChange(of: conversation.turns.count) {
                scrollToBottom(proxy, animated: true)
            }
            .onChange(of: conversation.turns.last?.revision) {
                scrollToBottom(proxy, animated: true)
            }
        }
        .frame(maxHeight: .infinity)
        .background(Color(nsColor: .textBackgroundColor).opacity(0.28))
    }

    private func messageBubble(
        _ message: ChatMessage,
        isUser: Bool,
        taskSelection: TaskReplySelection? = nil
    ) -> some View {
        HStack {
            if isUser { Spacer(minLength: 54) }
            VStack(alignment: .leading, spacing: 8) {
                Text(message.text)
                    .font(.system(size: 12))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)

                if let taskSelection {
                    Divider()
                    Button {
                        guard let service = conversation.messageTaskGraphService else { return }
                        onInspectTaskReply(taskSelection, service)
                    } label: {
                        Label(
                            model.localized("查看详情与执行过程", english: "View Details and Execution"),
                            systemImage: "doc.text.magnifyingglass"
                        )
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(Color.accentColor)
                    }
                    .buttonStyle(.plain)
                    .help(model.localized(
                        "直接在宠物窗口中查看任务详情和执行过程",
                        english: "View task details and execution without leaving the pet window"
                    ))
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .foregroundStyle(isUser ? Color.white : Color.primary)
            .background(
                isUser ? Color.accentColor : Color(nsColor: .controlBackgroundColor),
                in: RoundedRectangle(cornerRadius: 11)
            )
            .id(message.id)
            if !isUser { Spacer(minLength: 54) }
        }
        .frame(maxWidth: .infinity)
    }

    private func scrollToBottom(_ proxy: ScrollViewProxy, animated: Bool) {
        DispatchQueue.main.async {
            if animated {
                withAnimation(.easeOut(duration: 0.18)) {
                    proxy.scrollTo("pet-chat-bottom", anchor: .bottom)
                }
            } else {
                proxy.scrollTo("pet-chat-bottom", anchor: .bottom)
            }
        }
    }
}

struct PetQuickChatTaskInspectorView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: TaskReplyInspectorViewModel
    private let selection: TaskReplySelection
    private let onClose: () -> Void

    init(
        selection: TaskReplySelection,
        service: any MessageTaskGraphServicing,
        onClose: @escaping () -> Void
    ) {
        self.selection = selection
        self.onClose = onClose
        _viewModel = StateObject(
            wrappedValue: TaskReplyInspectorViewModel(
                selection: selection,
                service: service
            )
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            ScrollView {
                TaskReplyInspectorContent(viewModel: viewModel)
                    .padding(20)
            }
        }
        .frame(width: 720, height: 620)
        .background(
            Color(nsColor: .windowBackgroundColor),
            in: RoundedRectangle(cornerRadius: 16)
        )
        .overlay {
            RoundedRectangle(cornerRadius: 16)
                .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
        }
        .task {
            viewModel.update(selection: selection)
            viewModel.load()
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                Image(systemName: "doc.text.magnifyingglass")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(AppPalette.ai)
                    .frame(width: 32, height: 32)
                    .background(AppPalette.ai.opacity(0.1), in: Circle())
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.localized("任务详情与执行过程", english: "Task Details and Execution"))
                        .font(.system(size: 15, weight: .semibold))
                    Text(viewModel.task?.title ?? model.localized(
                        "正在读取任务信息…",
                        english: "Loading task information…"
                    ))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                }
                Spacer()
                if viewModel.isLoading || viewModel.isLoadingModelOutput {
                    ProgressView().controlSize(.small)
                }
                Button {
                    viewModel.refresh()
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
                .help(model.localized("刷新", english: "Refresh"))
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.system(size: 11, weight: .semibold))
                        .frame(width: 26, height: 26)
                        .background(Color(nsColor: .controlBackgroundColor), in: Circle())
                }
                .buttonStyle(.plain)
                .help(model.localized("关闭", english: "Close"))
            }

            Picker(
                model.localized("查看内容", english: "View"),
                selection: Binding(
                    get: { viewModel.section },
                    set: { viewModel.selectSection($0) }
                )
            ) {
                ForEach(TaskReplyInspectorSection.allCases, id: \.self) { section in
                    Text(section.title(language: model.interfaceLanguage)).tag(section)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
        }
        .padding(16)
    }
}

private struct PetQuickChatComposer: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var conversation: ConversationSessionViewModel
    @FocusState private var isFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            if let error = conversation.sendError {
                Text(error)
                    .font(.system(size: 10))
                    .foregroundStyle(.red)
                    .lineLimit(2)
            }
            HStack(alignment: .bottom, spacing: 8) {
                TextField(
                    model.localized("发送消息…", english: "Send a message…"),
                    text: $conversation.draft,
                    axis: .vertical
                )
                .textFieldStyle(.plain)
                .font(.system(size: 12))
                .lineLimit(1...4)
                .focused($isFocused)
                .onSubmit(send)
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
                .overlay {
                    RoundedRectangle(cornerRadius: 10)
                        .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
                }

                Button(action: send) {
                    Group {
                        if conversation.isSending {
                            ProgressView().controlSize(.small)
                        } else {
                            Image(systemName: "arrow.up")
                                .font(.system(size: 12, weight: .bold))
                        }
                    }
                    .frame(width: 30, height: 30)
                    .foregroundStyle(canSend ? Color.white : Color.secondary)
                    .background(canSend ? Color.accentColor : Color.secondary.opacity(0.14), in: Circle())
                }
                .buttonStyle(.plain)
                .disabled(!canSend)
                .help(model.localized("发送", english: "Send"))
            }
        }
        .padding(11)
        .onAppear { isFocused = true }
    }

    private var canSend: Bool {
        !conversation.isSending
            && !conversation.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func send() {
        guard canSend else { return }
        conversation.sendDraft()
    }
}
