import AppKit
import ChatOSCore
import ImageIO
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
            if let data {
                AppAsyncDataImage(
                    data: data,
                    identity: "agent-avatar|\(name)",
                    maximumDisplayPixelSize: 256
                ) { image in
                    image.resizable().scaledToFill()
                } placeholder: {
                    fallback
                }
            } else {
                fallback
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
        .accessibilityLabel("\(name)的头像")
    }

    private var fallback: some View {
        Text(String(name.prefix(1)))
            .font(.system(size: max(12, size * 0.34), weight: .semibold, design: .rounded))
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(AppPalette.ai)
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
    @State private var loadTask: Task<Void, Never>?
    @State private var loadGeneration = UUID()

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
        .onDisappear {
            loadGeneration = UUID()
            loadTask?.cancel()
            loadTask = nil
        }
    }

    private func loadFile(_ url: URL) {
        loadTask?.cancel()
        loadGeneration = UUID()
        let generation = loadGeneration
        isLoading = true
        errorMessage = nil
        loadTask = Task {
            do {
                let normalized = try await AppCancellableDetachedWork.run {
                    let accessed = url.startAccessingSecurityScopedResource()
                    defer { if accessed { url.stopAccessingSecurityScopedResource() } }
                    let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
                    guard values.isRegularFile == true, (values.fileSize ?? 0) <= 20 * 1_024 * 1_024 else {
                        throw AgentAvatarImageError.invalidImage
                    }
                    return try AgentAvatarImageProcessor.normalize(
                        try AppBoundedFileReader.read(
                            url,
                            maximumBytes: 20 * 1_024 * 1_024
                        )
                    )
                }
                guard loadGeneration == generation, !Task.isCancelled else { return }
                avatarData = normalized
            } catch {
                guard loadGeneration == generation, !Task.isCancelled else { return }
                errorMessage = error.localizedDescription
            }
            guard loadGeneration == generation else { return }
            isLoading = false
            loadTask = nil
        }
    }
}

private struct AgentGeneratedAvatarPicker: View {
    @Environment(\.dismiss) private var dismiss
    let images: [GeneratedMediaAsset]
    let onSelect: (Data) -> Void

    @State private var loadingID: String?
    @State private var errorMessage: String?
    @State private var loadTask: Task<Void, Never>?
    @State private var loadGeneration = UUID()

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
        .onDisappear {
            loadGeneration = UUID()
            loadTask?.cancel()
            loadTask = nil
        }
    }

    private var pickerHeight: CGFloat {
        let rows = max(1, Int(ceil(Double(images.count) / 3.0)))
        return min(520, max(300, CGFloat(rows * 160 + 150)))
    }

    private func select(_ asset: GeneratedMediaAsset) {
        loadTask?.cancel()
        loadGeneration = UUID()
        let generation = loadGeneration
        loadingID = asset.id
        errorMessage = nil
        loadTask = Task {
            do {
                let source = try await MediaStudioImageLoader.data(for: asset)
                let normalized = try await AppCancellableDetachedWork.run {
                    try AgentAvatarImageProcessor.normalize(source)
                }
                guard loadGeneration == generation, !Task.isCancelled else { return }
                onSelect(normalized)
                dismiss()
            } catch {
                guard loadGeneration == generation, !Task.isCancelled else { return }
                errorMessage = error.localizedDescription
                loadingID = nil
            }
            if loadGeneration == generation { loadTask = nil }
        }
    }
}

enum AgentAvatarImageProcessor {
    static let maximumInputBytes = 20 * 1_024 * 1_024
    static let maximumSourcePixelCount = 64_000_000
    static let maximumDecodePixelSize = 2_048
    static let outputPixelSize = 256
    static let maximumOutputBytes = 512 * 1_024

    static func normalize(_ data: Data) throws -> Data {
        try Task.checkCancellation()
        guard !data.isEmpty,
              data.count <= maximumInputBytes,
              let source = CGImageSourceCreateWithData(data as CFData, [
                kCGImageSourceShouldCache: false,
              ] as CFDictionary),
              CGImageSourceGetCount(source) > 0,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil)
                as? [CFString: Any],
              let widthValue = properties[kCGImagePropertyPixelWidth] as? NSNumber,
              let heightValue = properties[kCGImagePropertyPixelHeight] as? NSNumber else {
            throw AgentAvatarImageError.invalidImage
        }
        try Task.checkCancellation()
        let sourceWidth = widthValue.doubleValue
        let sourceHeight = heightValue.doubleValue
        guard sourceWidth.isFinite, sourceHeight.isFinite,
              sourceWidth > 0, sourceHeight > 0,
              sourceWidth * sourceHeight <= Double(maximumSourcePixelCount),
              let decoded = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maximumDecodePixelSize,
                kCGImageSourceShouldCacheImmediately: true,
              ] as CFDictionary) else {
            throw AgentAvatarImageError.invalidImage
        }
        try Task.checkCancellation()
        let side = min(decoded.width, decoded.height)
        let cropRect = CGRect(
            x: (decoded.width - side) / 2,
            y: (decoded.height - side) / 2,
            width: side,
            height: side
        )
        guard let cropped = decoded.cropping(to: cropRect),
              let context = CGContext(
                data: nil,
                width: outputPixelSize,
                height: outputPixelSize,
                bitsPerComponent: 8,
                bytesPerRow: outputPixelSize * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              ) else {
            throw AgentAvatarImageError.cannotProcess
        }
        context.interpolationQuality = .high
        context.draw(cropped, in: CGRect(
            x: 0,
            y: 0,
            width: outputPixelSize,
            height: outputPixelSize
        ))
        try Task.checkCancellation()
        guard let outputImage = context.makeImage() else {
            throw AgentAvatarImageError.cannotProcess
        }

        let encoded = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            encoded,
            "public.jpeg" as CFString,
            1,
            nil
        ) else {
            throw AgentAvatarImageError.cannotProcess
        }
        CGImageDestinationAddImage(destination, outputImage, [
            kCGImageDestinationLossyCompressionQuality: 0.86,
        ] as CFDictionary)
        try Task.checkCancellation()
        guard CGImageDestinationFinalize(destination),
              encoded.length > 0,
              encoded.length <= maximumOutputBytes else {
            throw AgentAvatarImageError.cannotProcess
        }
        try Task.checkCancellation()
        return Data(referencing: encoded)
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
