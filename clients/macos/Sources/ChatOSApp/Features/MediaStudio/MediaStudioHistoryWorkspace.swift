import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

extension MediaStudioView {
    var historyWorkspace: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(appModel.localized("创作记录", english: "Creation History"))
                        .font(.system(size: 24, weight: .semibold, design: .rounded))
                    Text(appModel.localized(
                        "已自动保存到本机，重启后仍可查看",
                        english: "Saved locally and available after restarting the app"
                    ))
                        .foregroundStyle(.secondary)
                }

                if let message = viewModel.historyErrorMessage {
                    Label(message, systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.orange)
                        .textSelection(.enabled)
                }

                if viewModel.isLoadingHistory || stories.isLoading {
                    ProgressView(appModel.localized("正在加载记录", english: "Loading history"))
                        .frame(maxWidth: .infinity, minHeight: 200)
                } else if viewModel.history.isEmpty && viewModel.videoHistory.isEmpty
                            && stories.creationHistoryGroups.isEmpty {
                    ContentUnavailableView(
                        appModel.localized("还没有创作记录", english: "No Creation History"),
                        systemImage: "clock.arrow.circlepath"
                    )
                    .frame(maxWidth: .infinity, minHeight: 420)
                } else {
                    storyHistorySection
                    videoHistorySection
                    imageHistorySection
                }
            }
            .padding(24)
        }
    }

    @ViewBuilder
    var storyHistorySection: some View {
        if !stories.creationHistoryGroups.isEmpty {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(appModel.localized("剧情作品", english: "Story Projects"))
                        .font(.system(size: 17, weight: .semibold))
                    Text(appModel.localized(
                        "图片和视频按所属剧情集中展示，分段视频可按顺序连续播放。",
                        english: "Images and videos are grouped by story, with sequential playback for segments."
                    ))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                Spacer()
            }

            VStack(spacing: 16) {
                ForEach(stories.creationHistoryGroups) { group in
                    storyHistoryCard(group)
                }
            }
        }
    }

    func storyHistoryCard(_ group: StoryStudioViewModel.CreationHistoryGroup) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 13) {
                ZStack {
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(LinearGradient(colors: [.indigo.opacity(0.18), .purple.opacity(0.1)],
                                             startPoint: .topLeading, endPoint: .bottomTrailing))
                    Image(systemName: "film.stack.fill")
                        .font(.system(size: 18, weight: .semibold))
                        .foregroundStyle(.indigo)
                }
                .frame(width: 44, height: 44)

                VStack(alignment: .leading, spacing: 4) {
                    Text(group.projectTitle)
                        .font(.system(size: 15, weight: .semibold))
                        .lineLimit(1)
                    HStack(spacing: 10) {
                        Label("\(group.images.count) " + appModel.localized("张图片", english: "images"),
                              systemImage: "photo")
                        Label("\(group.currentVideos.count) / \(group.totalSegmentCount) "
                              + appModel.localized("段当前成片 · \(group.videos.count) 个版本", english: "current segments · \(group.videos.count) versions"),
                              systemImage: "play.rectangle")
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                Spacer()
                if !group.currentVideos.isEmpty {
                    Button { playlistPreview = group } label: {
                        Label(group.isComplete
                              ? appModel.localized("全剧连播", english: "Play Full Story")
                              : appModel.localized("连续播放已完成分段", english: "Play Completed Segments"),
                              systemImage: "play.fill")
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(.indigo)
                }
            }

            if !group.images.isEmpty {
                VStack(alignment: .leading, spacing: 9) {
                    Text(appModel.localized("剧情图片", english: "Story Images"))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    ScrollView(.horizontal) {
                        HStack(spacing: 11) {
                            ForEach(Array(group.images.enumerated()), id: \.element.id) { index, item in
                                storyImageCard(item, images: group.images, index: index)
                            }
                        }
                    }
                    .scrollIndicators(.hidden)
                }
            }

            if !group.videos.isEmpty {
                VStack(alignment: .leading, spacing: 9) {
                    Text(appModel.localized("分段视频", english: "Segment Videos"))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 330), spacing: 11)], spacing: 11) {
                        ForEach(group.videos) { item in
                            storyVideoCard(item)
                        }
                    }
                }
            }
        }
        .padding(16)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .stroke(Color.indigo.opacity(0.14))
        }
        .shadow(color: .black.opacity(0.035), radius: 10, y: 4)
    }

    func storyImageCard(_ item: StoryStudioViewModel.CreationHistoryImage,
                                images: [StoryStudioViewModel.CreationHistoryImage], index: Int) -> some View {
        Button {
            imagePreview = .init(images: images.map(\.asset), selectedIndex: index)
        } label: {
            VStack(alignment: .leading, spacing: 7) {
                GeneratedMediaAssetView(asset: item.asset, compact: true)
                    .frame(width: 132, height: 92)
                Text(storyImageKind(item.kind))
                    .font(.system(size: 9.5, weight: .semibold))
                    .foregroundStyle(storyImageKindColor(item.kind))
                    .padding(.horizontal, 7)
                    .padding(.vertical, 3)
                    .background(storyImageKindColor(item.kind).opacity(0.1), in: Capsule())
                Text(item.title)
                    .font(.system(size: 11.5, weight: .medium))
                    .lineLimit(1)
                    .frame(width: 132, alignment: .leading)
            }
        }
        .buttonStyle(.plain)
        .help(appModel.localized("放大查看图片", english: "Enlarge Image"))
    }

    func storyVideoCard(_ item: StoryStudioViewModel.CreationHistoryVideo) -> some View {
        HStack(spacing: 12) {
            Button { videoPreview = .init(item) } label: {
                ZStack {
                    LocalVideoPlayer(url: item.fileURL)
                    Color.black.opacity(0.08)
                    Image(systemName: "play.circle.fill")
                        .font(.system(size: 25))
                        .foregroundStyle(.white)
                        .shadow(radius: 3)
                }
                .frame(width: 126, height: 76)
                .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
            }
            .buttonStyle(.plain)
            VStack(alignment: .leading, spacing: 5) {
                Text(appModel.localized("第 \(item.segmentNumber) 段", english: "Segment \(item.segmentNumber)"))
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.indigo)
                Text(item.isCurrentVersion
                     ? appModel.localized("当前成片", english: "Current Cut")
                     : appModel.localized("历史版本", english: "Previous Version"))
                    .font(.system(size: 9.5, weight: .semibold))
                    .foregroundStyle(item.isCurrentVersion ? Color.green : .secondary)
                Text(item.title)
                    .font(.system(size: 12.5, weight: .semibold))
                    .lineLimit(1)
                Text("\(item.seconds)s · \(item.modelName)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 4)
            Button {
                NSWorkspace.shared.activateFileViewerSelecting([item.fileURL])
            } label: {
                Image(systemName: "folder")
            }
            .buttonStyle(.borderless)
            .help(appModel.localized("在访达中显示", english: "Show in Finder"))
        }
        .padding(10)
        .background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 12))
        .overlay { RoundedRectangle(cornerRadius: 12).stroke(Color.primary.opacity(0.055)) }
    }

    func storyImageKind(_ kind: StoryStudioViewModel.CreationHistoryImage.Kind) -> String {
        switch kind {
        case .character: appModel.localized("人物", english: "Character")
        case .scene: appModel.localized("场景", english: "Scene")
        case .prop: appModel.localized("道具", english: "Prop")
        case .firstFrame: appModel.localized("首帧", english: "First Frame")
        case .lastFrame: appModel.localized("尾帧", english: "Last Frame")
        case .videoLastFrame: appModel.localized("成片末帧", english: "Video Final Frame")
        }
    }

    func storyImageKindColor(_ kind: StoryStudioViewModel.CreationHistoryImage.Kind) -> Color {
        switch kind {
        case .character: .purple
        case .scene: .blue
        case .prop: .orange
        case .firstFrame: .teal
        case .lastFrame: .pink
        case .videoLastFrame: .green
        }
    }

    @ViewBuilder
    var videoHistorySection: some View {
        if !viewModel.videoHistory.isEmpty {
            Text(appModel.localized("单独生成的视频", english: "Standalone Videos"))
                .font(.system(size: 15, weight: .semibold))
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 420), spacing: 14)],
                spacing: 14
            ) {
                ForEach(viewModel.videoHistory) { item in
                    HStack(spacing: 14) {
                        ZStack(alignment: .topTrailing) {
                            LocalVideoPlayer(url: item.fileURL)
                                .frame(width: 144, height: 88)
                                .clipShape(RoundedRectangle(cornerRadius: 9))
                            Button { videoPreview = .init(item) } label: {
                                Image(systemName: "arrow.up.left.and.arrow.down.right")
                                    .font(.system(size: 11, weight: .semibold))
                                    .padding(7)
                                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 7))
                                    .padding(7)
                            }
                            .buttonStyle(.plain)
                            .help(appModel.localized("放大播放视频", english: "Enlarge Video"))
                            .accessibilityLabel(appModel.localized("放大播放视频", english: "Enlarge Video"))
                        }
                        historyDescription(
                            prompt: item.prompt,
                            modelName: item.modelName,
                            createdAt: item.createdAt
                        )
                        Spacer()
                        Button {
                            NSWorkspace.shared.activateFileViewerSelecting([item.fileURL])
                        } label: {
                            Image(systemName: "folder")
                        }
                        .buttonStyle(.borderless)
                        .help(appModel.localized("在访达中显示", english: "Show in Finder"))
                    }
                    .padding(12)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
                    .overlay { RoundedRectangle(cornerRadius: 14).stroke(Color.primary.opacity(0.07)) }
                }
            }
        }
    }

    @ViewBuilder
    var imageHistorySection: some View {
        if !viewModel.history.isEmpty {
            Text(appModel.localized("单独生成的图片", english: "Standalone Images"))
                .font(.system(size: 15, weight: .semibold))
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 360), spacing: 14)],
                spacing: 14
            ) {
                ForEach(viewModel.history) { item in
                    HStack(spacing: 14) {
                        if let asset = item.images.first {
                            previewThumbnail(asset, images: item.images, compact: true)
                                .frame(width: 108, height: 82)
                        }
                        historyDescription(
                            prompt: item.prompt,
                            modelName: item.modelName,
                            createdAt: item.createdAt
                        )
                        Spacer()
                        Text("\(item.images.count)")
                            .font(.system(size: 11, weight: .semibold, design: .rounded))
                            .padding(7)
                            .background(.quaternary, in: Capsule())
                        if let url = item.images.first?.url, url.isFileURL {
                            Button {
                                NSWorkspace.shared.activateFileViewerSelecting(item.images.compactMap(\.url))
                            } label: {
                                Image(systemName: "folder")
                            }
                            .buttonStyle(.borderless)
                            .help(appModel.localized("在访达中显示", english: "Show in Finder"))
                        }
                    }
                    .padding(12)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
                    .overlay { RoundedRectangle(cornerRadius: 14).stroke(Color.primary.opacity(0.07)) }
                }
            }
        }
    }

    func historyDescription(prompt: String, modelName: String, createdAt: Date) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(prompt)
                .font(.system(size: 12.5, weight: .medium))
                .lineLimit(2)
            Text("\(modelName) · \(createdAt.formatted(date: .abbreviated, time: .shortened))")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    func previewThumbnail(_ asset: GeneratedMediaAsset, images: [GeneratedMediaAsset], compact: Bool = false) -> some View {
        Button {
            imagePreview = .init(images: images, selectedIndex: images.firstIndex(of: asset) ?? 0)
        } label: {
            GeneratedMediaAssetView(asset: asset, compact: compact)
                .overlay(alignment: .topTrailing) {
                    Image(systemName: "arrow.up.left.and.arrow.down.right")
                        .font(.system(size: 11, weight: .semibold))
                        .padding(7)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 7))
                        .padding(8)
                }
        }
        .buttonStyle(.plain)
        .help(appModel.localized("放大查看图片", english: "Enlarge Image"))
        .accessibilityLabel(appModel.localized("放大查看图片", english: "Enlarge Image"))
    }

    func fieldLabel(_ value: String) -> some View {
        Text(value)
            .font(.system(size: 11.5, weight: .semibold))
            .foregroundStyle(.secondary)
    }
}
