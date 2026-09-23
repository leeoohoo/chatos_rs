import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

enum AgentAvatarMetrics {
    static let navigation: CGFloat = 50
    static let message: CGFloat = 68
    static let header: CGFloat = 81
    static let managementCard: CGFloat = 95
    static let editorPreview: CGFloat = 104
}

struct AgentAvatarView: View {
    let name: String
    let data: Data?
    var size: CGFloat = 63
    var cornerRadius: CGFloat = 19

    var body: some View {
        Group {
            if let data, let image = NSImage(data: data) {
                Image(nsImage: image)
                    .resizable()
                    .scaledToFill()
            } else {
                Text(String(name.prefix(1)))
                    .font(.system(size: max(12, size * 0.34), weight: .semibold, design: .rounded))
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(AppPalette.ai)
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
        .accessibilityLabel("\(name)的头像")
    }
}

struct AgentAvatarEditor: View {
    @Binding var avatarData: Data?
    let agentName: String
    let generatedImages: [GeneratedMediaAsset]

    @State private var showsFileImporter = false
    @State private var showsGeneratedPicker = false
    @State private var isLoading = false
    @State private var errorMessage: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 14) {
                AgentAvatarView(
                    name: agentName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        ? "A" : agentName,
                    data: avatarData,
                    size: AgentAvatarMetrics.editorPreview,
                    cornerRadius: 30
                )

                VStack(alignment: .leading, spacing: 7) {
                    HStack(spacing: 8) {
                        Button("从本机选择", systemImage: "photo") {
                            showsFileImporter = true
                        }
                        Button("从生成记录选择", systemImage: "sparkles") {
                            showsGeneratedPicker = true
                        }
                        .disabled(generatedImages.isEmpty)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)

                    if avatarData != nil {
                        Button("使用默认头像", systemImage: "arrow.uturn.backward") {
                            avatarData = nil
                            errorMessage = nil
                        }
                        .buttonStyle(.plain)
                        .foregroundStyle(.secondary)
                        .controlSize(.small)
                    } else if generatedImages.isEmpty {
                        Text("暂无图片生成记录，仍可从本机选择。")
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                if isLoading {
                    ProgressView().controlSize(.small)
                }
            }

            if let errorMessage {
                Text(errorMessage)
                    .appFont(.caption)
                    .foregroundStyle(.red)
            }
        }
        .fileImporter(
            isPresented: $showsFileImporter,
            allowedContentTypes: [.image],
            allowsMultipleSelection: false
        ) { result in
            switch result {
            case let .success(urls):
                guard let url = urls.first else { return }
                loadFile(url)
            case let .failure(error):
                errorMessage = error.localizedDescription
            }
        }
        .sheet(isPresented: $showsGeneratedPicker) {
            AgentGeneratedAvatarPicker(
                images: generatedImages,
                onSelect: { avatarData = $0 }
            )
        }
    }

    private func loadFile(_ url: URL) {
        isLoading = true
        errorMessage = nil
        Task {
            do {
                let normalized = try await Task.detached(priority: .userInitiated) {
                    let accessed = url.startAccessingSecurityScopedResource()
                    defer { if accessed { url.stopAccessingSecurityScopedResource() } }
                    let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
                    guard values.isRegularFile == true, (values.fileSize ?? 0) <= 20 * 1_024 * 1_024 else {
                        throw AgentAvatarImageError.invalidImage
                    }
                    return try AgentAvatarImageProcessor.normalize(try Data(contentsOf: url))
                }.value
                avatarData = normalized
            } catch {
                errorMessage = error.localizedDescription
            }
            isLoading = false
        }
    }
}

private struct AgentGeneratedAvatarPicker: View {
    @Environment(\.dismiss) private var dismiss
    let images: [GeneratedMediaAsset]
    let onSelect: (Data) -> Void

    @State private var loadingID: String?
    @State private var errorMessage: String?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text("选择头像")
                        .appFont(.headline.weight(.semibold))
                    Text("从图片生成记录复制一张图片作为头像。")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("取消") { dismiss() }
            }
            .padding(18)

            Divider()

            ScrollView {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 132, maximum: 180), spacing: 12)],
                    spacing: 12
                ) {
                    ForEach(images) { asset in
                        Button {
                            select(asset)
                        } label: {
                            ZStack {
                                GeneratedMediaAssetView(asset: asset, compact: true)
                                    .frame(height: 132)
                                    .frame(maxWidth: .infinity)
                                    .clipped()
                                    .clipShape(RoundedRectangle(cornerRadius: 12))
                                if loadingID == asset.id {
                                    ProgressView()
                                        .padding(12)
                                        .background(.regularMaterial, in: Circle())
                                }
                            }
                        }
                        .buttonStyle(.plain)
                        .disabled(loadingID != nil)
                    }
                }
                .padding(18)
            }

            if let errorMessage {
                Divider()
                Text(errorMessage)
                    .appFont(.caption)
                    .foregroundStyle(.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(14)
            }
        }
        .frame(width: 650, height: pickerHeight)
    }

    private var pickerHeight: CGFloat {
        let rows = max(1, Int(ceil(Double(images.count) / 3.0)))
        return min(520, max(300, CGFloat(rows * 160 + 150)))
    }

    private func select(_ asset: GeneratedMediaAsset) {
        loadingID = asset.id
        errorMessage = nil
        Task {
            do {
                let source = try await MediaStudioImageLoader.data(for: asset)
                let normalized = try await Task.detached(priority: .userInitiated) {
                    try AgentAvatarImageProcessor.normalize(source)
                }.value
                onSelect(normalized)
                dismiss()
            } catch {
                errorMessage = error.localizedDescription
                loadingID = nil
            }
        }
    }
}

enum AgentAvatarImageProcessor {
    static func normalize(_ data: Data) throws -> Data {
        guard !data.isEmpty, data.count <= 20 * 1_024 * 1_024,
              let source = NSImage(data: data), source.isValid else {
            throw AgentAvatarImageError.invalidImage
        }

        let sourceSize = source.size
        guard sourceSize.width > 0, sourceSize.height > 0 else {
            throw AgentAvatarImageError.invalidImage
        }
        let side = min(sourceSize.width, sourceSize.height)
        let sourceRect = NSRect(
            x: (sourceSize.width - side) / 2,
            y: (sourceSize.height - side) / 2,
            width: side,
            height: side
        )
        let targetSize = NSSize(width: 256, height: 256)
        let target = NSImage(size: targetSize)
        target.lockFocus()
        NSColor.clear.setFill()
        NSRect(origin: .zero, size: targetSize).fill()
        NSGraphicsContext.current?.imageInterpolation = .high
        source.draw(
            in: NSRect(origin: .zero, size: targetSize),
            from: sourceRect,
            operation: .sourceOver,
            fraction: 1
        )
        target.unlockFocus()

        guard let tiff = target.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiff),
              let result = bitmap.representation(
                using: .jpeg,
                properties: [.compressionFactor: 0.86]
              ),
              result.count <= 512 * 1_024 else {
            throw AgentAvatarImageError.cannotProcess
        }
        return result
    }
}

enum AgentAvatarImageError: LocalizedError {
    case invalidImage
    case cannotProcess

    var errorDescription: String? {
        switch self {
        case .invalidImage: "无法读取这张图片，请选择 20 MB 以内的有效图片。"
        case .cannotProcess: "无法生成头像，请换一张图片重试。"
        }
    }
}
