import ChatOSCore
import Foundation
import UniformTypeIdentifiers

extension ConversationSessionViewModel {
    static let longPasteCharacterThreshold = 4_000
    static let longPasteByteThreshold = 8_000

    var canSendDraft: Bool {
        !isSending && (
            !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                || !attachments.isEmpty
        )
    }

    func addAttachmentFiles(_ urls: [URL]) {
        guard !urls.isEmpty else { return }
        attachmentError = nil
        Task {
            let result = await Task.detached(priority: .userInitiated) {
                Self.loadAttachmentFiles(urls)
            }.value
            appendAttachments(result.attachments, errors: result.errors)
        }
    }

    func addPastedImage(data: Data, mimeType: String, suggestedName: String? = nil) {
        appendAttachments([
            ConversationAttachmentDraft(
                name: suggestedName ?? Self.pastedName(prefix: "粘贴的图片", extension: "png"),
                mimeType: mimeType,
                kind: .image,
                origin: .pastedImage,
                data: data
            ),
        ])
    }

    func addPastedDocument(
        data: Data,
        mimeType: String,
        suggestedName: String,
        kind: ConversationAttachmentKind = .file
    ) {
        appendAttachments([
            ConversationAttachmentDraft(
                name: suggestedName,
                mimeType: mimeType,
                kind: kind,
                origin: .pastedDocument,
                data: data
            ),
        ])
    }

    func addLongPastedText(_ text: String) {
        guard let data = text.data(using: .utf8), !data.isEmpty else { return }
        appendAttachments([
            ConversationAttachmentDraft(
                name: Self.pastedName(prefix: "粘贴的长文本", extension: "txt"),
                mimeType: "text/plain",
                kind: .file,
                origin: .pastedText,
                data: data
            ),
        ])
    }

    func removeAttachment(id: String) {
        attachments.removeAll(where: { $0.id == id })
        if attachments.isEmpty { attachmentError = nil }
    }

    func clearAttachmentError() {
        attachmentError = nil
    }

    private func appendAttachments(
        _ incoming: [ConversationAttachmentDraft],
        errors: [String] = []
    ) {
        let maximumCount = 20
        let maximumFileBytes = 20 * 1024 * 1024
        let maximumTotalBytes = 20 * 1024 * 1024
        var accepted: [ConversationAttachmentDraft] = []
        var messages = errors
        var totalBytes = attachments.reduce(0) { $0 + $1.size }

        for attachment in incoming {
            if attachments.count + accepted.count >= maximumCount {
                messages.append("单次最多添加 \(maximumCount) 个附件")
                break
            }
            if attachment.size > maximumFileBytes {
                messages.append("“\(attachment.name)”超过 20 MB")
                continue
            }
            if totalBytes + attachment.size > maximumTotalBytes {
                messages.append("附件总大小不能超过 20 MB")
                continue
            }
            accepted.append(attachment)
            totalBytes += attachment.size
        }

        attachments.append(contentsOf: accepted)
        attachmentError = messages.isEmpty ? nil : messages.joined(separator: "；")
    }

    nonisolated private static func loadAttachmentFiles(
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
                let contentType = values.contentType ?? UTType(filenameExtension: url.pathExtension)
                let mimeType = contentType?.preferredMIMEType ?? "application/octet-stream"
                attachments.append(
                    ConversationAttachmentDraft(
                        name: url.lastPathComponent,
                        mimeType: mimeType,
                        kind: attachmentKind(mimeType: mimeType),
                        origin: .file,
                        data: data
                    )
                )
            } catch {
                errors.append("无法读取“\(url.lastPathComponent)”：\(error.localizedDescription)")
            }
        }
        return (attachments, errors)
    }

    nonisolated private static func attachmentKind(
        mimeType: String
    ) -> ConversationAttachmentKind {
        if mimeType.hasPrefix("image/") { return .image }
        if mimeType.hasPrefix("audio/") { return .audio }
        return .file
    }

    private static func pastedName(prefix: String, extension fileExtension: String) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH.mm.ss"
        return "\(prefix) \(formatter.string(from: Date())).\(fileExtension)"
    }
}
