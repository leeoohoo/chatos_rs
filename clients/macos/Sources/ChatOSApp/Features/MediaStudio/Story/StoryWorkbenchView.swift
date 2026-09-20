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

    var segmentsPanel: some View {
        VStack(alignment: .leading, spacing: 16) {
            if !viewModel.projectMediaBatches.isEmpty {
                StoryMediaBatchPanel(viewModel: viewModel)
            }
            HStack(alignment: .top, spacing: 16) {
                overview.frame(width: 410)
                detailPanel
                    .frame(minWidth: 560, maxWidth: .infinity, maxHeight: .infinity,
                           alignment: .topLeading)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .onAppear { selectFirstSegmentIfNeeded(project.segments.map(\.id)) }
        .onChange(of: project.segments.map(\.id)) { _, ids in selectFirstSegmentIfNeeded(ids) }
    }

    var overview: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                VStack(alignment: .leading, spacing: 6) {
                    Text(appModel.localized("全剧时间线", english: "Story Timeline")).font(.title3.bold())
                    Text("\(project.segments.count) " + appModel.localized("个分段", english: "segments") + " · \(project.totalSeconds)s")
                        .font(.callout.monospacedDigit()).foregroundStyle(.blue)
                }
                Spacer()
                Menu(appModel.localized("计划操作", english: "Plan Actions")) {
                    Button(appModel.localized("批量制作 / 一键执行", english: "Batch Production / Full Pipeline")) {
                        mediaBatchKind = .pipeline; showsMediaBatch = true
                    }
                    Button(appModel.localized("逐段细化未完成计划", english: "Refine Unfinished Plans")) {
                        confirmPlanning(.refine, targets: project.segments.filter { $0.detail == nil && $0.attempt == nil && $0.video == nil }.map(\.id))
                    }.disabled(project.segments.isEmpty)
                    Button(appModel.localized("添加剧情分段", english: "Add Story Segment")) { viewModel.addSegment() }
                        .disabled(project.source.isEmpty || project.hasUnresolvedJobs || project.completedCount > 0)
                }.disabled(editingLocked)
            }
            if project.segments.isEmpty {
                ContentUnavailableView {
                    Label(appModel.localized("先规划整个故事", english: "Plan the Entire Story"), systemImage: "list.bullet.rectangle")
                } description: {
                    Text(appModel.localized("请先到“剧情原文”保存完整故事并规划，之后这里会展示剧情段与独立转场段。", english: "Save and plan the complete story first; story segments and independent transitions will appear here."))
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: 9) {
                        ForEach(Array(project.segments.enumerated()), id: \.element.id) { index, segment in
                            segmentRow(segment, index: index)
                        }
                    }
                }
                Text(appModel.localized("每段单独生成，完成后可按剧情顺序连续播放。", english: "Segments generate separately, then play continuously in story order."))
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(18)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .storySurface(tint: .blue)
    }

    func segmentRow(_ segment: StorySegment, index: Int) -> some View {
        let start = project.segments.prefix(index).reduce(0) { $0 + $1.seconds }
        let tint: Color = segment.kind == .transition ? .purple : .blue
        return HStack(spacing: 10) {
            Toggle("", isOn: Binding(get: { viewModel.selectedSegments.contains(segment.id) }, set: { on in
                if on { viewModel.selectedSegments.insert(segment.id) } else { viewModel.selectedSegments.remove(segment.id) }
            })).labelsHidden().toggleStyle(.checkbox)
                .disabled(!segment.isReady || viewModel.isBusy || isAgentDraftVisible)
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text(String(format: "%02d", index + 1))
                        .font(.caption.bold().monospacedDigit()).foregroundStyle(.white)
                        .frame(width: 27, height: 27).background(tint, in: Circle())
                    Text(segment.title).fontWeight(.semibold).lineLimit(1)
                    if segment.kind == .transition {
                        Text(appModel.localized("转场", english: "Transition"))
                            .font(.caption2.bold()).foregroundStyle(.purple)
                            .padding(.horizontal, 7).padding(.vertical, 3)
                            .background(Color.purple.opacity(0.1), in: Capsule())
                    }
                    Spacer(minLength: 4)
                    Text("\(time(start))–\(time(start + segment.seconds))").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                }
                Text(segment.synopsis).font(.caption).lineLimit(2).foregroundStyle(.secondary)
                HStack {
                    Text(status(segment)).font(.caption2).foregroundStyle(segment.video == nil ? Color.blue : .green)
                    Spacer()
                    Text("\(segment.seconds)s").font(.caption2).foregroundStyle(.secondary)
                }
            }.contentShape(Rectangle()).onTapGesture { viewModel.selectedSegmentID = segment.id }
        }.padding(12)
        .background(viewModel.selectedSegmentID == segment.id ? Color.blue.opacity(0.09) : Color.primary.opacity(0.025),
                    in: RoundedRectangle(cornerRadius: 11, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 11, style: .continuous)
            .stroke(viewModel.selectedSegmentID == segment.id ? Color.blue.opacity(0.55) : Color.primary.opacity(0.045)))
        .contextMenu {
            Button(appModel.localized("编辑分段", english: "Edit Segment")) { editSegment = segment }.disabled(editingLocked || segment.attempt != nil)
            Button(appModel.localized("上移（重新细化衔接）", english: "Move Up and Re-plan")) { viewModel.moveSegment(segment.id, offset: -1) }
                .disabled(editingLocked || index == 0 || project.hasUnresolvedJobs || project.completedCount > 0)
            Button(appModel.localized("下移（重新细化衔接）", english: "Move Down and Re-plan")) { viewModel.moveSegment(segment.id, offset: 1) }
                .disabled(editingLocked || index + 1 == project.segments.count || project.hasUnresolvedJobs || project.completedCount > 0)
            Button(appModel.localized("删除分段", english: "Remove Segment"), role: .destructive) { segmentToDelete = segment.id }
                .disabled(editingLocked || project.hasUnresolvedJobs || project.completedCount > 0)
        }
    }

    @ViewBuilder var detailPanel: some View {
        if let segment = selected {
            ScrollView {
                VStack(alignment: .leading, spacing: 15) {
                    HStack {
                        Text(segment.title).font(.headline)
                        Spacer()
                        Button(appModel.localized("编辑", english: "Edit")) { editSegment = segment }
                            .disabled(editingLocked || segment.attempt != nil)
                    }
                    Text(status(segment)).font(.caption).foregroundStyle(.secondary)
                    if let video = segment.video, let url = viewModel.videoURL(video, projectID: project.id) {
                        VStack(alignment: .leading, spacing: 9) {
                            HStack(spacing: 7) {
                                Image(systemName: "play.rectangle.fill")
                                    .foregroundStyle(.purple)
                                Text(appModel.localized("生成视频", english: "Generated Video"))
                                    .font(.callout.weight(.semibold))
                                Spacer()
                                Text(video.modelName)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            LocalVideoPlayer(url: url)
                                .aspectRatio(16.0 / 9.0, contentMode: .fit)
                                .frame(maxWidth: .infinity, minHeight: 260, maxHeight: 520)
                                .background(Color.black.opacity(0.92))
                                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                                .overlay {
                                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                                        .stroke(Color.primary.opacity(0.1))
                                }
                            HStack(spacing: 10) {
                                Button {
                                    videoToRegenerate = segment
                                } label: {
                                    Label(appModel.localized("重新生成本段视频…", english: "Regenerate This Video…"),
                                          systemImage: "arrow.triangle.2.circlepath")
                                        .font(.callout.weight(.semibold))
                                }
                                .buttonStyle(.borderedProminent)
                                .tint(.indigo)
                                .disabled(editingLocked)
                                Text(appModel.localized("旧视频会保留在创作记录中", english: "The previous video stays in creation history"))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            HStack(spacing: 10) {
                                if let actual = segment.actualVideoLastFrame,
                                   let asset = viewModel.mediaAsset(actual, projectID: project.id) {
                                    StoryThumbnail(asset: asset)
                                        .frame(width: 76, height: 48)
                                        .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                                    VStack(alignment: .leading, spacing: 2) {
                                        HStack(spacing: 6) {
                                            Text(appModel.localized("成片末帧", english: "Video Final Frame"))
                                                .font(.caption.weight(.semibold))
                                            Label(appModel.localized("已确认用于衔接", english: "Confirmed for Continuity"),
                                                  systemImage: "checkmark.circle.fill")
                                                .font(.caption2.weight(.semibold)).foregroundStyle(.green)
                                        }
                                        Text(appModel.localized("默认衔接到下一段，也可手动改选下一段首帧",
                                                                english: "Used for the next segment by default; you can still choose another first frame"))
                                            .font(.caption2).foregroundStyle(.secondary).lineLimit(2)
                                    }
                                } else {
                                    Label(appModel.localized("尚未提取成片末帧", english: "Video final frame not extracted yet"),
                                          systemImage: "photo.badge.arrow.down")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer(minLength: 4)
                                let extracting = viewModel.isExtractingVideoLastFrame(segment.id, projectID: project.id)
                                Button {
                                    viewModel.reextractVideoLastFrame(segment.id)
                                } label: {
                                    if extracting {
                                        HStack(spacing: 6) {
                                            ProgressView().controlSize(.mini)
                                            Text(appModel.localized("提取中", english: "Extracting"))
                                        }
                                    } else {
                                        Label(segment.actualVideoLastFrame == nil
                                              ? appModel.localized("提取末帧", english: "Extract Final Frame")
                                              : appModel.localized("重新抽取", english: "Extract Again"),
                                              systemImage: "arrow.clockwise")
                                    }
                                }
                                .buttonStyle(.bordered)
                                .disabled(extracting)
                            }
                            if let index = project.segments.firstIndex(where: { $0.id == segment.id }),
                               project.segments.indices.contains(index + 1) {
                                let next = project.segments[index + 1]
                                Button {
                                    imageTarget = .init(assetID: nil, segmentID: next.id, frameRole: .first)
                                } label: {
                                    Label(appModel.localized("手动生成或选择下一段首帧",
                                                             english: "Generate or Choose Next First Frame"),
                                          systemImage: "sparkles.rectangle.stack")
                                }
                                .buttonStyle(.bordered)
                                .disabled(next.attempt != nil)
                            }
                        }
                        .padding(12)
                        .background(Color.purple.opacity(0.045), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    } else if let attempt = segment.attempt {
                        let isGeneratingVideo = viewModel.isGeneratingVideo(segment.id, projectID: project.id)
                        if isGeneratingVideo {
                            VStack(alignment: .leading, spacing: 9) {
                                HStack(spacing: 8) {
                                    ProgressView().controlSize(.small)
                                    Text(appModel.localized("视频任务正在提交和生成", english: "Submitting and generating video"))
                                        .font(.callout.weight(.medium))
                                    Spacer()
                                    if let percent = viewModel.videoProgress(segment.id, projectID: project.id)?.percent {
                                        Text("\(Int(min(100, max(0, percent))))%")
                                            .font(.caption.monospacedDigit())
                                    }
                                }
                                if let percent = viewModel.videoProgress(segment.id, projectID: project.id)?.percent {
                                    ProgressView(value: min(100, max(0, percent)), total: 100)
                                }
                                if attempt.jobID == nil {
                                    Text(appModel.localized(
                                        "正在等待生成服务确认，请稍候。",
                                        english: "Waiting for the generation service to confirm."
                                    ))
                                    .font(.caption).foregroundStyle(.secondary)
                                }
                            }
                            .padding(12)
                            .background(Color.blue.opacity(0.055), in: RoundedRectangle(cornerRadius: 11, style: .continuous))
                        } else {
                            if attempt.jobID != nil {
                                Button(appModel.localized("查询原任务 / 恢复下载", english: "Check Task / Resume Download")) {
                                    viewModel.resumeVideo(segment.id)
                                }
                                .disabled(editingLocked)
                            } else {
                                Text(appModel.localized(
                                    "上次提交状态尚未确认。请先核对生成记录，再决定是否重试。",
                                    english: "The previous submission is not yet confirmed. Check your generation history before retrying."
                                ))
                                .font(.caption).foregroundStyle(.orange)
                            }
                            Button(appModel.localized("核对后允许重试…", english: "Allow Retry after Verification…")) {
                                segmentToRetry = segment.id
                            }
                            .disabled(editingLocked)
                        }
                    }
                    if let error = segment.error { Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
                    HStack(alignment: .top, spacing: 12) {
                        frameCard(segment, role: .first)
                        frameCard(segment, role: .last)
                    }
                    Label(appModel.localized(
                        "这里只生成文字计划。手动生成首帧或尾帧时，会同时使用已选参考图、本段全部关联场景/人物/道具及关系，以及完整 \(segment.seconds) 秒镜头语言。",
                        english: "Planning creates text only. Manual frame generation combines selected references, every linked scene/character/prop and relation, and the complete \(segment.seconds)-second shot plan."
                    ), systemImage: "info.circle")
                    .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    videoActionPanel(segment)
                    if let videoModel = mediaStudio.models.first(where: { $0.id == project.models.videoModelID }) {
                        videoGuidancePicker(segment, model: videoModel)
                    }
                    if !hasConfirmedAssets(segment) {
                        Text(appModel.localized("可以打开首帧或尾帧选择器，从本段已确认的素材中任选一个或多个进行生成。",
                                                english: "Open a frame picker and choose any one or more confirmed assets from this segment."))
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    Divider()
                    if let detail = segment.detail {
                        Text(appModel.localized("镜头语言与提示词", english: "Shot Plan and Prompts")).font(.headline)
                        ForEach(Array(detail.shots.enumerated()), id: \.offset) { _, shot in
                            VStack(alignment: .leading, spacing: 6) {
                                Text("\(shot.start)–\(shot.end)s").font(.caption.bold()).foregroundStyle(.purple)
                                Text(shot.prompt).font(.callout).textSelection(.enabled)
                            }
                        }
                        Divider()
                        Text(appModel.localized("前后衔接", english: "Continuity")).font(.headline)
                        Text(detail.continuityIn + "\n\n" + detail.continuityOut).font(.caption).foregroundStyle(.secondary)
                        Text(detail.audio + "\n" + detail.constraints).font(.caption).foregroundStyle(.secondary)
                        Button {
                            confirmPlanning(.refine, targets: [segment.id])
                        } label: {
                            Label(appModel.localized("重新生成本段镜头计划", english: "Regenerate This Segment Plan"),
                                  systemImage: "arrow.triangle.2.circlepath")
                        }
                        .buttonStyle(.bordered)
                        .disabled(editingLocked
                                  || viewModel.activeVideoGenerationCount(projectID: project.id) > 0
                                  || (segment.attempt != nil && segment.video == nil)
                                  || segment.imageGenerationAttemptID != nil
                                  || segment.lastFrameGenerationAttemptID != nil)
                        .help(appModel.localized("旧图片版本会保留；新计划应用后需要重新确认首帧与尾帧。",
                                                 english: "Existing image versions are kept; first and last frames must be confirmed again after applying the new plan."))
                    } else {
                        Text(appModel.localized("此段只有剧情概要，尚未展开镜头计划。", english: "This segment has an outline but no detailed shot plan yet."))
                            .font(.callout).foregroundStyle(.secondary)
                        Button(appModel.localized("细化这个 \(segment.seconds) 秒分段", english: "Refine This \(segment.seconds)-second Segment")) { confirmPlanning(.refine, targets: [segment.id]) }
                            .disabled(editingLocked)
                    }
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .topLeading)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .storySurface(tint: .blue)
        } else {
            VStack(spacing: 14) {
                Image(systemName: "rectangle.stack.badge.plus")
                    .font(.system(size: 34, weight: .medium))
                    .foregroundStyle(.blue)
                    .frame(width: 68, height: 68)
                    .background(Color.blue.opacity(0.1), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                Text(appModel.localized("选择一个分段", english: "Select a Segment"))
                    .font(.title3.weight(.semibold))
                Text(appModel.localized("在左侧时间线选择分段后，可以查看镜头计划、首尾帧和视频。",
                                        english: "Choose a segment from the timeline to review its shot plan, frames, and video."))
                    .font(.callout).foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 420)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
            .storySurface(tint: .blue)
        }
    }

    @ViewBuilder func videoGuidancePicker(_ segment: StorySegment,
                                                  model: MediaGenerationModel) -> some View {
        let index = project.segments.firstIndex(where: { $0.id == segment.id })
        let hasPreviousVideo = index.map { $0 > 0 && project.segments[$0 - 1].video != nil } == true
        VStack(alignment: .leading, spacing: 9) {
            Text(appModel.localized("视频衔接方式", english: "Video Continuity Input"))
                .font(.callout.weight(.semibold))
            Picker("", selection: Binding(get: { segment.videoGuidanceMode }, set: {
                viewModel.setVideoGuidanceMode($0, segmentID: segment.id)
            })) {
                Text(appModel.localized("仅首帧", english: "First Frame"))
                    .tag(StoryVideoGuidanceMode.firstFrame)
                if model.supportsVideoLastFrame
                    && (segment.lastFrame != nil || segment.videoGuidanceMode == .firstAndLastFrames) {
                    Text(appModel.localized("首帧 + 尾帧", english: "First + Last Frames"))
                        .tag(StoryVideoGuidanceMode.firstAndLastFrames)
                }
                if model.supportsVideoReference
                    && (hasPreviousVideo || segment.videoGuidanceMode == .previousVideo) {
                    Text(appModel.localized("上一段视频", english: "Previous Video"))
                        .tag(StoryVideoGuidanceMode.previousVideo)
                }
                if segment.videoGuidanceMode == .sourceVideo {
                    Text(appModel.localized("原视频重做", english: "Original Video"))
                        .tag(StoryVideoGuidanceMode.sourceVideo)
                }
            }
            .labelsHidden()
            .pickerStyle(.segmented)
            .disabled(editingLocked || segment.attempt != nil || segment.video != nil)

            switch segment.videoGuidanceMode {
            case .firstFrame:
                Label(appModel.localized("只把当前确认首帧交给模型；已保存尾帧不会参与本次视频生成。",
                                         english: "Only the confirmed first frame is sent; saved last frames are not used for this generation."),
                      systemImage: "1.circle.fill")
            case .firstAndLastFrames:
                Label(appModel.localized("同时发送当前确认的首帧和尾帧，约束视频的开始与结束。",
                                         english: "Sends both confirmed frames to constrain the beginning and end."),
                      systemImage: "rectangle.leadinghalf.inset.filled.arrow.leading")
            case .previousVideo:
                Label(model.supportsVideoExtension
                      ? appModel.localized("从紧邻上一段成片的结尾继续生成，延续人物、动作、光线与运镜；不会同时发送首帧或尾帧。",
                                           english: "Continues from the end of the immediately previous cut, preserving character, action, lighting, and camera continuity; frame inputs are not sent at the same time.")
                      : appModel.localized("把紧邻上一段的完整成片作为参考，帮助延续人物、动作和运镜；当前模型属于参考重生成，并非从结尾精确续写。",
                                           english: "Uses the immediately previous cut as a reference for character, action, and camera continuity. This model regenerates from reference rather than precisely extending the ending."),
                      systemImage: "film.stack.fill")
            case .sourceVideo:
                Label(model.supportsVideoEditing
                      ? appModel.localized("编辑创作记录中最近一版原视频，按你填写的问题修改，并尽量保留未提及的画面。",
                                           english: "Edits the latest archived original using your requested changes while preserving unaffected content where possible.")
                      : appModel.localized("把最近一版原视频作为参考重新生成；当前模型不能精确编辑原片，画面可能整体变化。",
                                           english: "Regenerates using the latest original as reference. This model cannot precisely edit the source, so the overall result may change."),
                      systemImage: "arrow.triangle.2.circlepath.camera.fill")
            }
            if model.supportsVideoReference, index != 0, !hasPreviousVideo,
               segment.videoGuidanceMode != .previousVideo {
                Text(appModel.localized("上一段成片完成后，这里会出现“上一段视频”选项。",
                                        english: "The Previous Video option appears after the preceding cut is complete."))
                    .font(.caption2).foregroundStyle(.secondary)
            }
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .padding(11)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.blue.opacity(0.055), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Color.blue.opacity(0.14)))
    }

    func selectFirstSegmentIfNeeded(_ ids: [String]) {
        guard !ids.isEmpty, viewModel.selectedSegmentID.map(ids.contains) != true else { return }
        viewModel.selectedSegmentID = ids.first
    }

    func frameCard(_ segment: StorySegment, role: StoryFrameRole) -> some View {
        let collection = segment.frames(for: role)
        // A cancelled last frame remains available in history, but must not keep
        // occupying the active last-frame slot. First-frame generation still uses
        // the latest image as a preview until the user confirms it.
        let frame = role == .first ? (collection.confirmedImage ?? collection.images.last) : collection.confirmedImage
        let isGenerating = viewModel.isGeneratingFrame(segment.id, role: role, projectID: project.id)
        let isFirst = role == .first
        let previousTail = isFirst ? StoryContinuityContext.previousActualVideoTail(project, segmentID: segment.id) : nil
        let directlyInherited = previousTail?.image.id == collection.confirmedImage?.id
            && segment.inheritedFirstFrameSourceSegmentID == previousTail?.segment.id
        return VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(isFirst ? appModel.localized("首帧", english: "First Frame") : appModel.localized("尾帧", english: "Last Frame"))
                    .font(.caption.bold())
                Spacer()
                if collection.confirmedImage != nil {
                    Label(appModel.localized("已确认", english: "Confirmed"), systemImage: "checkmark.circle.fill")
                        .font(.caption2).foregroundStyle(.green)
                } else if isGenerating {
                    HStack(spacing: 6) {
                        ProgressView().controlSize(.mini)
                        Text(appModel.localized("生成中", english: "Generating"))
                    }
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.indigo)
                }
            }
            if directlyInherited {
                Label(appModel.localized("已承接上一段成片最后一帧 · 未调用图片模型",
                                         english: "Inherited from the previous video's final frame · No image model call"),
                      systemImage: "link.circle.fill")
                    .font(.caption2.weight(.semibold)).foregroundStyle(.green)
            }
            Button {
                if isGenerating || frame == nil {
                    imageTarget = .init(assetID: nil, segmentID: segment.id, frameRole: role)
                } else if let frame, let asset = viewModel.mediaAsset(frame, projectID: project.id) {
                    preview = .init(images: [asset])
                }
            } label: {
                ZStack {
                    StoryThumbnail(asset: frame.flatMap { viewModel.mediaAsset($0, projectID: project.id) })
                        .frame(height: 118).frame(maxWidth: .infinity).clipped()
                    if isGenerating {
                        Color.black.opacity(frame == nil ? 0.025 : 0.3)
                        VStack(spacing: 8) {
                            ProgressView().controlSize(.small)
                            Text(isFirst
                                 ? appModel.localized("正在生成首帧…", english: "Generating First Frame…")
                                 : appModel.localized("正在生成尾帧…", english: "Generating Last Frame…"))
                                .font(.caption.weight(.semibold))
                            Text(appModel.localized("点击查看状态", english: "Click to view status"))
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                        .padding(10)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }
            .buttonStyle(.plain)
            Button(isFirst
                   ? appModel.localized("选择 / 上传 / 确认首帧", english: "Choose / Upload / Confirm First Frame")
                   : appModel.localized("选择 / 上传 / 确认尾帧", english: "Choose / Upload / Confirm Last Frame")) {
                imageTarget = .init(assetID: nil, segmentID: segment.id, frameRole: role)
            }.disabled((editingLocked && !isGenerating) || segment.attempt != nil)
            if isGenerating {
                Button {
                    imageTarget = .init(assetID: nil, segmentID: segment.id, frameRole: role)
                } label: {
                    Label(appModel.localized("查看生成状态", english: "View Generation Status"),
                          systemImage: "arrow.up.right.square")
                }
                .buttonStyle(.borderedProminent)
                .tint(.indigo)
            } else {
                Button {
                    imageTarget = .init(assetID: nil, segmentID: segment.id, frameRole: role)
                } label: {
                    Label(frameGenerationTitle(collection, role: role),
                          systemImage: collection.images.isEmpty ? "sparkles.rectangle.stack" : "arrow.clockwise")
                }
                .buttonStyle(.borderedProminent)
                .tint(.indigo)
                .disabled(editingLocked || segment.detail == nil || segment.attempt != nil
                          || collection.generationAttemptID != nil)
            }
            if !isFirst, collection.confirmedImage != nil,
               segment.attempt == nil, segment.video == nil {
                Button(role: .destructive) {
                    viewModel.clearConfirmedLastFrame(segment.id)
                } label: {
                    Label(appModel.localized("取消使用尾帧", english: "Stop Using Last Frame"),
                          systemImage: "xmark.circle")
                }
                .buttonStyle(.bordered)
                .disabled(editingLocked || isGenerating)
            }
            if let error = viewModel.frameGenerationError(segment.id, role: role, projectID: project.id) {
                Text(error).font(.caption2).foregroundStyle(.red).lineLimit(3)
            }
        }
        .padding(10).frame(maxWidth: .infinity)
        .background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 11))
        .overlay(RoundedRectangle(cornerRadius: 11).stroke(Color.primary.opacity(0.06)))
    }

    @ViewBuilder func videoActionPanel(_ segment: StorySegment) -> some View {
        if segment.video == nil, segment.attempt == nil, segment.detail != nil {
            let hasGeneratedFirst = !segment.firstFrames.images.isEmpty
            let hasUnconfirmedFirst = segment.firstFrame == nil && hasGeneratedFirst
            VStack(alignment: .leading, spacing: 10) {
                if hasUnconfirmedFirst {
                    Button {
                        viewModel.confirmLatestFrames(segment.id,
                                                      useConfirmedLastFrameForVideo: false)
                    } label: {
                        Label(appModel.localized("确认最新首帧", english: "Confirm Latest First Frame"),
                              systemImage: "checkmark.circle.fill")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent).tint(.green)
                    .disabled(editingLocked)
                    Text(appModel.localized("图片已经生成，但尚未选定使用版本。确认不会再次调用模型或产生费用。",
                                            english: "Images exist but no version is selected. Confirming does not call a model or incur cost."))
                        .font(.caption).foregroundStyle(.secondary)
                } else if segment.firstFrame != nil {
                    Button {
                        viewModel.selectedSegments = [segment.id]
                        mediaBatchKind = .videos
                        showsMediaBatch = true
                    } label: {
                        Label(appModel.localized("生成本段视频…", english: "Generate This Segment Video…"),
                              systemImage: "play.rectangle.fill")
                            .font(.headline).frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent).tint(.blue).controlSize(.large)
                    .disabled(viewModel.isBusy || isAgentDraftVisible)
                    Text(appModel.localized("点击后仍会显示费用确认；确认前不会提交视频任务。",
                                            english: "A cost confirmation appears next; no video task is submitted before confirmation."))
                        .font(.caption).foregroundStyle(.secondary)
                } else {
                    Label(appModel.localized("请先生成或上传首帧，再生成本段视频。",
                                             english: "Generate or upload a first frame before creating this video."),
                          systemImage: "photo.badge.plus")
                        .font(.callout).foregroundStyle(.secondary)
                }
            }
            .padding(13)
            .background(Color.blue.opacity(0.055), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Color.blue.opacity(0.16)))
        }
    }

    var batchBar: some View {
        let activeVideos = viewModel.activeVideoGenerationCount(projectID: project.id)
        return HStack(spacing: 16) {
            ZStack {
                Circle().fill(Color.blue.opacity(0.12))
                Image(systemName: "film.stack.fill").foregroundStyle(.blue)
            }.frame(width: 38, height: 38)
            VStack(alignment: .leading, spacing: 5) {
                Text("\(project.completedCount) / \(project.segments.count) " + appModel.localized("段已完成", english: "segments complete")).font(.callout.bold())
                Text(viewModel.isBusy
                     ? viewModel.operation
                     : activeVideos > 0
                        ? appModel.localized("\(activeVideos) 段正在并行生成；仍可继续提交其它就绪分段",
                                             english: "\(activeVideos) running in parallel; more ready segments can still be submitted")
                        : appModel.localized("先确认素材与首帧，再批量生成视频", english: "Confirm assets and first frames before generating videos"))
                    .font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if let playlistGroup {
                Button { playlistPreview = playlistGroup } label: {
                    Label(playlistGroup.isComplete
                          ? appModel.localized("全剧连播", english: "Play Full Story")
                          : appModel.localized("连播已完成分段", english: "Play Completed Segments"),
                          systemImage: "play.fill")
                }
                .buttonStyle(.borderedProminent)
                .tint(.indigo)
            }
            if viewModel.isBusy {
                ProgressView().controlSize(.small)
                Button(viewModel.pauseRequested ? appModel.localized("将在当前步骤结束后暂停", english: "Pausing after this step") : appModel.localized("暂停后续任务", english: "Pause Remaining Tasks")) { viewModel.requestPause() }
                    .disabled(viewModel.pauseRequested)
            } else {
                if activeVideos > 0 { ProgressView().controlSize(.small) }
                Text("\(selectedReady.count) " + appModel.localized("段已选", english: "selected") + " · \(selectedReadySeconds)s").font(.callout)
                Button(appModel.localized("选择全部就绪段", english: "Select Ready Segments")) {
                    viewModel.selectedSegments = Set(project.segments.filter(\.isReady).map(\.id))
                }.disabled(viewModel.isBusy || isAgentDraftVisible)
                Button(appModel.localized("批量生成所选视频", english: "Generate Selected Videos")) {
                    mediaBatchKind = .videos
                    showsMediaBatch = true
                }
                    .buttonStyle(.borderedProminent).tint(.blue)
                    .disabled(viewModel.isBusy || isAgentDraftVisible || selectedReady.isEmpty)
            }
        }
        .padding(.horizontal, 24).padding(.vertical, 12)
        .background(.ultraThinMaterial)
        .overlay(alignment: .top) { Divider().opacity(0.65) }
    }

    func sectionHeading(icon: String, color: Color, title: String, subtitle: String) -> some View {
        HStack(alignment: .center, spacing: 13) {
            ZStack {
                RoundedRectangle(cornerRadius: 11, style: .continuous).fill(color.opacity(0.11))
                Image(systemName: icon).font(.system(size: 19, weight: .semibold)).foregroundStyle(color)
            }.frame(width: 44, height: 44)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.title2.bold())
                Text(subtitle).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    func readinessRow(_ title: String, ready: Bool) -> some View {
        HStack(spacing: 9) {
            Image(systemName: ready ? "checkmark.circle.fill" : "circle")
                .foregroundStyle(ready ? Color.green : .secondary)
            Text(title).font(.callout)
            Spacer()
            Text(ready ? appModel.localized("完成", english: "Ready") : appModel.localized("待完成", english: "Pending"))
                .font(.caption2).foregroundStyle(.secondary)
        }
    }

    func ratioChoice(_ option: String) -> some View {
        let dimensions: CGSize = if option == "9:16" {
            .init(width: 15, height: 25)
        } else if option == "1:1" {
            .init(width: 22, height: 22)
        } else {
            .init(width: 28, height: 17)
        }
        return VStack(spacing: 7) {
            RoundedRectangle(cornerRadius: 3)
                .stroke(ratio == option ? Color.pink : Color.secondary.opacity(0.55), lineWidth: ratio == option ? 2 : 1)
                .frame(width: dimensions.width, height: dimensions.height)
                .frame(height: 27)
            Text(option).font(.caption.bold().monospacedDigit())
        }
        .frame(maxWidth: .infinity).padding(.vertical, 10)
        .foregroundStyle(ratio == option ? Color.pink : .secondary)
        .background(ratio == option ? Color.pink.opacity(0.09) : Color.primary.opacity(0.025),
                    in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 9, style: .continuous)
            .stroke(ratio == option ? Color.pink.opacity(0.28) : Color.primary.opacity(0.05)))
    }

    func portraitMetric(_ value: Int, _ title: String, color: Color) -> some View {
        VStack(spacing: 2) {
            Text("\(value)").font(.headline.monospacedDigit()).foregroundStyle(color)
            Text(title).font(.caption2).foregroundStyle(.secondary)
        }.frame(minWidth: 42)
    }

    func assetKindLabel(_ asset: StoryResource) -> String {
        switch asset.kind {
        case .character: appModel.localized("人物", english: "CHARACTER")
        case .scene: appModel.localized("场景", english: "SCENE")
        case .prop: appModel.localized("道具", english: "PROP")
        }
    }

    func assetKindColor(_ asset: StoryResource) -> Color {
        switch asset.kind {
        case .character: .purple
        case .scene: .orange
        case .prop: .green
        }
    }

    func hasConfirmedAssets(_ segment: StorySegment) -> Bool {
        segment.resourceIDs.allSatisfy { project.resource(id: $0)?.confirmedImage != nil }
    }
    func frameGenerationTitle(_ collection: StoryImageCollection, role: StoryFrameRole) -> String {
        if !collection.images.isEmpty {
            return role == .first
                ? appModel.localized("重新生成首帧", english: "Regenerate First Frame")
                : appModel.localized("重新生成尾帧", english: "Regenerate Last Frame")
        }
        return role == .first
            ? appModel.localized("选择素材并生成首帧", english: "Choose Assets and Generate First Frame")
            : appModel.localized("选择素材并生成尾帧", english: "Choose Assets and Generate Last Frame")
    }
    func status(_ segment: StorySegment) -> String {
        if segment.video != nil { return appModel.localized("视频已完成", english: "Video Complete") }
        if viewModel.isGeneratingVideo(segment.id, projectID: project.id) {
            return appModel.localized("正在生成视频", english: "Generating Video")
        }
        if segment.attempt != nil { return appModel.localized("已提交 · 查询原任务", english: "Submitted · Check Existing Task") }
        if viewModel.isGeneratingFrame(segment.id, role: .first, projectID: project.id) {
            return appModel.localized("正在生成首帧 · 可点开查看", english: "Generating First Frame · Click to View")
        }
        if viewModel.isGeneratingFrame(segment.id, role: .last, projectID: project.id) {
            return appModel.localized("正在生成尾帧 · 可点开查看", english: "Generating Last Frame · Click to View")
        }
        if segment.detail == nil { return appModel.localized("待细化镜头计划", english: "Needs Shot Plan") }
        if segment.firstFrame == nil {
            return segment.firstFrames.images.isEmpty
                ? appModel.localized("待生成首帧", english: "Needs First Frame")
                : appModel.localized("首帧已生成 · 待确认", english: "First Frame Generated · Needs Confirmation")
        }
        return appModel.localized("就绪 · 可以生成视频", english: "Ready to Generate")
    }
    func time(_ seconds: Int) -> String { String(format: "%02d:%02d", seconds / 60, seconds % 60) }
    func rememberDraft() { viewModel.rememberSourceDraft(projectID: project.id, source: source, style: style, ratio: ratio) }
    func confirmPlanning(_ stage: StoryAgentRun.Stage, targets: [String]) {
        planningConfirmation = .init(stage: stage, targets: targets, project: project)
    }
}
