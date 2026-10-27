import ChatOSCore
import ChatOSConnector
import SwiftUI
import UniformTypeIdentifiers

struct ComposerView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var conversation: ConversationSessionViewModel
    @ObservedObject private var composerState: ConversationComposerState
    @State private var showsFileImporter = false
    @State private var previewedAttachment: ConversationAttachmentDraft?
    @State private var isDropTargeted = false
    @State private var pluginCapabilityCandidates: [ComposerCapabilityCandidate] = []
    @State private var capabilityCatalogError: String?
    @State private var highlightedCapabilityID: String?
    @State private var suppressesCapabilitySuggestions = false

    init(conversation: ConversationSessionViewModel) {
        self.conversation = conversation
        _composerState = ObservedObject(wrappedValue: conversation.composerState)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            controls
            selectedCapabilityChip
            attachmentStrip
            attachmentError
            inputWithCapabilitySuggestions
        }
        .padding(14)
        .background(AppPalette.surfaceSubtle, in: RoundedRectangle(cornerRadius: 13))
        .overlay {
            RoundedRectangle(cornerRadius: 13).stroke(AppPalette.border, lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.055), radius: 10, y: 3)
        .overlay {
            if isDropTargeted {
                RoundedRectangle(cornerRadius: 13)
                    .stroke(AppPalette.ai, style: StrokeStyle(lineWidth: 2, dash: [7, 5]))
                    .background(AppPalette.ai.opacity(0.05), in: RoundedRectangle(cornerRadius: 13))
                    .allowsHitTesting(false)
            }
        }
        .dropDestination(for: URL.self) { urls, _ in
            conversation.addAttachmentFiles(urls)
            return !urls.isEmpty
        } isTargeted: { targeted in
            isDropTargeted = targeted
        }
        .fileImporter(
            isPresented: $showsFileImporter,
            allowedContentTypes: [.item],
            allowsMultipleSelection: true
        ) { result in
            switch result {
            case let .success(urls):
                conversation.addAttachmentFiles(urls)
            case let .failure(error):
                composerState.attachmentError = error.localizedDescription
            }
        }
        .sheet(item: $previewedAttachment) { attachment in
            ComposerAttachmentPreview(attachment: attachment)
        }
        .task(id: capabilityCatalogReloadKey) {
            await loadPluginCapabilityCandidates()
        }
        .onChange(of: composerState.draft) { _, _ in
            suppressesCapabilitySuggestions = false
            selectFirstCapabilitySuggestion()
        }
        .onChange(of: pluginCapabilityCandidates) { _, _ in
            selectFirstCapabilitySuggestion()
        }
    }

    private var controls: some View {
        HStack(spacing: 8) {
            Menu(selectedModelDisplayName) {
                ForEach(conversation.availableModels) { model in
                    Button {
                        conversation.setSelectedModelID(model.id)
                    } label: {
                        if conversation.selectedModelID == model.id {
                            Label(model.displayName, systemImage: "checkmark")
                        } else {
                            Text(model.displayName)
                        }
                    }
                }
            }
            .disabled(conversation.availableModels.isEmpty || conversation.isUpdatingRuntimeSettings)
            Menu {
                Button {
                    conversation.setRemoteConnectionID(nil)
                } label: {
                    if conversation.selectedRemoteConnectionID == nil {
                        Label(
                            model.localized("不使用远端", english: "No Remote Connection"),
                            systemImage: "checkmark"
                        )
                    } else {
                        Text(model.localized("不使用远端", english: "No Remote Connection"))
                    }
                }
                if !model.remoteConnections.isEmpty {
                    Divider()
                    ForEach(model.remoteConnections) { connection in
                        Button {
                            conversation.setRemoteConnectionID(connection.id)
                        } label: {
                            if conversation.selectedRemoteConnectionID == connection.id {
                                Label(connection.name, systemImage: "checkmark")
                            } else {
                                Text(connection.name)
                            }
                        }
                    }
                }
            } label: {
                Label(
                    selectedRemoteConnectionDisplayName,
                    systemImage: conversation.selectedRemoteConnectionID == nil
                        ? "network.slash"
                        : "network"
                )
            }
            .disabled(conversation.isUpdatingRuntimeSettings)
            .help(model.localized(
                "为当前会话绑定远端连接；之后的新任务会默认使用该服务器",
                english: "Bind a remote connection to this conversation for future tasks"
            ))
            Button(model.localized("附件", english: "Attachments"), systemImage: "paperclip") {
                showsFileImporter = true
            }
                .labelStyle(.iconOnly)
                .help(model.localized(
                    "添加图片、文档或其他文件；也可以直接粘贴或拖入",
                    english: "Add images, documents, or other files. You can also paste or drag them here."
                ))
            ReasoningLevelControl(conversation: conversation)
            Spacer()
        }
        .controlSize(.small)
    }

    private var selectedModelDisplayName: String {
        if let selected = conversation.availableModels.first(where: {
            $0.id == conversation.selectedModelID
        }) {
            return selected.displayName
        }
        if let first = conversation.availableModels.first {
            return first.displayName
        }
        return model.localized("选择模型", english: "Select Model")
    }

    private var selectedRemoteConnectionDisplayName: String {
        guard let connectionID = conversation.selectedRemoteConnectionID else {
            return model.localized("未绑定远端", english: "No Remote")
        }
        if let connection = model.remoteConnections.first(where: { $0.id == connectionID }) {
            return model.localized(
                "远端：\(connection.name)",
                english: "Remote: \(connection.name)"
            )
        }
        return model.localized("远端不可用", english: "Remote Unavailable")
    }

    @ViewBuilder
    private var selectedCapabilityChip: some View {
        if let selection = composerState.selectedCapability {
            HStack(spacing: 6) {
                Image(systemName: selection.kind == .plugin ? "shippingbox.fill" : "list.clipboard.fill")
                    .foregroundStyle(AppPalette.ai)
                Text(selection.displayName)
                    .appFont(.caption.weight(.semibold))
                Button {
                    composerState.selectedCapability = nil
                } label: {
                    Image(systemName: "xmark")
                        .appFont(.caption2.weight(.semibold))
                        .frame(width: 18, height: 18)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(model.localized("移除所选能力", english: "Remove selected capability"))
            }
            .padding(.leading, 9)
            .padding(.trailing, 4)
            .padding(.vertical, 4)
            .background(AppPalette.aiSoft, in: Capsule())
            .overlay { Capsule().stroke(AppPalette.ai.opacity(0.18), lineWidth: 1) }
            .accessibilityElement(children: .combine)
            .accessibilityLabel(model.localized(
                "已选择能力：\(selection.displayName)",
                english: "Selected capability: \(selection.displayName)"
            ))
        }
    }

    @ViewBuilder
    private var attachmentStrip: some View {
        if !composerState.attachments.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(composerState.attachments) { attachment in
                        ComposerAttachmentChip(
                            attachment: attachment,
                            onPreview: { previewedAttachment = attachment },
                            onRemove: { conversation.removeAttachment(id: attachment.id) }
                        )
                    }
                }
                .padding(.horizontal, 1)
            }
        }
    }

    @ViewBuilder
    private var attachmentError: some View {
        if let error = composerState.attachmentError {
            HStack(alignment: .top, spacing: 7) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                Text(error)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button(model.localized("关闭", english: "Dismiss"), systemImage: "xmark") {
                    conversation.clearAttachmentError()
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
            }
        }
    }

    private var input: some View {
        HStack(alignment: .bottom, spacing: 10) {
            ComposerPasteTextEditor(
                text: $composerState.draft,
                placeholder: model.localized(
                    "输入消息，或粘贴图片、文档和长文本…",
                    english: "Type a message, or paste images, documents, and long text…"
                ),
                onSubmit: conversation.sendDraft,
                onPasteContent: handlePasteContent,
                onCommand: handleTextCommand
            )

            Button(action: conversation.sendDraft) {
                Group {
                    if conversation.isSending {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: "arrow.up").appFont(.headline)
                    }
                }
                .frame(width: 30, height: 30)
            }
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.circle)
            .disabled(!conversation.canSendDraft)
        }
        .padding(.leading, 13)
        .padding(.trailing, 7)
        .padding(.vertical, 4)
        .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 11))
        .overlay {
            RoundedRectangle(cornerRadius: 11)
                .stroke(AppPalette.ai.opacity(0.24), lineWidth: 1)
        }
    }

    private var inputWithCapabilitySuggestions: some View {
        input
            .overlay(alignment: .topLeading) {
                capabilitySuggestions
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .alignmentGuide(.top) { dimensions in
                        dimensions[.bottom] + 8
                    }
            }
    }

    @ViewBuilder
    private var capabilitySuggestions: some View {
        if let query = activeCapabilityQuery {
            VStack(alignment: .leading, spacing: 2) {
                ScrollViewReader { proxy in
                    ScrollView(.vertical) {
                        LazyVStack(alignment: .leading, spacing: 2) {
                            ForEach(visibleCapabilityCandidates) { candidate in
                                Button {
                                    selectCapability(candidate, query: query)
                                } label: {
                                    HStack(spacing: 10) {
                                        Image(systemName: candidate.selection.kind == .plugin
                                            ? "shippingbox.fill" : "list.clipboard.fill")
                                            .foregroundStyle(.tint)
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(candidate.selection.displayName)
                                                .appFont(.body.weight(.medium))
                                                .foregroundStyle(.primary)
                                            Text(candidate.subtitle)
                                                .appFont(.caption)
                                                .foregroundStyle(.secondary)
                                                .lineLimit(2)
                                        }
                                        Spacer()
                                        if highlightedCapabilityID == candidate.id {
                                            Text("↩")
                                                .appFont(.caption)
                                                .foregroundStyle(.secondary)
                                        }
                                    }
                                    .padding(.horizontal, 10)
                                    .padding(.vertical, 7)
                                    .contentShape(Rectangle())
                                    .background(
                                        highlightedCapabilityID == candidate.id
                                            ? Color.accentColor.opacity(0.12)
                                            : Color.clear,
                                        in: RoundedRectangle(cornerRadius: 8)
                                    )
                                }
                                .buttonStyle(.plain)
                                .id(candidate.id)
                            }
                        }
                    }
                    .frame(maxHeight: 360)
                    .onChange(of: highlightedCapabilityID) { _, id in
                        guard let id else { return }
                        withAnimation(.easeOut(duration: 0.12)) {
                            proxy.scrollTo(id, anchor: .center)
                        }
                    }
                }
                if visibleCapabilityCandidates.isEmpty {
                    Text(model.localized("没有匹配的能力", english: "No matching capability"))
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 8)
                }
                if let capabilityCatalogError {
                    Text(capabilityCatalogError)
                        .appFont(.caption2)
                        .foregroundStyle(.orange)
                        .padding(.horizontal, 10)
                        .padding(.bottom, 6)
                }
            }
            .padding(6)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 11))
            .overlay {
                RoundedRectangle(cornerRadius: 11)
                    .stroke(AppPalette.border, lineWidth: 1)
            }
            .shadow(color: .black.opacity(0.10), radius: 10, y: 4)
        }
    }

    private var capabilityCatalogReloadKey: String {
        let owner = model.localProjectOwnerUserID ?? ""
        let applications = model.pluginApplications
            .map(\.id)
            .sorted()
            .joined(separator: "|")
        return "\(owner)|\(applications)"
    }

    private var capabilityCandidates: [ComposerCapabilityCandidate] {
        [ComposerCapabilityCandidate(
            selection: .requirementSurvey(displayName: model.localized(
                "需求调研",
                english: "Requirement Survey"
            )),
            subtitle: model.localized(
                "先澄清目标、范围、偏好和验收标准",
                english: "Clarify goals, scope, preferences, and acceptance criteria first"
            )
        )] + pluginCapabilityCandidates
    }

    private var activeCapabilityQuery: ComposerCapabilityQuery? {
        guard !suppressesCapabilitySuggestions else { return nil }
        return ComposerCapabilitySyntax.trailingQuery(in: composerState.draft)
    }

    private var visibleCapabilityCandidates: [ComposerCapabilityCandidate] {
        guard let query = activeCapabilityQuery else { return [] }
        let normalized = query.value.trimmingCharacters(in: .whitespacesAndNewlines)
        return capabilityCandidates.filter { candidate in
            normalized.isEmpty
                || candidate.selection.displayName.localizedCaseInsensitiveContains(normalized)
                || candidate.subtitle.localizedCaseInsensitiveContains(normalized)
                || candidate.selection.pluginKey?.localizedCaseInsensitiveContains(normalized) == true
        }
    }

    @MainActor
    private func loadPluginCapabilityCandidates() async {
        guard let ownerUserID = model.localProjectOwnerUserID else {
            pluginCapabilityCandidates = []
            capabilityCatalogError = nil
            return
        }
        do {
            let plugins = try await model.localConnectorService.installedAgentPlugins(
                ownerUserID: ownerUserID
            )
            guard !Task.isCancelled else { return }
            pluginCapabilityCandidates = plugins.map { plugin in
                ComposerCapabilityCandidate(
                    selection: .plugin(
                        key: plugin.pluginKey,
                        componentKey: plugin.componentKey,
                        displayName: plugin.displayName
                    ),
                    subtitle: plugin.description
                )
            }
            capabilityCatalogError = nil
        } catch {
            guard !Task.isCancelled else { return }
            pluginCapabilityCandidates = []
            capabilityCatalogError = model.localized(
                "插件列表暂时不可用：\(error.localizedDescription)",
                english: "Plugin list is temporarily unavailable: \(error.localizedDescription)"
            )
        }
    }

    private func selectFirstCapabilitySuggestion() {
        let visibleIDs = Set(visibleCapabilityCandidates.map(\.id))
        if let highlightedCapabilityID, visibleIDs.contains(highlightedCapabilityID) { return }
        highlightedCapabilityID = visibleCapabilityCandidates.first?.id
    }

    private func selectCapability(
        _ candidate: ComposerCapabilityCandidate,
        query: ComposerCapabilityQuery? = nil
    ) {
        guard let query = query ?? ComposerCapabilitySyntax.trailingQuery(in: composerState.draft)
        else { return }
        composerState.draft = ComposerCapabilitySyntax.removingTrailingQuery(
            query,
            from: composerState.draft
        )
        composerState.selectedCapability = candidate.selection
        highlightedCapabilityID = nil
        suppressesCapabilitySuggestions = false
    }

    private func handleTextCommand(_ command: ComposerTextCommand) -> Bool {
        guard activeCapabilityQuery != nil else { return false }
        let candidates = visibleCapabilityCandidates
        switch command {
        case .escape:
            suppressesCapabilitySuggestions = true
            return true
        case .moveUp, .moveDown:
            guard !candidates.isEmpty else { return true }
            let currentIndex = candidates.firstIndex { $0.id == highlightedCapabilityID } ?? 0
            let offset = command == .moveDown ? 1 : -1
            highlightedCapabilityID = candidates[
                (currentIndex + offset + candidates.count) % candidates.count
            ].id
            return true
        case .submit:
            guard let candidate = candidates.first(where: { $0.id == highlightedCapabilityID })
                ?? candidates.first else { return true }
            selectCapability(candidate)
            return true
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
