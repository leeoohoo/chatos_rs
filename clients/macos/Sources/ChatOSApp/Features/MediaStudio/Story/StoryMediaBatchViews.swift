import ChatOSCore
import SwiftUI

struct StoryVideoRegenerationStartView: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: StoryStudioViewModel
    let project: StoryProject
    let segmentID: String
    let models: [MediaGenerationModel]
    @State private var additionalIdeas = ""
    @State private var useOriginalVideo = true

    private var currentProject: StoryProject {
        viewModel.projects.first(where: { $0.id == project.id }) ?? project
    }
    private var segment: StorySegment? { currentProject.segments.first { $0.id == segmentID } }
    private var videoModel: MediaGenerationModel? {
        models.first { $0.id == currentProject.models.videoModelID }
    }
    private var usesLastFrame: Bool {
        videoModel?.supportsVideoLastFrame == true
            && segment?.useLastFrameForVideo == true && segment?.lastFrame != nil
    }
    private var validationMessage: String? {
        guard let segment, segment.video != nil else {
            return appModel.localized("当前成片状态已经改变，请关闭后重试。", english: "The current video changed. Close and try again.")
        }
        guard useOriginalVideo || segment.firstFrame != nil else {
            return appModel.localized("当前没有确认首帧，请先调整画面。", english: "No first frame is confirmed. Adjust the frames first.")
        }
        guard let videoModel, videoModel.enabled, videoModel.hasAPIKey else {
            return appModel.localized("当前视频模型不可用，请先检查剧情设置。", english: "The selected video model is unavailable. Check story settings first.")
        }
        guard VideoGenerationProfile(modelName: videoModel.modelName).durations.contains(segment.seconds) else {
            return appModel.localized("当前视频模型不支持这个片段的时长。", english: "The selected video model does not support this segment duration.")
        }
        if useOriginalVideo {
            guard videoModel.supportsVideoReference else {
                return appModel.localized("当前视频模型不支持参考原视频重做。", english: "The selected model cannot use the original video as a reference.")
            }
            guard !StoryGenerationContext.normalizedUserIdeas(additionalIdeas).isEmpty else {
                return appModel.localized("请填写原视频哪里有问题，以及希望如何修改。", english: "Describe what is wrong in the original video and how it should change.")
            }
        }
        return nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(appModel.localized("重新生成本段视频", english: "Regenerate This Video"))
                .font(.title2.bold())
            if let segment {
                VStack(alignment: .leading, spacing: 7) {
                    HStack {
                        Text(segment.title).font(.headline)
                        Spacer()
                        Text("\(segment.seconds)s").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                    }
                    Text(segment.synopsis).font(.callout).foregroundStyle(.secondary)
                }
                .padding(12)
                .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 11, style: .continuous))

                VStack(alignment: .leading, spacing: 9) {
                    Toggle(appModel.localized("参考当前原视频重做", english: "Use Current Video as the Regeneration Reference"),
                           isOn: $useOriginalVideo)
                        .toggleStyle(.switch)
                    if useOriginalVideo {
                        Label(videoModel?.supportsVideoEditing == true
                              ? appModel.localized("会编辑当前成片，按你的要求修改，并尽量保留其他画面",
                                                   english: "Edits the current cut using your request while preserving other content where possible")
                              : appModel.localized("会参考当前成片重新生成；当前模型不能精确编辑原片，整体画面可能变化",
                                                   english: "Regenerates from the current cut as reference; this model cannot precisely edit the source, so the overall result may change"),
                              systemImage: "film.stack.fill")
                    } else if segment.videoGuidanceMode == .previousVideo {
                        Label(videoModel?.supportsVideoExtension == true
                              ? appModel.localized("会从紧邻上一段成片的结尾继续生成",
                                                   english: "Continues from the end of the immediately previous cut")
                              : appModel.localized("会继续参考上一段成片，但不是精确续写",
                                                   english: "Uses the previous cut as reference rather than a precise extension"),
                              systemImage: "film.stack")
                    } else {
                        Label(appModel.localized("当前首帧已确认", english: "Current first frame is confirmed"),
                              systemImage: "checkmark.circle.fill")
                        Label(usesLastFrame
                              ? appModel.localized("本次会同时使用当前尾帧", english: "The current last frame will also be used")
                              : appModel.localized("本次只使用首帧，不使用尾帧", english: "Only the first frame will be used"),
                              systemImage: usesLastFrame ? "2.circle.fill" : "1.circle.fill")
                    }
                    Label(appModel.localized("现有成片会保留在创作记录中，可以随时回看", english: "The existing cut remains in creation history"),
                          systemImage: "clock.arrow.circlepath")
                }
                .font(.callout)
            }

            StoryAdditionalIdeasField(
                text: $additionalIdeas,
                help: appModel.localized("请直接写原视频哪里有问题、要删除什么或人物应该怎么表现。例如：去掉右侧路人；女主更开心，笑意更明显。", english: "Describe what is wrong, what to remove, or how the performance should change. For example: remove the person on the right; make the heroine happier with a clearer smile."),
                maximumLength: StoryGenerationContext.maximumUserIdeasLength
            )

            if let validationMessage {
                Text(validationMessage).font(.caption).foregroundStyle(.orange)
            }
            Text(appModel.localized("确认重新生成后会提交新的 AI 视频任务，并可能产生费用。", english: "Confirming submits a new AI video task and may incur charges."))
                .font(.caption).foregroundStyle(.secondary)

            HStack {
                Button(appModel.localized("取消", english: "Cancel")) { dismiss() }
                Spacer()
                Button(appModel.localized("先调整首帧 / 尾帧", english: "Adjust Frames First")) {
                    viewModel.prepareCompletedVideoForEditing(segmentID)
                    dismiss()
                }
                .disabled(viewModel.isBusy || segment?.video == nil)
                Button(useOriginalVideo
                       ? videoModel?.supportsVideoEditing == true
                            ? appModel.localized("按要求编辑原视频", english: "Edit Original Video")
                            : appModel.localized("参考原视频重新生成", english: "Regenerate from Original Video")
                       : segment?.videoGuidanceMode == .previousVideo
                            ? appModel.localized("按当前衔接方式重新生成", english: "Regenerate with Current Continuity Input")
                            : usesLastFrame
                            ? appModel.localized("确认重新生成", english: "Confirm Regeneration")
                            : appModel.localized("仅使用首帧，确认重新生成", english: "Regenerate with First Frame Only")) {
                    viewModel.regenerateCompletedVideo(
                        segmentID, availableModels: models,
                        useOriginalVideo: useOriginalVideo, userIdeas: additionalIdeas
                    )
                    dismiss()
                }
                .buttonStyle(.borderedProminent)
                .disabled(viewModel.isBusy || validationMessage != nil)
            }
        }
        .padding(24)
        .frame(width: 640)
    }
}

struct StoryMediaBatchStartView: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: StoryStudioViewModel
    let project: StoryProject
    let models: [MediaGenerationModel]
    @State private var kind: StoryMediaBatch.Kind = .pipeline
    @State private var selected: Set<String> = []
    @State private var acceptsAutomaticVersions = false
    @State private var additionalIdeas = ""
    init(viewModel: StoryStudioViewModel, project: StoryProject, models: [MediaGenerationModel],
         initialKind: StoryMediaBatch.Kind = .pipeline) {
        self.viewModel = viewModel; self.project = project; self.models = models
        _kind = State(initialValue: initialKind)
    }
    private var currentProject: StoryProject {
        viewModel.projects.first(where: { $0.id == project.id }) ?? project
    }
    private var candidates: [String] { StoryMediaBatch.candidates(currentProject, kind: kind) }
    private var preview: StoryMediaBatch? {
        try? viewModel.previewMediaBatch(kind: kind, targets: candidates.filter(selected.contains),
                                         models: models, userIdeas: additionalIdeas)
    }
    private var previewError: String? {
        guard !selected.isEmpty else { return nil }
        do {
            _ = try viewModel.previewMediaBatch(kind: kind, targets: candidates.filter(selected.contains),
                                                models: models, userIdeas: additionalIdeas)
            return nil
        }
        catch { return error.localizedDescription }
    }
    private var videoModel: MediaGenerationModel? { models.first { $0.id == currentProject.models.videoModelID } }
    private var unsupportedVideoSegments: [StorySegment] {
        guard kind == .videos || kind == .pipeline, let videoModel else { return [] }
        let durations = VideoGenerationProfile(modelName: videoModel.modelName).durations
        return currentProject.segments.filter {
            selected.contains($0.id) && !durations.contains($0.seconds)
        }
    }
    private var minimumVideoSeconds: Int? {
        videoModel.map { VideoGenerationProfile(modelName: $0.modelName).durations.min() ?? 0 }
    }
    private var previewIncludesVideo: Bool { preview?.steps.contains { $0.kind == .videos } == true }
    private var videoOmitsTailFrame: Bool {
        previewIncludesVideo && videoModel?.supportsVideoLastFrame != true
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(appModel.localized("按已保存描述制作", english: "Produce from Saved Descriptions")).font(.title2.bold())
            Text(appModel.localized("选择要制作的内容和目标。你可以只完成一个环节，也可以一次制作所选分段。", english: "Choose what to create and which items to include. Complete one stage or produce the selected segments in one go."))
                .font(.callout).foregroundStyle(.secondary)
            Picker(appModel.localized("执行范围", english: "Production Stage"), selection: $kind) {
                Text(appModel.localized("一键全流程", english: "Full Pipeline")).tag(StoryMediaBatch.Kind.pipeline)
                Text(appModel.localized("素材图片", english: "Asset Images")).tag(StoryMediaBatch.Kind.assets)
                Text(appModel.localized("分段首帧", english: "First Frames")).tag(StoryMediaBatch.Kind.frames)
                Text(appModel.localized("分段尾帧", english: "Last Frames")).tag(StoryMediaBatch.Kind.lastFrames)
                Text(appModel.localized("分段视频", english: "Videos")).tag(StoryMediaBatch.Kind.videos)
            }.pickerStyle(.segmented)
            StoryAdditionalIdeasField(
                text: $additionalIdeas,
                help: appModel.localized("例如：整体更温暖、减少快速运镜、突出某个角色，或注明所有目标都要遵守的画面要求。", english: "For example: use a warmer look, reduce fast camera movement, emphasize a character, or add a visual requirement for every selected item."),
                maximumLength: StoryGenerationContext.maximumUserIdeasLength
            )
            HStack {
                Text(appModel.localized("只列出当前可执行、尚未完成的目标。", english: "Shows eligible unfinished targets only.")).font(.caption)
                Spacer()
                Button(appModel.localized("全选", english: "Select All")) { selected = Set(candidates) }
                Button(appModel.localized("清空", english: "Clear")) { selected = [] }
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(candidates, id: \.self) { id in
                        Toggle(isOn: Binding(get: { selected.contains(id) }, set: { on in if on { selected.insert(id) } else { selected.remove(id) } })) {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(kind == .assets ? currentProject.resource(id: id)?.name ?? id : currentProject.segments.first { $0.id == id }?.title ?? id)
                                Text(kind == .assets ? currentProject.resource(id: id)?.prompt ?? "" : currentProject.segments.first { $0.id == id }?.synopsis ?? "")
                                    .font(.caption).foregroundStyle(.secondary).lineLimit(2)
                            }
                        }.toggleStyle(.checkbox)
                    }
                    if candidates.isEmpty { Text(appModel.localized("没有就绪目标，请先完成文字计划或确认参考图片。", english: "No eligible targets. Complete the written plan or confirm reference images first.")).foregroundStyle(.secondary) }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }.frame(height: 190)
            if !unsupportedVideoSegments.isEmpty, let videoModel, let minimumVideoSeconds {
                HStack(spacing: 12) {
                    Label(appModel.localized(
                        "\(videoModel.name) 最少需要 \(minimumVideoSeconds) 秒；当前所选包含：\(unsupportedVideoSegments.map { "\($0.title)（\($0.seconds)秒）" }.joined(separator: "、"))。",
                        english: "\(videoModel.name) requires at least \(minimumVideoSeconds) seconds. Some selected segments are shorter."
                    ), systemImage: "clock.badge.exclamationmark")
                        .font(.callout).foregroundStyle(.orange)
                    Spacer(minLength: 8)
                    Button(appModel.localized("一键调整时长", english: "Adjust Durations")) {
                        viewModel.adjustSegmentDurationsForVideoModel(
                            Set(unsupportedVideoSegments.map(\.id)), model: videoModel
                        )
                        dismiss()
                    }
                    .buttonStyle(.borderedProminent).tint(.orange)
                }
                .padding(12)
                .background(Color.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            }
            if kind == .pipeline {
                Toggle(appModel.localized("自动采用本次新生成的素材与首尾帧，继续生成视频", english: "Use newly generated assets and first/last frames automatically, then generate videos"), isOn: $acceptsAutomaticVersions)
                Text(appModel.localized("顺序：缺少的关联素材 → 首帧 → 尾帧 → 所选视频。已有确认版本保持不变；已有未确认图片需先由你选定。", english: "Order: missing linked assets → first frames → last frames → selected videos. Existing confirmed versions are retained; select any existing unconfirmed images yourself first."))
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let preview {
                let imageCount = preview.steps.filter { $0.kind != .videos }.count
                let videoCount = preview.steps.filter { $0.kind == .videos }.count
                let videoSeconds = preview.steps.filter { $0.kind == .videos }.compactMap { step in
                    currentProject.segments.first { $0.id == step.targetID }?.seconds
                }.reduce(0, +)
                Text(appModel.localized("本次将提交：\(imageCount) 张图片，\(videoCount) 段视频，共 \(videoSeconds) 秒。", english: "Will submit \(imageCount) images and \(videoCount) videos totaling \(videoSeconds) seconds."))
                    .font(.headline)
            }
            if videoOmitsTailFrame {
                Label(appModel.localized("本批视频只使用首帧，不会使用已确认的尾帧；尾帧仍会用于衔接下一段画面。",
                                         english: "These videos use first frames only, not confirmed last frames. Last frames still guide continuity into the next segment."),
                      systemImage: "exclamationmark.triangle.fill")
                    .font(.callout.weight(.semibold)).foregroundStyle(.orange)
                    .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.orange.opacity(0.09), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Color.orange.opacity(0.22)))
            }
            if let previewError { Text(previewError).font(.caption).foregroundStyle(.orange) }
            Text(appModel.localized("开始后可能产生 AI 使用费用。未完成的内容之后可以继续。", english: "AI usage charges may apply after you start. Unfinished work can be continued later."))
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button(appModel.localized("取消", english: "Cancel")) { dismiss() }
                Button(videoOmitsTailFrame
                       ? appModel.localized("仅使用首帧，确认执行", english: "Confirm with First Frames Only")
                       : appModel.localized("确认并执行", english: "Confirm and Run")) {
                    if let preview { viewModel.startMediaBatch(preview); dismiss() }
                }.buttonStyle(.borderedProminent).disabled(preview == nil || viewModel.isBusy || (kind == .pipeline && !acceptsAutomaticVersions))
            }
        }.padding(24).frame(width: 720)
        .onAppear { selectDefaults() }
        .onChange(of: kind) { _, _ in selected = Set(candidates); acceptsAutomaticVersions = false }
    }
    private func selectDefaults() {
        let selectedSegments = viewModel.selectedSegments.intersection(Set(candidates))
        selected = selectedSegments.isEmpty ? Set(candidates) : selectedSegments
    }
}

struct StoryMediaBatchPanel: View {
    @EnvironmentObject private var appModel: AppModel
    @ObservedObject var viewModel: StoryStudioViewModel
    @State private var history = false
    @State private var resumeID: UUID?
    @State private var abandonID: UUID?
    var body: some View {
        if let latest = viewModel.projectMediaBatches.first {
            HStack(spacing: 16) {
                ZStack {
                    RoundedRectangle(cornerRadius: 11, style: .continuous)
                        .fill(queueColor(latest).opacity(0.11))
                    Image(systemName: queueIcon(latest))
                        .font(.system(size: 18, weight: .semibold))
                        .foregroundStyle(queueColor(latest))
                }.frame(width: 46, height: 46)
                VStack(alignment: .leading, spacing: 5) {
                    HStack(spacing: 8) {
                        Text(appModel.localized("制作队列", english: "Production Queue"))
                            .font(.headline)
                        Text(kind(latest.kind))
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(queueColor(latest))
                            .padding(.horizontal, 7).padding(.vertical, 3)
                            .background(queueColor(latest).opacity(0.09), in: Capsule())
                        Text(status(latest))
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    ProgressView(value: Double(latest.completedCount), total: Double(max(latest.steps.count, 1)))
                        .tint(queueColor(latest))
                        .frame(maxWidth: 420)
                    if let error = latest.error {
                        Text(error).font(.caption).foregroundStyle(.orange).lineLimit(2)
                    } else {
                        Text(appModel.localized("已完成 \(latest.completedCount) / \(latest.steps.count) 个制作步骤", english: "\(latest.completedCount) of \(latest.steps.count) production steps complete"))
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                Spacer(minLength: 16)
                Text(latest.updatedAt, format: .dateTime.month().day().hour().minute())
                    .font(.caption).foregroundStyle(.secondary)
                Button { history = true } label: {
                    Label(appModel.localized("查看记录与恢复", english: "History & Recovery"), systemImage: "clock.arrow.circlepath")
                }.buttonStyle(.bordered)
            }
            .padding(16)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(queueColor(latest).opacity(0.16)))
            .sheet(isPresented: $history) {
                VStack(alignment: .leading, spacing: 16) {
                    HStack {
                        Text(appModel.localized("制作记录", english: "Production History")).font(.title2.bold())
                        Spacer(); Button(appModel.localized("关闭", english: "Close")) { history = false }
                    }
                    ScrollView {
                        ForEach(viewModel.projectMediaBatches) { batch in
                            VStack(alignment: .leading, spacing: 8) {
                                Text("\(batch.completedCount) / \(batch.steps.count) · " + status(batch)).font(.headline)
                                Text(batch.consentAt, style: .date).font(.caption)
                                if let error = batch.error { Text(error).foregroundStyle(.orange) }
                                if !batch.finished {
                                    HStack {
                                        Button(appModel.localized("恢复批次", english: "Resume Batch")) { history = false; resumeID = batch.id }
                                        Button(appModel.localized("核对后结束批次", english: "End Batch After Verification")) { history = false; abandonID = batch.id }
                                    }.disabled(viewModel.isBusy)
                                }
                                DisclosureGroup(appModel.localized("最近 50 条记录", english: "Last 50 Events")) {
                                    ForEach(Array(batch.events.suffix(50))) { event in Text(event.detail).font(.caption).textSelection(.enabled) }
                                }
                            }.frame(maxWidth: .infinity, alignment: .leading).padding(12)
                        }
                    }
                }.padding(24).frame(width: 700, height: 540)
            }
            .confirmationDialog(appModel.localized("继续执行未完成目标？", english: "Continue Unfinished Targets?"), isPresented: Binding(get: { resumeID != nil }, set: { if !$0 { resumeID = nil } }), titleVisibility: .visible) {
                Button(appModel.localized("确认继续", english: "Confirm and Continue")) { if let resumeID { viewModel.resumeMediaBatch(resumeID) }; resumeID = nil }
            } message: { Text(appModel.localized("将继续完成上次没有完成的内容，已经生成的结果会保留。继续后可能产生 AI 使用费用。", english: "Continues the unfinished work while keeping completed results. Additional AI usage charges may apply.")) }
            .confirmationDialog(appModel.localized("已核实原生成任务？", english: "Have You Verified the Original Tasks?"), isPresented: Binding(get: { abandonID != nil }, set: { if !$0 { abandonID = nil } }), titleVisibility: .visible) {
                Button(appModel.localized("已核对，结束旧批次", english: "Verified — End Old Batch")) { if let abandonID { viewModel.abandonMediaBatch(abandonID) }; abandonID = nil }
            } message: { Text(appModel.localized("图片提交结果不明时，请先核实失败或未创建；已生成的图片可手动导入。结束后可重新发起图片生成，可能再次计费。已有图片、视频和视频任务记录不会删除。", english: "For uncertain image submissions, verify failure or non-creation first; import successful results manually. Ending allows new image submissions that may incur another charge. Saved images, videos and video task records are retained.")) }
        }
    }
    private func status(_ batch: StoryMediaBatch) -> String {
        switch batch.status {
        case .completed: appModel.localized("完成", english: "Complete")
        case .abandoned: appModel.localized("已结束", english: "Ended")
        case .needsReview: appModel.localized("需核对 / 恢复", english: "Review / Resume Required")
        default: viewModel.isBusy ? appModel.localized("执行中", english: "Running") : appModel.localized("已暂停 / 中断", english: "Paused / Interrupted")
        }
    }
    private func kind(_ kind: StoryMediaBatch.Kind) -> String {
        switch kind {
        case .assets: appModel.localized("素材图片", english: "Asset Images")
        case .frames: appModel.localized("分段首帧", english: "First Frames")
        case .lastFrames: appModel.localized("分段尾帧", english: "Last Frames")
        case .videos: appModel.localized("分段视频", english: "Videos")
        case .pipeline: appModel.localized("一键全流程", english: "Full Pipeline")
        }
    }
    private func queueColor(_ batch: StoryMediaBatch) -> Color {
        switch batch.status {
        case .completed: .green
        case .needsReview: .orange
        case .abandoned: .secondary
        default: .blue
        }
    }
    private func queueIcon(_ batch: StoryMediaBatch) -> String {
        switch batch.status {
        case .completed: "checkmark.circle.fill"
        case .needsReview: "exclamationmark.triangle.fill"
        case .abandoned: "stop.circle.fill"
        default: "film.stack.fill"
        }
    }
}
