import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

@MainActor
final class AgentChatComposerState: ObservableObject {
    @Published var draftMessage = ""
    @Published var attachments: [ConversationAttachmentDraft] = []
    @Published var attachmentError: String?
    @Published var selectedMentionAgentIDs: Set<String> = []
}

struct AgentChatComposerView<LeadingControl: View>: View {
    @ObservedObject var state: AgentChatComposerState
    let isSending: Bool
    let placeholder: String
    let mentionCandidates: [AgentChatMentionCandidate]
    let onMentionSelected: (String) -> Void
    let onSend: () -> Void
    @ViewBuilder let leadingControl: () -> LeadingControl

    @State private var showsFileImporter = false
    @State private var previewedAttachment: ConversationAttachmentDraft?
    @State private var isDropTargeted = false
    @State private var highlightedMentionID: String?
    @State private var suppressesMentionSuggestions = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            controls
            attachmentStrip
            errorView
            mentionSuggestions
            input
        }
        .padding(12)
        .background(AppPalette.surfaceSubtle, in: RoundedRectangle(cornerRadius: 13))
        .overlay {
            RoundedRectangle(cornerRadius: 13)
                .stroke(AppPalette.border, lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.05), radius: 8, y: 2)
        .overlay {
            if isDropTargeted {
                RoundedRectangle(cornerRadius: 13)
                    .stroke(AppPalette.ai, style: StrokeStyle(lineWidth: 2, dash: [7, 5]))
                    .background(
                        AppPalette.ai.opacity(0.05),
                        in: RoundedRectangle(cornerRadius: 13)
                    )
                    .allowsHitTesting(false)
            }
        }
        .dropDestination(for: URL.self) { urls, _ in
            addFiles(urls)
            return !urls.isEmpty
        } isTargeted: { isDropTargeted = $0 }
        .fileImporter(
            isPresented: $showsFileImporter,
            allowedContentTypes: [.item],
            allowsMultipleSelection: true
        ) { result in
            switch result {
            case let .success(urls): addFiles(urls)
            case let .failure(error): state.attachmentError = error.localizedDescription
            }
        }
        .sheet(item: $previewedAttachment) { attachment in
            ComposerAttachmentPreview(attachment: attachment)
        }
        .onChange(of: state.draftMessage) { _, _ in
            suppressesMentionSuggestions = false
            selectFirstMentionSuggestion()
        }
        .onChange(of: mentionCandidates) { _, _ in
            selectFirstMentionSuggestion()
        }
    }

    private var controls: some View {
        HStack(spacing: 8) {
            leadingControl()
            Button("附件", systemImage: "paperclip") {
                showsFileImporter = true
            }
            .labelStyle(.iconOnly)
            .help("添加图片、文档或其他文件；也可以直接粘贴或拖入")
            Spacer()
        }
        .controlSize(.small)
    }

    @ViewBuilder
    private var attachmentStrip: some View {
        if !state.attachments.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(state.attachments) { attachment in
                        ComposerAttachmentChip(
                            attachment: attachment,
                            onPreview: { previewedAttachment = attachment },
                            onRemove: {
                                state.attachments.removeAll { $0.id == attachment.id }
                                if state.attachments.isEmpty { state.attachmentError = nil }
                            }
                        )
                    }
                }
                .padding(.horizontal, 1)
            }
        }
    }

    @ViewBuilder
    private var errorView: some View {
        if let attachmentError = state.attachmentError {
            HStack(alignment: .top, spacing: 7) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                Text(attachmentError)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("关闭", systemImage: "xmark") {
                    state.attachmentError = nil
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
            }
        }
    }

    private var input: some View {
        HStack(alignment: .bottom, spacing: 10) {
            ComposerPasteTextEditor(
                text: $state.draftMessage,
                placeholder: placeholder,
                onSubmit: onSend,
                onPasteContent: handlePasteContent,
                onCommand: handleTextCommand
            )
            Button(action: onSend) {
                Group {
                    if isSending {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: "arrow.up").appFont(.headline)
                    }
                }
                .frame(width: 30, height: 30)
            }
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.circle)
            .disabled(!canSend)
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

    @ViewBuilder
    private var mentionSuggestions: some View {
        if !visibleMentionCandidates.isEmpty {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(visibleMentionCandidates.prefix(6)) { candidate in
                    Button {
                        selectMention(candidate)
                    } label: {
                        HStack(spacing: 10) {
                            Image(systemName: "person.crop.circle.fill")
                                .foregroundStyle(.tint)
                            VStack(alignment: .leading, spacing: 2) {
                                Text("@\(candidate.name)")
                                    .appFont(.body)
                                    .foregroundStyle(.primary)
                                if let subtitle = candidate.subtitle, !subtitle.isEmpty {
                                    Text(subtitle)
                                        .appFont(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            Spacer()
                            if highlightedMentionID == candidate.id {
                                Text("↩")
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 7)
                        .contentShape(Rectangle())
                        .background(
                            highlightedMentionID == candidate.id
                                ? Color.accentColor.opacity(0.12)
                                : Color.clear,
                            in: RoundedRectangle(cornerRadius: 8)
                        )
                    }
                    .buttonStyle(.plain)
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

    private var activeMentionQuery: AgentChatMentionQuery? {
        guard !suppressesMentionSuggestions else { return nil }
        return AgentChatMentionSyntax.trailingQuery(in: state.draftMessage)
    }

    private var visibleMentionCandidates: [AgentChatMentionCandidate] {
        guard let query = activeMentionQuery else { return [] }
        let normalized = query.value.trimmingCharacters(in: .whitespacesAndNewlines)
        let matches = mentionCandidates.filter {
            normalized.isEmpty || $0.name.localizedCaseInsensitiveContains(normalized)
        }
        return matches.sorted(by: { lhs, rhs in
            let prefixOptions: String.CompareOptions = [
                .caseInsensitive,
                .diacriticInsensitive,
                .anchored,
            ]
            let leftPrefix = lhs.name.range(of: normalized, options: prefixOptions) != nil
            let rightPrefix = rhs.name.range(of: normalized, options: prefixOptions) != nil
            if leftPrefix != rightPrefix { return leftPrefix }
            return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
        })
    }

    private func selectFirstMentionSuggestion() {
        let visibleIDs = Set(visibleMentionCandidates.prefix(6).map(\.id))
        if let highlightedMentionID, visibleIDs.contains(highlightedMentionID) { return }
        highlightedMentionID = visibleMentionCandidates.first?.id
    }

    private func selectMention(_ candidate: AgentChatMentionCandidate) {
        guard let query = AgentChatMentionSyntax.trailingQuery(in: state.draftMessage) else { return }
        state.draftMessage = AgentChatMentionSyntax.removingTrailingQuery(
            query,
            from: state.draftMessage
        )
        onMentionSelected(candidate.id)
        highlightedMentionID = nil
        suppressesMentionSuggestions = false
    }

    private func handleTextCommand(_ command: ComposerTextCommand) -> Bool {
        let candidates = Array(visibleMentionCandidates.prefix(6))
        guard !candidates.isEmpty else { return false }
        switch command {
        case .moveUp, .moveDown:
            let currentIndex = candidates.firstIndex { $0.id == highlightedMentionID } ?? 0
            let offset = command == .moveDown ? 1 : -1
            highlightedMentionID = candidates[(currentIndex + offset + candidates.count) % candidates.count].id
            return true
        case .escape:
            suppressesMentionSuggestions = true
            return true
        case .submit:
            selectMention(candidates.first { $0.id == highlightedMentionID } ?? candidates[0])
            return true
        }
    }

    private var canSend: Bool {
        !isSending && (
            !state.draftMessage.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                || !state.attachments.isEmpty
        )
    }

    private func handlePasteContent(_ content: ComposerPasteContent) {
        switch content {
        case let .files(urls):
            addFiles(urls)
        case let .image(data, mimeType, suggestedName):
            append([
                .init(
                    name: suggestedName,
                    mimeType: mimeType,
                    kind: .image,
                    origin: .pastedImage,
                    data: data
                ),
            ])
        case let .document(data, mimeType, suggestedName):
            append([
                .init(
                    name: suggestedName,
                    mimeType: mimeType,
                    kind: attachmentKind(mimeType),
                    origin: .pastedDocument,
                    data: data
                ),
            ])
        case let .longText(value):
            guard let data = value.data(using: .utf8), !data.isEmpty else { return }
            append([
                .init(
                    name: pastedName(prefix: "粘贴的长文本", extension: "txt"),
                    mimeType: "text/plain",
                    kind: .file,
                    origin: .pastedText,
                    data: data
                ),
            ])
        }
    }

    private func addFiles(_ urls: [URL]) {
        guard !urls.isEmpty else { return }
        state.attachmentError = nil
        Task {
            let result = await ConversationSessionViewModel.loadAttachmentFilesOffMain(urls)
            guard !Task.isCancelled else { return }
            append(result.attachments, errors: result.errors)
        }
    }

    private func append(
        _ incoming: [ConversationAttachmentDraft],
        errors: [String] = []
    ) {
        let maximumCount = 20
        let maximumBytes = 20 * 1_024 * 1_024
        var accepted: [ConversationAttachmentDraft] = []
        var messages = errors
        var totalBytes = state.attachments.reduce(0) { $0 + $1.size }
        for attachment in incoming {
            if state.attachments.count + accepted.count >= maximumCount {
                messages.append("单次最多添加 \(maximumCount) 个附件")
                break
            }
            if attachment.size > maximumBytes {
                messages.append("“\(attachment.name)”超过 20 MB")
                continue
            }
            if totalBytes + attachment.size > maximumBytes {
                messages.append("附件总大小不能超过 20 MB")
                continue
            }
            accepted.append(attachment)
            totalBytes += attachment.size
        }
        state.attachments.append(contentsOf: accepted)
        state.attachmentError = messages.isEmpty ? nil : messages.joined(separator: "；")
    }

    private func attachmentKind(_ mimeType: String) -> ConversationAttachmentKind {
        agentChatAttachmentKind(mimeType)
    }

    private func pastedName(prefix: String, extension fileExtension: String) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH.mm.ss"
        return "\(prefix) \(formatter.string(from: Date())).\(fileExtension)"
    }
}

private func agentChatAttachmentKind(_ mimeType: String) -> ConversationAttachmentKind {
    if mimeType.hasPrefix("image/") { return .image }
    if mimeType.hasPrefix("audio/") { return .audio }
    return .file
}
