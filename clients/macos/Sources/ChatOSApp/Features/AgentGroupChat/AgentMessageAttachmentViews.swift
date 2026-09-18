import AppKit
import ChatOSConnector
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct AgentMessageAttachmentChips: View {
    let ownerUserID: String
    let roomID: String
    let messageID: String
    let creatorName: String
    let attachments: [ProjectAgentMessageAttachment]
    let dataByID: [String: Data]
    let service: NativeAgentGroupChatService
    @State private var previewedImage: ProjectAgentMessageAttachment?
    @State private var previewedDocument: AgentMarkdownPreviewItem?
    @State private var loadingAttachmentIDs: Set<String> = []
    @State private var retryingAttachmentIDs: Set<String> = []
    @State private var operationError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            ForEach(attachments, id: \.id) { attachment in
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
                    HStack(spacing: 9) {
                        Button {
                            if isMarkdown(attachment) {
                                loadMarkdownPreview(attachment)
                            } else {
                                saveAttachment(attachment)
                            }
                        } label: {
                            HStack(spacing: 7) {
                                Image(systemName: icon(attachment))
                                    .foregroundStyle(AppPalette.ai)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(attachment.name)
                                        .lineLimit(1)
                                        .appFont(.caption.weight(.medium))
                                    Text("\(formattedSize(attachment.size)) · \(creatorName)")
                                        .lineLimit(1)
                                        .appFont(.caption2)
                                        .foregroundStyle(.secondary)
                                    Label(
                                        syncStatusText(attachment),
                                        systemImage: syncStatusIcon(attachment)
                                    )
                                    .appFont(.caption2)
                                    .foregroundStyle(syncStatusColor(attachment))
                                }
                            }
                        }
                        .buttonStyle(.plain)
                        .disabled(loadingAttachmentIDs.contains(attachment.id))
                        Spacer(minLength: 6)
                        if loadingAttachmentIDs.contains(attachment.id) {
                            ProgressView().controlSize(.small)
                        }
                        if attachment.syncStatus == .failed {
                            Button("重试", systemImage: "arrow.clockwise") {
                                retryUpload(attachment)
                            }
                            .labelStyle(.iconOnly)
                            .help(attachment.uploadError ?? "重新同步到云端")
                            .disabled(retryingAttachmentIDs.contains(attachment.id))
                        }
                        Button("另存为", systemImage: "square.and.arrow.down") {
                            saveAttachment(attachment)
                        }
                        .labelStyle(.iconOnly)
                        .help("另存为…")
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
        .sheet(item: $previewedDocument) { item in
            AgentMarkdownAttachmentPreview(item: item)
        }
        .alert("附件操作失败", isPresented: Binding(
            get: { operationError != nil },
            set: { if !$0 { operationError = nil } }
        )) {
            Button("好", role: .cancel) { operationError = nil }
        } message: {
            Text(operationError ?? "")
        }
    }

    private func loadMarkdownPreview(_ attachment: ProjectAgentMessageAttachment) {
        guard loadingAttachmentIDs.insert(attachment.id).inserted else { return }
        Task {
            defer { loadingAttachmentIDs.remove(attachment.id) }
            let startedAt = ContinuousClock.now
            do {
                let data = try await attachmentData(attachment)
                guard let markdown = String(data: data, encoding: .utf8) else {
                    throw AgentAttachmentPresentationError.invalidUTF8
                }
                previewedDocument = .init(attachment: attachment, markdown: markdown)
                await recordPreview(outcome: .succeeded, startedAt: startedAt)
            } catch {
                await recordPreview(outcome: .failed, startedAt: startedAt)
                operationError = error.localizedDescription
            }
        }
    }

    private func recordPreview(
        outcome: AgentDocumentPreviewMetricOutcome,
        startedAt: ContinuousClock.Instant
    ) async {
        let duration = startedAt.duration(to: .now)
        let milliseconds = max(
            0,
            Int64(duration.components.seconds) * 1_000
                + Int64(duration.components.attoseconds / 1_000_000_000_000_000)
        )
        guard let store = try? await service.store() else { return }
        try? await store.recordAgentDocumentPreview(
            ownerUserID: ownerUserID,
            outcome: outcome,
            durationMilliseconds: milliseconds,
            nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
        )
    }

    private func saveAttachment(_ attachment: ProjectAgentMessageAttachment) {
        guard loadingAttachmentIDs.insert(attachment.id).inserted else { return }
        Task {
            defer { loadingAttachmentIDs.remove(attachment.id) }
            do {
                let data = try await attachmentData(attachment)
                let panel = NSSavePanel()
                panel.nameFieldStringValue = attachment.name
                panel.canCreateDirectories = true
                guard panel.runModal() == .OK, let url = panel.url else { return }
                try data.write(to: url, options: .atomic)
            } catch {
                operationError = error.localizedDescription
            }
        }
    }

    private func retryUpload(_ attachment: ProjectAgentMessageAttachment) {
        guard retryingAttachmentIDs.insert(attachment.id).inserted else { return }
        Task {
            defer { retryingAttachmentIDs.remove(attachment.id) }
            do {
                try await service.retryAgentArtifactUpload(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    attachmentID: attachment.id
                )
            } catch {
                operationError = error.localizedDescription
            }
        }
    }

    private func attachmentData(_ attachment: ProjectAgentMessageAttachment) async throws -> Data {
        if let data = dataByID[attachment.id] { return data }
        let store = try await service.store()
        guard let payload = try await store.messageAttachment(
            ownerUserID: ownerUserID,
            roomID: roomID,
            messageID: messageID,
            attachmentID: attachment.id
        ) else { throw AgentAttachmentPresentationError.notFound }
        let fileURL = payload.localFileURL
        return try await Task.detached(priority: .userInitiated) {
            try Data(contentsOf: fileURL, options: [.mappedIfSafe])
        }.value
    }

    private func isMarkdown(_ attachment: ProjectAgentMessageAttachment) -> Bool {
        attachment.mimeType.lowercased().hasPrefix("text/markdown")
            || attachment.name.lowercased().hasSuffix(".md")
    }

    private func icon(_ attachment: ProjectAgentMessageAttachment) -> String {
        if attachment.kind == .audio { return "waveform" }
        if attachment.mimeType == "application/pdf" { return "doc.richtext" }
        if attachment.mimeType.hasPrefix("text/") { return "doc.text" }
        return "doc"
    }

    private func formattedSize(_ bytes: Int) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: Int64(bytes))
    }

    private func syncStatusText(_ attachment: ProjectAgentMessageAttachment) -> String {
        switch attachment.syncStatus {
        case .localOnly: "本机可用"
        case .queued: "等待云端同步"
        case .uploading: "正在同步"
        case .synced: "云端已同步"
        case .failed: attachment.uploadError ?? "云端同步失败"
        }
    }

    private func syncStatusIcon(_ attachment: ProjectAgentMessageAttachment) -> String {
        switch attachment.syncStatus {
        case .localOnly: "desktopcomputer"
        case .queued: "clock"
        case .uploading: "arrow.triangle.2.circlepath"
        case .synced: "checkmark.icloud"
        case .failed: "exclamationmark.icloud"
        }
    }

    private func syncStatusColor(_ attachment: ProjectAgentMessageAttachment) -> Color {
        switch attachment.syncStatus {
        case .failed: .orange
        case .synced: .green
        default: .secondary
        }
    }
}

private enum AgentAttachmentPresentationError: LocalizedError {
    case invalidUTF8
    case notFound

    var errorDescription: String? {
        switch self {
        case .invalidUTF8: "Markdown 附件不是有效的 UTF-8 文本。"
        case .notFound: "附件不存在或当前消息无权访问。"
        }
    }
}

private struct AgentMarkdownPreviewItem: Identifiable {
    let attachment: ProjectAgentMessageAttachment
    let markdown: String

    var id: String { attachment.id }
}

private struct AgentMarkdownAttachmentPreview: View {
    let item: AgentMarkdownPreviewItem
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var matchCount = 0

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(item.attachment.name).appFont(.headline)
                    Text(formattedSize(item.attachment.size))
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                TextField("搜索文档", text: $query)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 220)
                if !query.isEmpty {
                    Text("\(matchCount) 处")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Button("复制", systemImage: "doc.on.doc") { copyMarkdown() }
                Button("另存为", systemImage: "square.and.arrow.down") { saveMarkdown() }
                Button("关闭", action: dismiss.callAsFunction)
                    .keyboardShortcut(.cancelAction)
            }
            .padding(16)
            Divider()
            ScrollView {
                DeferredMarkdownDocumentView(markdown: item.markdown)
                    .padding(24)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .frame(minWidth: 760, idealWidth: 960, minHeight: 560, idealHeight: 740)
        .task(id: query) {
            let source = item.markdown
            let needle = query
            matchCount = await Task.detached(priority: .utility) {
                guard !needle.isEmpty else { return 0 }
                return source.lowercased().components(separatedBy: needle.lowercased()).count - 1
            }.value
        }
    }

    private func copyMarkdown() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(item.markdown, forType: .string)
    }

    private func saveMarkdown() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = item.attachment.name
        panel.allowedContentTypes = [UTType(filenameExtension: "md") ?? .plainText]
        panel.canCreateDirectories = true
        guard panel.runModal() == .OK, let url = panel.url else { return }
        try? Data(item.markdown.utf8).write(to: url, options: .atomic)
    }

    private func formattedSize(_ bytes: Int) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: Int64(bytes))
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
