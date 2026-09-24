import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct PetQuickChatView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var interactionState: PetOverlayInteractionState
    @ObservedObject var translationViewModel: PetTranslationViewModel
    @ObservedObject var notepadViewModel: NotepadViewModel
    let onInspectTaskReply: (TaskReplySelection, any MessageTaskGraphServicing) -> Void

    var body: some View {
        Group {
            if interactionState.isTranslationPresented {
                PetTranslationView(
                    viewModel: translationViewModel,
                    onBack: closeTranslation,
                    onClose: close
                )
            } else if interactionState.isNotepadPresented {
                PetQuickNotepadView(
                    viewModel: notepadViewModel,
                    onBack: closeNotepad,
                    onClose: close
                )
            } else if let selectedResource {
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
                    Text(model.localized(
                        "选择快速功能、联系人或常用项目",
                        english: "Choose a quick action, contact, or favorite project"
                    ))
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
                    translationButton
                    notepadButton

                    ForEach(resources) { resource in
                        resourceButton(resource)
                    }

                    if resources.isEmpty || resources.allSatisfy({ $0.kind == .contact }) {
                        Text(model.localized(
                            resources.isEmpty
                                ? "可直接使用快速翻译；也可在项目设置中添加常用项目。"
                                : "可在项目设置中开启“设为常用项目”。",
                            english: resources.isEmpty
                                ? "Use Quick Translate now, or add favorite projects in Project Settings."
                                : "Enable “Add to Favorite Projects” in Project Settings."
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

    private var translationButton: some View {
        Button {
            interactionState.selectedQuickChatResourceID = nil
            interactionState.isNotepadPresented = false
            interactionState.isTranslationPresented = true
        } label: {
            HStack(spacing: 11) {
                Image(systemName: "translate")
                    .font(.system(size: 15))
                    .foregroundStyle(Color.purple)
                    .frame(width: 34, height: 34)
                    .background(Color.purple.opacity(0.1), in: RoundedRectangle(cornerRadius: 9))
                VStack(alignment: .leading, spacing: 3) {
                    Text(model.localized("快速翻译", english: "Quick Translate"))
                        .font(.system(size: 13, weight: .semibold))
                    Text(model.localized(
                        "粘贴文字、截图或拖入文件",
                        english: "Paste text, screenshots, or drop files"
                    ))
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.tertiary)
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.purple.opacity(0.06), in: RoundedRectangle(cornerRadius: 11))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private var notepadButton: some View {
        Button {
            interactionState.selectedQuickChatResourceID = nil
            interactionState.isTranslationPresented = false
            interactionState.isNotepadPresented = true
        } label: {
            HStack(spacing: 11) {
                Image(systemName: "note.text")
                    .font(.system(size: 15))
                    .foregroundStyle(Color.orange)
                    .frame(width: 34, height: 34)
                    .background(Color.orange.opacity(0.1), in: RoundedRectangle(cornerRadius: 9))
                VStack(alignment: .leading, spacing: 3) {
                    Text(model.localized("快速记事本", english: "Quick Notepad"))
                        .font(.system(size: 13, weight: .semibold))
                    Text(model.localized(
                        "快速记录，与完整记事本同步",
                        english: "Capture notes synced with the full notepad"
                    ))
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.tertiary)
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.orange.opacity(0.06), in: RoundedRectangle(cornerRadius: 11))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func resourceButton(_ resource: PetQuickChatResource) -> some View {
        Button {
            interactionState.isTranslationPresented = false
            interactionState.isNotepadPresented = false
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
        translationViewModel.cancel()
        interactionState.isTranslationPresented = false
        interactionState.isNotepadPresented = false
        interactionState.selectedQuickChatResourceID = nil
        interactionState.isQuickChatPresented = false
    }

    private func closeTranslation() {
        translationViewModel.cancel()
        interactionState.isTranslationPresented = false
    }

    private func closeNotepad() {
        interactionState.isNotepadPresented = false
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
        .onDisappear {
            if let conversation {
                model.deactivatePetConversation(conversation)
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

                if !message.attachments.isEmpty {
                    MessageAttachmentChips(attachments: message.attachments)
                }

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
    @State private var showsFileImporter = false
    @State private var previewedAttachment: ConversationAttachmentDraft?
    @State private var isDropTargeted = false

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            if !conversation.attachments.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 7) {
                        ForEach(conversation.attachments) { attachment in
                            ComposerAttachmentChip(
                                attachment: attachment,
                                onPreview: { previewedAttachment = attachment },
                                onRemove: { conversation.removeAttachment(id: attachment.id) }
                            )
                        }
                    }
                }
            }
            if let error = conversation.attachmentError ?? conversation.sendError {
                Text(error)
                    .font(.system(size: 10))
                    .foregroundStyle(.red)
                    .lineLimit(2)
            }
            HStack(alignment: .bottom, spacing: 8) {
                Button {
                    showsFileImporter = true
                } label: {
                    Image(systemName: "paperclip")
                        .frame(width: 28, height: 28)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .help(model.localized("添加附件", english: "Add Attachment"))

                ComposerPasteTextEditor(
                    text: $conversation.draft,
                    placeholder: model.localized(
                        "发送消息，或粘贴图片和文件…",
                        english: "Send a message, or paste images and files…"
                    ),
                    onSubmit: conversation.sendDraft,
                    onPasteContent: handlePasteContent
                )

                Button(action: conversation.sendDraft) {
                    Group {
                        if conversation.isSending {
                            ProgressView().controlSize(.small)
                        } else {
                            Image(systemName: "arrow.up")
                                .font(.system(size: 12, weight: .bold))
                        }
                    }
                    .frame(width: 30, height: 30)
                    .foregroundStyle(conversation.canSendDraft ? Color.white : Color.secondary)
                    .background(
                        conversation.canSendDraft
                            ? Color.accentColor
                            : Color.secondary.opacity(0.14),
                        in: Circle()
                    )
                }
                .buttonStyle(.plain)
                .disabled(!conversation.canSendDraft)
                .help(model.localized("发送", english: "Send"))
            }
            .padding(.leading, 7)
            .padding(.trailing, 5)
            .padding(.vertical, 4)
            .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 12))
            .overlay {
                RoundedRectangle(cornerRadius: 12)
                    .stroke(Color(nsColor: .separatorColor).opacity(0.72), lineWidth: 1)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .overlay {
            if isDropTargeted {
                RoundedRectangle(cornerRadius: 12)
                    .stroke(Color.accentColor, style: StrokeStyle(lineWidth: 2, dash: [7, 5]))
                    .allowsHitTesting(false)
            }
        }
        .dropDestination(for: URL.self) { urls, _ in
            conversation.addAttachmentFiles(urls)
            return !urls.isEmpty
        } isTargeted: { isDropTargeted = $0 }
        .fileImporter(
            isPresented: $showsFileImporter,
            allowedContentTypes: [.item],
            allowsMultipleSelection: true
        ) { result in
            switch result {
            case let .success(urls):
                conversation.addAttachmentFiles(urls)
            case let .failure(error):
                conversation.attachmentError = error.localizedDescription
            }
        }
        .sheet(item: $previewedAttachment) { attachment in
            ComposerAttachmentPreview(attachment: attachment)
        }
    }

    private func handlePasteContent(_ content: ComposerPasteContent) {
        switch content {
        case let .files(urls):
            conversation.addAttachmentFiles(urls)
        case let .image(data, mimeType, suggestedName):
            conversation.addPastedImage(
                data: data,
                mimeType: mimeType,
                suggestedName: suggestedName
            )
        case let .document(data, mimeType, suggestedName):
            conversation.addPastedDocument(
                data: data,
                mimeType: mimeType,
                suggestedName: suggestedName
            )
        case let .longText(text):
            conversation.addLongPastedText(text)
        }
    }
}
