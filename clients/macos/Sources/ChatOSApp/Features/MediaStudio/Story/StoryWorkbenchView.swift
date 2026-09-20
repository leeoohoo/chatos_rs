import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct StoryWorkbenchView: View {
    enum Section: String, CaseIterable {
        case source, style, portraits, segments, graph
    }
    @EnvironmentObject var appModel: AppModel
    @ObservedObject var viewModel: StoryStudioViewModel
    @ObservedObject var mediaStudio: MediaStudioViewModel
    let project: StoryProject
    let showSettings: () -> Void
    @State var source = ""
    @State var style = ""
    @State var ratio = "16:9"
    @State var editSegment: StorySegment?
    @State var imageTarget: StoryImageTarget?
    @State var preview: MediaStudioImagePreviewRequest?
    @State var segmentToDelete: String?
    @State var segmentToRetry: String?
    @State var agentRunToAbandon: UUID?
    @State var planningConfirmation: StoryPlanningConfirmation?
    @State var showsMediaBatch = false
    @State var mediaBatchKind: StoryMediaBatch.Kind = .pipeline
    @State var videoToRegenerate: StorySegment?
    @State var playlistPreview: StoryStudioViewModel.CreationHistoryGroup?
    @State var section: Section = .source

    var selected: StorySegment? { project.segments.first { $0.id == viewModel.selectedSegmentID } }
    var selectedReady: [StorySegment] { project.segments.filter { viewModel.selectedSegments.contains($0.id) && $0.isReady } }
    var selectedReadySeconds: Int { selectedReady.reduce(0) { $0 + $1.seconds } }
    var isAgentDraftVisible: Bool { viewModel.isPresentingAgentDraft(projectID: project.id) }
    var recoverableAgentRun: StoryAgentRun? { viewModel.recoverableAgentRun(projectID: project.id) }
    var editingLocked: Bool {
        viewModel.isBusy || isAgentDraftVisible || viewModel.hasActiveFrameGenerations(projectID: project.id)
    }
    var playlistGroup: StoryStudioViewModel.CreationHistoryGroup? {
        viewModel.creationHistoryGroups.first { $0.projectID == project.id && !$0.currentVideos.isEmpty }
    }

    var body: some View {
        ZStack {
            LinearGradient(
                colors: [Color(nsColor: .windowBackgroundColor), Color.indigo.opacity(0.035)],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            ).ignoresSafeArea()
            VStack(spacing: 0) {
                header
                sectionBar
                if isAgentDraftVisible || recoverableAgentRun != nil { agentDraftBanner }
                workspaceContent
                    .frame(maxWidth: 1_520, maxHeight: .infinity, alignment: .top)
                    .padding(.horizontal, 24)
                    .padding(.top, 22)
                    .padding(.bottom, section == .segments && !project.segments.isEmpty ? 14 : 24)
                if section == .segments && !project.segments.isEmpty {
                    batchBar
                }
            }
        }
        .onAppear {
            let draft = viewModel.sourceDraft(for: project)
            source = draft.source; style = draft.style; ratio = draft.ratio
        }
        .onChange(of: source) { _, _ in rememberDraft() }
        .onChange(of: style) { _, _ in rememberDraft() }
        .onChange(of: ratio) { _, _ in rememberDraft() }
        .sheet(item: $editSegment) { segment in
            StorySegmentEditor(project: project, segment: segment) { edited, relations in
                viewModel.saveSegment(edited, relations: relations)
            }
                .environmentObject(appModel)
        }
        .sheet(item: $planningConfirmation) { confirmation in
            StoryAgentStartView(viewModel: viewModel, confirmation: confirmation).environmentObject(appModel)
        }
        .sheet(isPresented: $showsMediaBatch) {
            StoryMediaBatchStartView(viewModel: viewModel, project: project, models: mediaStudio.models,
                                     initialKind: mediaBatchKind).environmentObject(appModel)
        }
        .sheet(item: $videoToRegenerate) { segment in
            StoryVideoRegenerationStartView(
                viewModel: viewModel, project: project, segmentID: segment.id,
                models: mediaStudio.models
            )
            .environmentObject(appModel)
        }
        .sheet(item: $playlistPreview) { group in
            StoryVideoPlaylistPlayer(group: group).environmentObject(appModel)
        }
        .sheet(item: $imageTarget) { target in
            StoryImagePicker(viewModel: viewModel, mediaStudio: mediaStudio, project: project, target: target)
                .environmentObject(appModel)
        }
        .sheet(item: $preview) { request in
            MediaStudioImagePreview(request: request) { mediaStudio.useGeneratedImageForVideo($0) }
                .environmentObject(appModel)
        }
        .confirmationDialog(appModel.localized("删除该分段并重新检查全剧衔接？", english: "Remove this segment and re-plan continuity?"), isPresented: Binding(get: { segmentToDelete != nil }, set: { if !$0 { segmentToDelete = nil } }), titleVisibility: .visible) {
            Button(appModel.localized("删除分段", english: "Remove Segment"), role: .destructive) {
                if let id = segmentToDelete { viewModel.removeSegment(id) }; segmentToDelete = nil
            }
        } message: {
            Text(appModel.localized("其它段的详细镜头计划将需要重新细化，素材文件仍保留。", english: "Other segments will need fresh shot plans. Existing asset files are retained."))
        }
        .confirmationDialog(appModel.localized("确认已核对原任务？", english: "Have you verified the original task?"), isPresented: Binding(get: { segmentToRetry != nil }, set: { if !$0 { segmentToRetry = nil } }), titleVisibility: .visible) {
            Button(appModel.localized("已核对，允许重新生成", english: "Verified — Allow a New Generation")) {
                if let id = segmentToRetry { viewModel.allowRetryAfterVerification(id) }; segmentToRetry = nil
            }
        } message: {
            Text(appModel.localized("只有确认原任务失败或未创建时才应重试。后续再次生成可能扣费，旧任务记录会保留。本操作不会立即提交。", english: "Only retry if the original task failed or was not created. A new generation may incur charges. Old task records are retained; this action does not submit immediately."))
        }
        .confirmationDialog(appModel.localized("放弃这份中断草稿？", english: "Discard This Interrupted Draft?"),
                            isPresented: Binding(get: { agentRunToAbandon != nil }, set: { if !$0 { agentRunToAbandon = nil } }),
                            titleVisibility: .visible) {
            Button(appModel.localized("放弃草稿", english: "Discard Draft"), role: .destructive) {
                if let id = agentRunToAbandon { viewModel.abandonAgent(id) }
                agentRunToAbandon = nil
            }
        } message: {
            Text(appModel.localized("只会放弃未提交的 AI 规划草稿；正式项目、已有素材、图片和视频都不会被删除。",
                                    english: "Only the unapplied AI planning draft is discarded. The project, assets, images and videos are kept."))
        }
    }

}
