import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct AgentChatComposerView<LeadingControl: View>: View {
    @Binding var text: String
    @Binding var attachments: [ConversationAttachmentDraft]
    @Binding var attachmentError: String?
    let isSending: Bool
    let placeholder: String
    let onSend: () -> Void
    @ViewBuilder let leadingControl: () -> LeadingControl

    @State private var showsFileImporter = false
    @State private var previewedAttachment: ConversationAttachmentDraft?
    @State private var isDropTargeted = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            controls
            attachmentStrip
            errorView
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
            case let .failure(error): attachmentError = error.localizedDescription
            }
        }
        .sheet(item: $previewedAttachment) { attachment in
            ComposerAttachmentPreview(attachment: attachment)
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
        if !attachments.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(attachments) { attachment in
                        ComposerAttachmentChip(
                            attachment: attachment,
                            onPreview: { previewedAttachment = attachment },
                            onRemove: {
                                attachments.removeAll { $0.id == attachment.id }
                                if attachments.isEmpty { attachmentError = nil }
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
        if let attachmentError {
            HStack(alignment: .top, spacing: 7) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                Text(attachmentError)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("关闭", systemImage: "xmark") {
                    self.attachmentError = nil
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
            }
        }
    }

    private var input: some View {
        HStack(alignment: .bottom, spacing: 10) {
            ComposerPasteTextEditor(
                text: $text,
                placeholder: placeholder,
                onSubmit: onSend,
                onPasteContent: handlePasteContent
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

    private var canSend: Bool {
        !isSending && (
            !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                || !attachments.isEmpty
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
        attachmentError = nil
        Task {
            let result = await Task.detached(priority: .userInitiated) {
                Self.loadFiles(urls)
            }.value
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
        var totalBytes = attachments.reduce(0) { $0 + $1.size }
        for attachment in incoming {
            if attachments.count + accepted.count >= maximumCount {
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
        attachments.append(contentsOf: accepted)
        attachmentError = messages.isEmpty ? nil : messages.joined(separator: "；")
    }

    nonisolated private static func loadFiles(
        _ urls: [URL]
    ) -> (attachments: [ConversationAttachmentDraft], errors: [String]) {
        var attachments: [ConversationAttachmentDraft] = []
        var errors: [String] = []
        for url in urls {
            let accessed = url.startAccessingSecurityScopedResource()
            defer { if accessed { url.stopAccessingSecurityScopedResource() } }
            do {
                let values = try url.resourceValues(forKeys: [.isRegularFileKey, .contentTypeKey])
                guard values.isRegularFile == true else {
                    errors.append("“\(url.lastPathComponent)”不是可发送的文件")
                    continue
                }
                let data = try Data(contentsOf: url, options: [.mappedIfSafe])
                let type = values.contentType ?? UTType(filenameExtension: url.pathExtension)
                let mimeType = type?.preferredMIMEType ?? "application/octet-stream"
                attachments.append(.init(
                    name: url.lastPathComponent,
                    mimeType: mimeType,
                    kind: attachmentKind(mimeType),
                    origin: .file,
                    data: data
                ))
            } catch {
                errors.append("无法读取“\(url.lastPathComponent)”：\(error.localizedDescription)")
            }
        }
        return (attachments, errors)
    }

    nonisolated private static func attachmentKind(
        _ mimeType: String
    ) -> ConversationAttachmentKind {
        if mimeType.hasPrefix("image/") { return .image }
        if mimeType.hasPrefix("audio/") { return .audio }
        return .file
    }

    private func attachmentKind(_ mimeType: String) -> ConversationAttachmentKind {
        Self.attachmentKind(mimeType)
    }

    private func pastedName(prefix: String, extension fileExtension: String) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH.mm.ss"
        return "\(prefix) \(formatter.string(from: Date())).\(fileExtension)"
    }
}

struct AgentMessageAttachmentChips: View {
    let attachments: [ProjectAgentMessageAttachment]
    let dataByID: [String: Data]
    @State private var previewedImage: ProjectAgentMessageAttachment?

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            ForEach(attachments) { attachment in
                if attachment.kind == .image,
                   let data = dataByID[attachment.id],
                   let image = NSImage(data: data) {
                    Button {
                        previewedImage = attachment
                    } label: {
                        Image(nsImage: image)
                            .resizable()
                            .scaledToFit()
                            .frame(maxWidth: 360, maxHeight: 220)
                            .clipShape(RoundedRectangle(cornerRadius: 10))
                            .overlay {
                                RoundedRectangle(cornerRadius: 10)
                                    .stroke(AppPalette.border, lineWidth: 1)
                            }
                    }
                    .buttonStyle(.plain)
                } else {
                    HStack(spacing: 7) {
                        Image(systemName: icon(attachment))
                            .foregroundStyle(AppPalette.ai)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(attachment.name)
                                .lineLimit(1)
                                .appFont(.caption.weight(.medium))
                            Text(formatAttachmentSize(attachment.size))
                                .appFont(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .padding(.horizontal, 9)
                    .padding(.vertical, 7)
                    .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 8))
                    .overlay {
                        RoundedRectangle(cornerRadius: 8)
                            .stroke(AppPalette.border, lineWidth: 1)
                    }
                }
            }
        }
        .sheet(item: $previewedImage) { attachment in
            if let data = dataByID[attachment.id], let image = NSImage(data: data) {
                AgentLocalImagePreview(name: attachment.name, image: image)
            }
        }
    }

    private func icon(_ attachment: ProjectAgentMessageAttachment) -> String {
        if attachment.kind == .audio { return "waveform" }
        if attachment.mimeType == "application/pdf" { return "doc.richtext" }
        if attachment.mimeType.hasPrefix("text/") { return "doc.text" }
        return "doc"
    }
}

private struct AgentLocalImagePreview: View {
    let name: String
    let image: NSImage
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(name).appFont(.headline)
                Spacer()
                Button("关闭", action: dismiss.callAsFunction)
                    .keyboardShortcut(.cancelAction)
            }
            .padding(16)
            Divider()
            ScrollView([.horizontal, .vertical]) {
                Image(nsImage: image)
                    .resizable()
                    .scaledToFit()
                    .padding(20)
            }
        }
        .frame(minWidth: 760, idealWidth: 920, minHeight: 560, idealHeight: 720)
    }
}
