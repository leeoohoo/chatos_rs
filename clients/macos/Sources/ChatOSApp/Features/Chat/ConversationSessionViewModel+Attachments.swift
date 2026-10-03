import ChatOSCore
import Foundation
import UniformTypeIdentifiers

extension ConversationSessionViewModel {
    static let longPasteCharacterThreshold = 4_000
    static let longPasteByteThreshold = 8_000
    nonisolated static let maximumAttachmentCount = 20
    nonisolated static let maximumAttachmentBytes = 20 * 1_024 * 1_024

    var canSendDraft: Bool {
        !isSending && (
            !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                || !attachments.isEmpty
        )
    }

    func addAttachmentFiles(_ urls: [URL]) {
        guard !urls.isEmpty else { return }
        attachmentError = nil
        let taskID = UUID()
        let task = Task { [weak self] in
            defer { self?.attachmentFileLoadTasks.removeValue(forKey: taskID) }
            let result = await Self.loadAttachmentFilesOffMain(urls)
            guard !Task.isCancelled else { return }
            self?.appendAttachments(result.attachments, errors: result.errors)
        }
        attachmentFileLoadTasks[taskID] = task
    }

    func addPastedImage(data: Data, mimeType: String, suggestedName: String? = nil) {
        guard AppPastedImageNormalizer.requiresPNGNormalization(mimeType: mimeType) else {
            appendPastedImage(data: data, mimeType: mimeType, suggestedName: suggestedName)
            return
        }

        let taskID = UUID()
        let task = Task { [weak self] in
            do {
                let normalized = try await AppPastedImageNormalizer.normalizeToPNGOffMain(data)
                try Task.checkCancellation()
                self?.appendPastedImage(
                    data: normalized,
                    mimeType: "image/png",
                    suggestedName: suggestedName.map(Self.pngSuggestedName)
                )
            } catch is CancellationError {
                // A discarded conversation should not publish stale attachment state.
            } catch {
                self?.attachmentError = error.localizedDescription
            }
            self?.pastedImageNormalizationTasks.removeValue(forKey: taskID)
        }
        pastedImageNormalizationTasks[taskID] = task
    }

    private func appendPastedImage(
        data: Data,
        mimeType: String,
        suggestedName: String?
    ) {
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

    nonisolated private static func pngSuggestedName(_ name: String) -> String {
        (name as NSString).deletingPathExtension + ".png"
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
        var accepted: [ConversationAttachmentDraft] = []
        var messages = errors
        var totalBytes = attachments.reduce(0) { $0 + $1.size }

        for attachment in incoming {
            if attachments.count + accepted.count >= Self.maximumAttachmentCount {
                messages.append("单次最多添加 \(Self.maximumAttachmentCount) 个附件")
                break
            }
            if attachment.size > Self.maximumAttachmentBytes {
                messages.append("“\(attachment.name)”超过 20 MB")
                continue
            }
            if totalBytes + attachment.size > Self.maximumAttachmentBytes {
                messages.append("附件总大小不能超过 20 MB")
                continue
            }
            accepted.append(attachment)
            totalBytes += attachment.size
        }

        attachments.append(contentsOf: accepted)
        attachmentError = messages.isEmpty ? nil : messages.joined(separator: "；")
    }

    nonisolated static func loadAttachmentFiles(
        _ urls: [URL]
    ) -> (attachments: [ConversationAttachmentDraft], errors: [String]) {
        var attachments: [ConversationAttachmentDraft] = []
        var errors: [String] = []
        var totalBytes = 0
        if urls.count > maximumAttachmentCount {
            errors.append("单次最多添加 \(maximumAttachmentCount) 个附件")
        }
        for url in urls.prefix(maximumAttachmentCount) {
            guard !Task.isCancelled else { break }
            let accessed = url.startAccessingSecurityScopedResource()
            defer { if accessed { url.stopAccessingSecurityScopedResource() } }
            do {
                let values = try url.resourceValues(forKeys: [
                    .isRegularFileKey,
                    .fileSizeKey,
                    .contentTypeKey,
                ])
                guard values.isRegularFile == true else {
                    errors.append("“\(url.lastPathComponent)”不是可发送的文件")
                    continue
                }
                if let fileSize = values.fileSize {
                    guard fileSize <= maximumAttachmentBytes else {
                        errors.append("“\(url.lastPathComponent)”超过 20 MB")
                        continue
                    }
                    guard totalBytes <= maximumAttachmentBytes - fileSize else {
                        errors.append("附件总大小不能超过 20 MB")
                        continue
                    }
                }
                let remainingBytes = maximumAttachmentBytes - totalBytes
                guard remainingBytes > 0 else {
                    errors.append("附件总大小不能超过 20 MB")
                    break
                }
                let data = try AppBoundedFileReader.read(
                    url,
                    maximumBytes: remainingBytes
                )
                guard totalBytes <= maximumAttachmentBytes - data.count else {
                    errors.append("附件总大小不能超过 20 MB")
                    continue
                }
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
                totalBytes += data.count
            } catch {
                errors.append("无法读取“\(url.lastPathComponent)”：\(error.localizedDescription)")
            }
        }
        return (attachments, errors)
    }

    nonisolated static func loadAttachmentFilesOffMain(
        _ urls: [URL]
    ) async -> (attachments: [ConversationAttachmentDraft], errors: [String]) {
        let task = Task.detached(priority: .userInitiated) {
            loadAttachmentFiles(urls)
        }
        return await withTaskCancellationHandler {
            await task.value
        } onCancel: {
            task.cancel()
        }
    }

    nonisolated private static func attachmentKind(
        mimeType: String
    ) -> ConversationAttachmentKind {
        if mimeType.hasPrefix("image/") { return .image }
        if mimeType.hasPrefix("audio/") { return .audio }
        return .file
    }

    static func pastedName(prefix: String, extension fileExtension: String) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH.mm.ss"
        return "\(prefix) \(formatter.string(from: Date())).\(fileExtension)"
    }
}
