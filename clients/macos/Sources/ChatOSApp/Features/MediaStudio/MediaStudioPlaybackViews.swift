import AppKit
import AVKit
import ChatOSCore
import SwiftUI

struct GeneratedMediaAssetView: View {
    let asset: GeneratedMediaAsset
    var compact = false

    var body: some View {
        Group {
            if let data = asset.base64Data.flatMap({ Data(base64Encoded: $0) }),
               let image = NSImage(data: data) {
                Image(nsImage: image)
                    .resizable()
                    .scaledToFit()
            } else if let url = asset.url {
                if url.isFileURL, let image = NSImage(contentsOf: url) {
                    Image(nsImage: image).resizable().scaledToFit()
                } else {
                    AsyncImage(url: url) { phase in
                        switch phase {
                        case let .success(image): image.resizable().scaledToFit()
                        case .failure: unavailable
                        case .empty: ProgressView()
                        @unknown default: unavailable
                        }
                    }
                }
            } else {
                unavailable
            }
        }
        .frame(maxWidth: .infinity, minHeight: compact ? 82 : 260, maxHeight: compact ? 82 : 620)
        .background(Color.black.opacity(0.035), in: RoundedRectangle(cornerRadius: compact ? 9 : 13))
        .clipShape(RoundedRectangle(cornerRadius: compact ? 9 : 13))
        .overlay {
            RoundedRectangle(cornerRadius: compact ? 9 : 13)
                .stroke(Color.primary.opacity(0.08))
        }
    }

    private var unavailable: some View {
        Image(systemName: "photo.badge.exclamationmark")
            .font(.system(size: compact ? 18 : 32))
            .foregroundStyle(.secondary)
    }
}

struct LocalVideoPlayer: NSViewRepresentable {
    let url: URL

    func makeNSView(context: Context) -> AVPlayerView {
        let view = AVPlayerView()
        view.controlsStyle = .floating
        view.videoGravity = .resizeAspect
        view.player = AVPlayer(url: url)
        return view
    }

    func updateNSView(_ view: AVPlayerView, context: Context) {
        guard let currentURL = (view.player?.currentItem?.asset as? AVURLAsset)?.url,
              currentURL == url else {
            view.player = AVPlayer(url: url)
            return
        }
    }

    static func dismantleNSView(_ view: AVPlayerView, coordinator: ()) {
        view.player?.pause()
        view.player = nil
    }
}

struct LocalVideoPlaylistPlayer: NSViewRepresentable {
    let urls: [URL]

    final class Coordinator {
        var urls: [URL] = []
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> AVPlayerView {
        let view = AVPlayerView()
        view.controlsStyle = .floating
        view.videoGravity = .resizeAspect
        installPlayer(in: view, context: context)
        return view
    }

    func updateNSView(_ view: AVPlayerView, context: Context) {
        guard context.coordinator.urls != urls else { return }
        installPlayer(in: view, context: context)
    }

    private func installPlayer(in view: AVPlayerView, context: Context) {
        view.player?.pause()
        view.player = AVQueuePlayer(items: urls.map { AVPlayerItem(url: $0) })
        context.coordinator.urls = urls
    }

    static func dismantleNSView(_ view: AVPlayerView, coordinator: Coordinator) {
        view.player?.pause()
        view.player = nil
        coordinator.urls = []
    }
}

struct StoryVideoPlaylistPlayer: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    let group: StoryStudioViewModel.CreationHistoryGroup
    private var missingSegmentNumbers: [Int] {
        guard group.totalSegmentCount > 0 else { return [] }
        let completed = Set(group.currentVideos.map(\.segmentNumber))
        return Array(1...group.totalSegmentCount).filter { !completed.contains($0) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(group.projectTitle)
                        .font(.title2.bold())
                    Text(group.isComplete
                         ? appModel.localized("全剧已就绪，共 \(group.currentVideos.count) 段，将按剧情顺序连续播放。",
                                              english: "Full story ready. All \(group.currentVideos.count) segments play in story order.")
                         : appModel.localized("已完成 \(group.currentVideos.count) / \(group.totalSegmentCount) 段，将按剧情顺序播放现有内容。",
                                              english: "\(group.currentVideos.count) of \(group.totalSegmentCount) segments are ready. Available segments play in story order."))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button(appModel.localized("关闭", english: "Close")) { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }

            LocalVideoPlaylistPlayer(urls: group.currentVideos.map(\.fileURL))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color.black)
                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))

            ScrollView(.horizontal) {
                HStack(spacing: 8) {
                    ForEach(group.currentVideos) { item in
                        HStack(spacing: 7) {
                            Text("\(item.segmentNumber)")
                                .font(.caption2.bold().monospacedDigit())
                                .foregroundStyle(.white)
                                .frame(width: 22, height: 22)
                                .background(Color.indigo, in: Circle())
                            Text(item.title)
                                .font(.caption.weight(.medium))
                                .lineLimit(1)
                            Text("\(item.seconds)s")
                                .font(.caption2.monospacedDigit())
                                .foregroundStyle(.secondary)
                        }
                        .padding(.horizontal, 9)
                        .padding(.vertical, 7)
                        .background(Color.primary.opacity(0.045), in: Capsule())
                    }
                }
            }
            .scrollIndicators(.hidden)

            if !missingSegmentNumbers.isEmpty {
                Text(appModel.localized(
                    "尚未完成：第 \(missingSegmentNumbers.map(String.init).joined(separator: "、")) 段",
                    english: "Not ready: segments \(missingSegmentNumbers.map(String.init).joined(separator: ", "))"
                ))
                .font(.caption)
                .foregroundStyle(.orange)
            }
        }
        .padding(20)
        .frame(minWidth: 960, minHeight: 650)
    }
}

struct MediaStudioVideoPreview: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    let item: MediaStudioVideoPreviewRequest

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(item.title ?? appModel.localized("视频预览", english: "Video Preview"))
                        .font(.title2.bold())
                    Text("\(item.modelName) · \(item.createdAt.formatted(date: .abbreviated, time: .shortened))")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button {
                    NSWorkspace.shared.activateFileViewerSelecting([item.fileURL])
                } label: {
                    Label(appModel.localized("在访达中显示", english: "Show in Finder"), systemImage: "folder")
                }
                Button(appModel.localized("关闭", english: "Close")) { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }

            LocalVideoPlayer(url: item.fileURL)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color.black)
                .clipShape(RoundedRectangle(cornerRadius: 12))

            ScrollView {
                Text(item.prompt)
                    .font(.callout)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxHeight: 100)
            .padding(12)
            .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 10))
        }
        .padding(20)
        .frame(minWidth: 900, minHeight: 620)
    }
}
