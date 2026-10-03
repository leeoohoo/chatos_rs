import AppKit
import ChatOSCore
import CryptoKit
import SwiftUI

struct ComposerAttachmentChip: View {
    let attachment: ConversationAttachmentDraft
    let onPreview: () -> Void
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Button(action: onPreview) {
                HStack(spacing: 8) {
                    thumbnail
                    VStack(alignment: .leading, spacing: 1) {
                        Text(attachment.name)
                            .lineLimit(1)
                            .appFont(.caption.weight(.medium))
                        Text(detailText)
                            .lineLimit(1)
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .buttonStyle(.plain)

            Button("移除附件", systemImage: "xmark.circle.fill", action: onRemove)
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
        }
        .padding(.leading, 7)
        .padding(.trailing, 6)
        .padding(.vertical, 6)
        .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 9))
        .overlay {
            RoundedRectangle(cornerRadius: 9)
                .stroke(AppPalette.border, lineWidth: 1)
        }
    }

    @ViewBuilder
    private var thumbnail: some View {
        if attachment.kind == .image {
            AppAsyncDataImage(
                data: attachment.data,
                identity: "composer-chip|\(attachment.id)",
                maximumDisplayPixelSize: 96
            ) { image in
                image
                    .resizable()
                    .scaledToFill()
                    .frame(width: 32, height: 32)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            } placeholder: {
                attachmentPlaceholder
            }
        } else {
            attachmentPlaceholder
        }
    }

    private var attachmentPlaceholder: some View {
        Image(systemName: attachmentIcon(
            kind: attachment.kind,
            mimeType: attachment.mimeType,
            origin: attachment.origin
        ))
        .foregroundStyle(AppPalette.ai)
        .frame(width: 32, height: 32)
        .background(AppPalette.ai.opacity(0.09), in: RoundedRectangle(cornerRadius: 6))
    }

    private var detailText: String {
        if attachment.origin == .pastedText,
           let text = String(data: attachment.data, encoding: .utf8) {
            return "长文本 · \(text.count) 字"
        }
        return formatAttachmentSize(attachment.size)
    }
}

struct MessageAttachmentChips: View {
    let attachments: [ConversationAttachmentReference]
    @State private var previewedImage: ConversationAttachmentReference?

    var body: some View {
        AttachmentFlowLayout(spacing: 7) {
            ForEach(attachments) { attachment in
                if attachment.kind == .image,
                   attachment.localURL?.isFileURL == true {
                    MessageInlineImage(
                        attachment: attachment,
                        onPreview: { previewedImage = attachment }
                    )
                } else {
                    MessageFileAttachmentChip(attachment: attachment)
                }
            }
        }
        .sheet(item: $previewedImage) { attachment in
            MessageLocalImagePreview(attachment: attachment)
        }
    }
}

private struct MessageInlineImage: View {
    let attachment: ConversationAttachmentReference
    let onPreview: () -> Void

    var body: some View {
        Button(action: onPreview) {
            LocalAttachmentImage(
                url: attachment.localURL,
                sha256: attachment.sha256,
                expectedBytes: attachment.size
            )
                .frame(width: 360, height: 220)
                .background(AppPalette.inputSurface)
                .clipShape(RoundedRectangle(cornerRadius: 10))
                .overlay {
                    RoundedRectangle(cornerRadius: 10)
                        .stroke(AppPalette.border, lineWidth: 1)
                }
        }
        .buttonStyle(.plain)
        .help("点击查看大图：\(attachment.name)")
    }

}

private struct MessageFileAttachmentChip: View {
    let attachment: ConversationAttachmentReference

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: attachmentIcon(
                kind: attachment.kind,
                mimeType: attachment.mimeType,
                origin: nil
            ))
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
            RoundedRectangle(cornerRadius: 8).stroke(AppPalette.border, lineWidth: 1)
        }
    }
}

private struct MessageLocalImagePreview: View {
    let attachment: ConversationAttachmentReference
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(attachment.name)
                    .appFont(.headline)
                Spacer()
                Button("关闭", action: dismiss.callAsFunction)
                    .keyboardShortcut(.cancelAction)
            }
            .padding(16)
            Divider()
            ScrollView([.horizontal, .vertical]) {
                LocalAttachmentImage(
                    url: attachment.localURL,
                    sha256: attachment.sha256,
                    expectedBytes: attachment.size
                )
                    .padding(20)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(minWidth: 760, idealWidth: 920, minHeight: 560, idealHeight: 720)
    }
}

private struct LocalAttachmentImage: View {
    let url: URL?
    let sha256: String?
    let expectedBytes: Int
    @State private var image: NSImage?
    @State private var loadFailed = false

    var body: some View {
        Group {
            if let image {
                Image(nsImage: image)
                    .resizable()
                    .scaledToFit()
            } else if loadFailed {
                ContentUnavailableView(
                    "图片加载失败",
                    systemImage: "exclamationmark.triangle"
                )
            } else {
                ZStack {
                    Image(systemName: "photo")
                        .appFont(.system(size: 24))
                        .foregroundStyle(.secondary)
                    ProgressView()
                        .controlSize(.small)
                }
            }
        }
        .task(id: url) {
            image = nil
            loadFailed = false
            guard let url, url.isFileURL else {
                loadFailed = true
                return
            }
            do {
                let decoded = try await AppCancellableDetachedWork.run {
                    guard expectedBytes > 0, expectedBytes <= 20 * 1_024 * 1_024 else {
                        throw CocoaError(.fileReadTooLarge)
                    }
                    let data = try AppBoundedFileReader.read(
                        url,
                        maximumBytes: expectedBytes
                    )
                    guard data.count == expectedBytes,
                          let sha256,
                          SHA256.hash(data: data).map({
                              String(format: "%02x", $0)
                          }).joined() == sha256.lowercased() else {
                        throw CocoaError(.fileReadCorruptFile)
                    }
                    guard let decoded = AppImageThumbnailLoader.decode(
                        data,
                        maximumSourcePixelCount: 64_000_000,
                        maximumDisplayPixelSize: 2_048
                    ) else {
                        throw CocoaError(.fileReadCorruptFile)
                    }
                    return decoded
                }
                guard !Task.isCancelled else { return }
                image = NSImage(cgImage: decoded.image, size: .zero)
            } catch {
                if !Task.isCancelled { loadFailed = true }
            }
        }
    }
}

struct ComposerAttachmentPreview: View {
    let attachment: ConversationAttachmentDraft
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(attachment.name)
                        .appFont(.headline)
                    Text("\(attachment.mimeType) · \(formatAttachmentSize(attachment.size))")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("关闭", action: dismiss.callAsFunction)
                    .keyboardShortcut(.cancelAction)
            }
            .padding(16)
            Divider()
            preview
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(minWidth: 620, idealWidth: 760, minHeight: 480, idealHeight: 620)
    }

    @ViewBuilder
    private var preview: some View {
        if attachment.kind == .image {
            ScrollView([.horizontal, .vertical]) {
                AppAsyncDataImage(
                    data: attachment.data,
                    identity: "composer-preview|\(attachment.id)",
                    maximumDisplayPixelSize: 2_048
                ) { image in
                    image.resizable().scaledToFit().padding(20)
                } placeholder: {
                    ProgressView().padding(20)
                }
            }
        } else if isTextLike(attachment.mimeType),
                  let text = String(data: attachment.data, encoding: .utf8) {
            ScrollView {
                Text(text)
                    .appFont(.system(.body, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(20)
            }
        } else {
            ContentUnavailableView(
                "文件已准备发送",
                systemImage: attachmentIcon(
                    kind: attachment.kind,
                    mimeType: attachment.mimeType,
                    origin: attachment.origin
                ),
                description: Text("该格式将在发送时上传，由 ChatOS 后端读取和处理。")
            )
        }
    }
}

private struct AttachmentFlowLayout: Layout {
    var spacing: CGFloat

    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) -> CGSize {
        arrange(proposal: proposal, subviews: subviews).size
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) {
        let arrangement = arrange(proposal: proposal, subviews: subviews)
        for (index, point) in arrangement.points.enumerated() {
            subviews[index].place(
                at: CGPoint(x: bounds.minX + point.x, y: bounds.minY + point.y),
                proposal: .unspecified
            )
        }
    }

    private func arrange(
        proposal: ProposedViewSize,
        subviews: Subviews
    ) -> (size: CGSize, points: [CGPoint]) {
        let maximumWidth = proposal.width ?? .greatestFiniteMagnitude
        var x: CGFloat = 0
        var y: CGFloat = 0
        var rowHeight: CGFloat = 0
        var points: [CGPoint] = []
        var usedWidth: CGFloat = 0

        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > 0, x + size.width > maximumWidth {
                x = 0
                y += rowHeight + spacing
                rowHeight = 0
            }
            points.append(CGPoint(x: x, y: y))
            x += size.width + spacing
            usedWidth = max(usedWidth, x - spacing)
            rowHeight = max(rowHeight, size.height)
        }
        return (CGSize(width: usedWidth, height: y + rowHeight), points)
    }
}

private func attachmentIcon(
    kind: ConversationAttachmentKind,
    mimeType: String,
    origin: ConversationAttachmentOrigin?
) -> String {
    if origin == .pastedText { return "text.alignleft" }
    if kind == .image { return "photo" }
    if kind == .audio { return "waveform" }
    if mimeType == "application/pdf" { return "doc.richtext" }
    if mimeType.contains("wordprocessingml") { return "doc.text" }
    if isTextLike(mimeType) { return "doc.plaintext" }
    return "doc"
}

private func isTextLike(_ mimeType: String) -> Bool {
    mimeType.hasPrefix("text/")
        || mimeType == "application/json"
        || mimeType == "application/xml"
        || mimeType == "application/x-yaml"
}

private func formatAttachmentSize(_ bytes: Int) -> String {
    let formatter = ByteCountFormatter()
    formatter.countStyle = .file
    return formatter.string(fromByteCount: Int64(bytes))
}
