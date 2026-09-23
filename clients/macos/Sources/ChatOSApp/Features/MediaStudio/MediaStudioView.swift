import AppKit
import AVKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct MediaStudioVideoPreviewRequest: Identifiable {
    var id: String
    var title: String?
    var prompt: String
    var modelName: String
    var createdAt: Date
    var fileURL: URL

    init(_ item: MediaStudioViewModel.VideoHistoryItem) {
        id = item.id; title = nil; prompt = item.prompt; modelName = item.modelName
        createdAt = item.createdAt; fileURL = item.fileURL
    }

    init(_ item: StoryStudioViewModel.CreationHistoryVideo) {
        id = item.id; title = item.title; prompt = item.prompt; modelName = item.modelName
        createdAt = item.createdAt; fileURL = item.fileURL
    }
}

struct MediaStudioView: View {
    @EnvironmentObject var appModel: AppModel
    @ObservedObject var viewModel: MediaStudioViewModel
    @ObservedObject var stories: StoryStudioViewModel
    @State var showsGeneratedImagePicker = false
    @State var showsGeneratedReferencePicker = false
    @State var imagePreview: MediaStudioImagePreviewRequest?
    @State var videoPreview: MediaStudioVideoPreviewRequest?
    @State var playlistPreview: StoryStudioViewModel.CreationHistoryGroup?

    init(viewModel: MediaStudioViewModel) {
        _viewModel = ObservedObject(wrappedValue: viewModel)
        _stories = ObservedObject(wrappedValue: viewModel.stories)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            content
        }
        .background(studioBackground)
        .task { viewModel.loadIfNeeded() }
        .onChange(of: viewModel.section) { _, section in
            if section == .story { viewModel.stories.backToList() }
        }
        .sheet(item: $imagePreview) { request in
            MediaStudioImagePreview(request: request) { asset in
                viewModel.useGeneratedImageForVideo(asset)
            }
            .environmentObject(appModel)
        }
        .sheet(item: $videoPreview) { item in
            MediaStudioVideoPreview(item: item)
                .environmentObject(appModel)
        }
        .sheet(item: $playlistPreview) { group in
            StoryVideoPlaylistPlayer(group: group)
                .environmentObject(appModel)
        }
        .sheet(isPresented: $showsGeneratedImagePicker) {
            MediaStudioGeneratedImagePicker(viewModel: viewModel)
                .environmentObject(appModel)
        }
        .sheet(isPresented: $showsGeneratedReferencePicker) {
            MediaStudioGeneratedReferencePicker(viewModel: viewModel)
                .environmentObject(appModel)
        }
        .onChange(of: appModel.authentication.phase) { _, _ in
            imagePreview = nil
            videoPreview = nil
            playlistPreview = nil
            showsGeneratedImagePicker = false
            showsGeneratedReferencePicker = false
        }
    }

    private var studioBackground: some View {
        ZStack {
            Color(nsColor: .windowBackgroundColor)
            LinearGradient(
                colors: [
                    Color.purple.opacity(0.055),
                    Color.clear,
                    Color.blue.opacity(0.025),
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        }
    }

    private var header: some View {
        HStack(spacing: 16) {
            ZStack {
                RoundedRectangle(cornerRadius: 11)
                    .fill(
                        LinearGradient(
                            colors: [.purple, .pink.opacity(0.82)],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        )
                    )
                Image(systemName: "wand.and.stars")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(.white)
            }
            .frame(width: 38, height: 38)
            .shadow(color: .purple.opacity(0.22), radius: 8, y: 3)

            VStack(alignment: .leading, spacing: 2) {
                Text(appModel.localized("AI 创作", english: "AI Creation"))
                    .font(.system(size: 16, weight: .semibold))
                Text(appModel.localized(
                    "独立于聊天的图片与视频工作台",
                    english: "A dedicated image and video workspace"
                ))
                    .font(.system(size: 11.5))
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 24)

            Picker("", selection: $viewModel.section) {
                Label(appModel.localized("图片", english: "Image"), systemImage: "photo")
                    .tag(MediaStudioViewModel.Section.image)
                Label(appModel.localized("视频", english: "Video"), systemImage: "video")
                    .tag(MediaStudioViewModel.Section.video)
                Label(appModel.localized("剧情模式", english: "Story"), systemImage: "film.stack")
                    .tag(MediaStudioViewModel.Section.story)
                Label(appModel.localized("记录", english: "History"), systemImage: "clock")
                    .tag(MediaStudioViewModel.Section.history)
            }
            .labelsHidden()
            .pickerStyle(.segmented)
            .frame(width: 400)
        }
        .padding(.horizontal, 20)
        .frame(height: 68)
        .background(.bar)
    }

    @ViewBuilder
    private var content: some View {
        switch viewModel.section {
        case .image:
            imageWorkspace
        case .video:
            videoWorkspace
        case .story:
            StoryStudioView(viewModel: stories, mediaStudio: viewModel)
        case .history:
            historyWorkspace
        }
    }

}
