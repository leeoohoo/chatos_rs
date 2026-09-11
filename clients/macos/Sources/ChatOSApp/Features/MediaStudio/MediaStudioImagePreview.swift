import AppKit
import ChatOSCore
import SwiftUI

struct MediaStudioImagePreviewRequest: Identifiable {
    let id = UUID()
    let images: [GeneratedMediaAsset]
    var selectedIndex = 0
}

struct MediaStudioImagePreview: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    let request: MediaStudioImagePreviewRequest
    let useForVideo: ((GeneratedMediaAsset) -> Void)?
    @State private var index: Int
    @State private var image: NSImage?
    @State private var errorMessage: String?
    @State private var scale: CGFloat?
    @GestureState private var pinchScale: CGFloat = 1

    init(request: MediaStudioImagePreviewRequest, useForVideo: @escaping (GeneratedMediaAsset) -> Void) {
        self.request = request
        self.useForVideo = useForVideo
        _index = State(initialValue: min(max(0, request.selectedIndex), max(0, request.images.count - 1)))
    }

    init(request: MediaStudioImagePreviewRequest) {
        self.request = request
        self.useForVideo = nil
        _index = State(initialValue: min(max(0, request.selectedIndex), max(0, request.images.count - 1)))
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Text(appModel.localized("图片预览", english: "Image Preview"))
                    .font(.headline)
                if request.images.count > 1 {
                    Button { index -= 1 } label: { Image(systemName: "chevron.left") }
                        .disabled(index == 0)
                        .keyboardShortcut(.leftArrow, modifiers: [])
                        .help(appModel.localized("上一张", english: "Previous Image"))
                    Text("\(index + 1) / \(request.images.count)").monospacedDigit()
                    Button { index += 1 } label: { Image(systemName: "chevron.right") }
                        .disabled(index + 1 >= request.images.count)
                        .keyboardShortcut(.rightArrow, modifiers: [])
                        .help(appModel.localized("下一张", english: "Next Image"))
                }
                Spacer()
                if let useForVideo {
                    Button {
                        guard request.images.indices.contains(index) else { return }
                        useForVideo(request.images[index])
                        dismiss()
                    } label: {
                        Label(appModel.localized("用作视频首帧", english: "Use as Video First Frame"), systemImage: "video.badge.plus")
                    }
                    .disabled(image == nil)
                    .buttonStyle(.borderedProminent)
                    .tint(.purple)
                }
                Button(appModel.localized("关闭", english: "Close")) { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(16)
            Divider()
            GeometryReader { geometry in
                if let image {
                    zoomableImage(image, viewport: geometry.size)
                } else if let errorMessage {
                    ContentUnavailableView {
                        Label(appModel.localized("无法预览图片", english: "Cannot Preview Image"), systemImage: "photo.badge.exclamationmark")
                    } description: {
                        Text(errorMessage)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
        }
        .frame(width: min(960, (NSScreen.main?.visibleFrame.width ?? 1080) - 100),
               height: min(740, (NSScreen.main?.visibleFrame.height ?? 900) - 140))
        .task(id: index) {
            image = nil
            errorMessage = nil
            scale = nil
            guard request.images.indices.contains(index) else { return }
            do {
                let data = try await MediaStudioImageLoader.data(for: request.images[index])
                try Task.checkCancellation()
                guard let decoded = NSImage(data: data), decoded.isValid else {
                    throw MediaStudioImageLoader.ImageError.invalidImage
                }
                if let rep = decoded.representations.first, rep.pixelsWide > 0, rep.pixelsHigh > 0 {
                    decoded.size = NSSize(width: rep.pixelsWide, height: rep.pixelsHigh)
                }
                image = decoded
            } catch {
                guard !Task.isCancelled else { return }
                errorMessage = error.localizedDescription
            }
        }
    }

    private func zoomableImage(_ image: NSImage, viewport: CGSize) -> some View {
        let canvas = CGSize(width: viewport.width, height: max(1, viewport.height - 52))
        let fit = min(canvas.width / max(1, image.size.width), canvas.height / max(1, image.size.height), 1)
        let baseScale = scale ?? fit
        let effectiveScale = min(8, max(0.05, baseScale * pinchScale))
        return VStack(spacing: 0) {
            ScrollView([.horizontal, .vertical]) {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .frame(width: image.size.width * effectiveScale, height: image.size.height * effectiveScale)
                    .frame(minWidth: canvas.width, minHeight: canvas.height)
                    .accessibilityLabel(appModel.localized("生成的图片", english: "Generated Image"))
            }
            .background(Color.black.opacity(0.9))
            .gesture(MagnificationGesture()
                .updating($pinchScale) { value, state, _ in state = value }
                .onEnded { value in scale = min(8, max(0.05, baseScale * value)) })
            HStack(spacing: 14) {
                Text("\(Int(image.size.width)) × \(Int(image.size.height))")
                    .foregroundStyle(.secondary)
                Spacer()
                Button { scale = max(0.05, baseScale / 1.25) } label: { Image(systemName: "minus.magnifyingglass") }
                    .help(appModel.localized("缩小", english: "Zoom Out"))
                Text("\(Int(effectiveScale * 100))%").monospacedDigit().frame(width: 48)
                Button { scale = min(8, baseScale * 1.25) } label: { Image(systemName: "plus.magnifyingglass") }
                    .help(appModel.localized("放大", english: "Zoom In"))
                Button("100%") { scale = 1 }
                Button(appModel.localized("适应窗口", english: "Fit to Window")) { scale = nil }
            }
            .buttonStyle(.borderless)
            .padding(.horizontal, 16)
            .frame(height: 52)
        }
    }
}

struct MediaStudioGeneratedImagePicker: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: MediaStudioViewModel
    @State private var preview: MediaStudioImagePreviewRequest?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(appModel.localized("选择已生成的图片", english: "Choose a Generated Image")).font(.headline)
                    Text(appModel.localized("选作视频首帧；不会自动开始生成", english: "Use as the first frame; generation will not start automatically"))
                        .font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button(appModel.localized("取消", english: "Cancel")) { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(18)
            Divider()
            if viewModel.history.isEmpty {
                ContentUnavailableView {
                    Label(appModel.localized("还没有生成的图片", english: "No Generated Images Yet"), systemImage: "photo.on.rectangle.angled")
                } description: {
                    Text(appModel.localized("先生成图片，或返回视频页从本机上传参考图。", english: "Generate an image first, or return to upload a reference from your computer."))
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 20) {
                        ForEach(viewModel.history) { record in
                            VStack(alignment: .leading, spacing: 8) {
                                Text(record.prompt).font(.system(size: 12, weight: .medium)).lineLimit(2)
                                Text("\(record.modelName) · \(record.createdAt.formatted(date: .abbreviated, time: .shortened))")
                                    .font(.caption).foregroundStyle(.secondary)
                                LazyVGrid(columns: [GridItem(.adaptive(minimum: 160), spacing: 12)], spacing: 12) {
                                    ForEach(record.images) { asset in
                                        VStack(spacing: 8) {
                                            Button {
                                                preview = .init(images: record.images, selectedIndex: record.images.firstIndex(of: asset) ?? 0)
                                            } label: {
                                                GeneratedMediaAssetView(asset: asset, compact: true)
                                            }
                                            .buttonStyle(.plain)
                                            .help(appModel.localized("放大查看图片", english: "Enlarge Image"))
                                            Button(appModel.localized("用作首帧", english: "Use as First Frame")) { select(asset) }
                                                .buttonStyle(.bordered)
                                                .tint(.purple)
                                        }
                                        .padding(10)
                                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                                    }
                                }
                            }
                        }
                    }
                    .padding(18)
                }
            }
        }
        .frame(width: min(760, (NSScreen.main?.visibleFrame.width ?? 900) - 100),
               height: min(620, (NSScreen.main?.visibleFrame.height ?? 900) - 140))
        .sheet(item: $preview) { request in
            MediaStudioImagePreview(request: request) { asset in select(asset) }
                .environmentObject(appModel)
        }
    }

    private func select(_ asset: GeneratedMediaAsset) {
        viewModel.useGeneratedImageForVideo(asset)
        dismiss()
    }
}

struct MediaStudioGeneratedReferencePicker: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: MediaStudioViewModel
    @State private var selectedIDs: Set<String> = []
    @State private var preview: MediaStudioImagePreviewRequest?

    private var selectedAssets: [GeneratedMediaAsset] {
        viewModel.history.flatMap(\.images).filter { selectedIDs.contains($0.id) }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(appModel.localized("选择参考图", english: "Choose Reference Images")).font(.headline)
                    Text(appModel.localized("可从生成记录选择多张，按页面顺序加入；总数最多 8 张。",
                                            english: "Select multiple generated images. They are added in display order, up to 8 total."))
                        .font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Text("\(selectedAssets.count) / \(max(0, 8 - viewModel.inputImages.count))")
                    .font(.caption.monospacedDigit()).foregroundStyle(.purple)
                Button(appModel.localized("取消", english: "Cancel")) { dismiss() }
                Button(appModel.localized("添加所选", english: "Add Selected")) {
                    viewModel.addGeneratedImagesAsReferences(selectedAssets)
                    dismiss()
                }
                .buttonStyle(.borderedProminent).tint(.purple)
                .disabled(selectedAssets.isEmpty || selectedAssets.count + viewModel.inputImages.count > 8)
            }.padding(18)
            Divider()
            if viewModel.history.isEmpty {
                ContentUnavailableView(appModel.localized("还没有生成记录", english: "No Generated Images Yet"),
                                       systemImage: "photo.on.rectangle.angled")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 20) {
                        ForEach(viewModel.history) { record in
                            VStack(alignment: .leading, spacing: 8) {
                                Text(record.prompt).font(.system(size: 12, weight: .medium)).lineLimit(2)
                                Text("\(record.modelName) · \(record.createdAt.formatted(date: .abbreviated, time: .shortened))")
                                    .font(.caption).foregroundStyle(.secondary)
                                LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 12)], spacing: 12) {
                                    ForEach(record.images) { asset in referenceCard(asset, siblings: record.images) }
                                }
                            }
                        }
                    }.padding(18)
                }
            }
        }
        .frame(width: min(820, (NSScreen.main?.visibleFrame.width ?? 980) - 100),
               height: min(660, (NSScreen.main?.visibleFrame.height ?? 900) - 140))
        .sheet(item: $preview) { request in
            MediaStudioImagePreview(request: request).environmentObject(appModel)
        }
    }

    private func referenceCard(_ asset: GeneratedMediaAsset, siblings: [GeneratedMediaAsset]) -> some View {
        let selected = selectedIDs.contains(asset.id)
        let atLimit = selectedAssets.count >= max(0, 8 - viewModel.inputImages.count)
        return VStack(spacing: 8) {
            Button {
                if selected { selectedIDs.remove(asset.id) }
                else if !atLimit { selectedIDs.insert(asset.id) }
            } label: {
                ZStack(alignment: .topTrailing) {
                    GeneratedMediaAssetView(asset: asset, compact: true)
                    Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                        .font(.system(size: 20, weight: .semibold))
                        .foregroundStyle(selected ? Color.purple : Color.secondary)
                        .background(.regularMaterial, in: Circle()).padding(7)
                }
                .overlay(RoundedRectangle(cornerRadius: 10).stroke(selected ? Color.purple : Color.clear, lineWidth: 2))
            }.buttonStyle(.plain).disabled(!selected && atLimit)
            Button(appModel.localized("放大", english: "Preview")) {
                preview = .init(images: siblings, selectedIndex: siblings.firstIndex(of: asset) ?? 0)
            }.buttonStyle(.borderless).font(.caption)
        }
    }
}
