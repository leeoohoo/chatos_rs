import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct StoryWorkbenchView: View {
    private enum Section: String, CaseIterable {
        case source, style, portraits, segments, graph
    }
    @EnvironmentObject private var appModel: AppModel
    @ObservedObject var viewModel: StoryStudioViewModel
    @ObservedObject var mediaStudio: MediaStudioViewModel
    let project: StoryProject
    let showSettings: () -> Void
    @State private var source = ""
    @State private var style = ""
    @State private var ratio = "16:9"
    @State private var editSegment: StorySegment?
    @State private var imageTarget: StoryImageTarget?
    @State private var preview: MediaStudioImagePreviewRequest?
    @State private var confirmsBatch = false
    @State private var segmentToDelete: String?
    @State private var segmentToRetry: String?
    @State private var agentRunToAbandon: UUID?
    @State private var planningConfirmation: StoryPlanningConfirmation?
    @State private var showsMediaBatch = false
    @State private var mediaBatchKind: StoryMediaBatch.Kind = .pipeline
    @State private var section: Section = .source

    private var selected: StorySegment? { project.segments.first { $0.id == viewModel.selectedSegmentID } }
    private var selectedReady: [StorySegment] { project.segments.filter { viewModel.selectedSegments.contains($0.id) && $0.isReady } }
    private var selectedReadySeconds: Int { selectedReady.reduce(0) { $0 + $1.seconds } }
    private var selectedVideoModel: MediaGenerationModel? {
        mediaStudio.models.first { $0.id == project.models.videoModelID }
    }
    private var videoSendsLastFrame: Bool { selectedVideoModel?.supportsVideoLastFrame == true }
    private var isAgentDraftVisible: Bool { viewModel.isPresentingAgentDraft(projectID: project.id) }
    private var recoverableAgentRun: StoryAgentRun? { viewModel.recoverableAgentRun(projectID: project.id) }
    private var editingLocked: Bool { viewModel.isBusy || isAgentDraftVisible }

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
        .sheet(item: $imageTarget) { target in
            StoryImagePicker(viewModel: viewModel, mediaStudio: mediaStudio, project: project, target: target)
                .environmentObject(appModel)
        }
        .sheet(item: $preview) { request in
            MediaStudioImagePreview(request: request) { mediaStudio.useGeneratedImageForVideo($0) }
                .environmentObject(appModel)
        }
        .confirmationDialog(videoSendsLastFrame
                            ? appModel.localized("确认批量生成视频？", english: "Generate the selected videos?")
                            : appModel.localized("当前接入不会使用尾帧，仍要生成吗？", english: "This connection will not use tail frames. Continue?"),
                            isPresented: $confirmsBatch, titleVisibility: .visible) {
            Button(videoSendsLastFrame
                   ? appModel.localized("确认生成", english: "Confirm Generation")
                   : appModel.localized("仅使用首帧，继续生成", english: "Continue with First Frames Only")) {
                viewModel.generateBatch(availableModels: mediaStudio.models)
            }
        } message: {
            if videoSendsLastFrame {
                Text("\(selectedReady.count) " + appModel.localized("段，共", english: "segments, totaling") + " \(selectedReadySeconds)s。"
                     + appModel.localized("已启用的确认尾帧会随视频请求发送；可能产生费用。失败时暂停，不自动重试。",
                                          english: "Enabled confirmed tail frames are sent with the video request. Charges may apply. Pauses on failure without automatic retries."))
            } else {
                Text(appModel.localized("当前 \(selectedVideoModel?.modelName ?? "视频模型") 通过 OpenAI 兼容 /v1/videos 接入。本次请求只发送首帧，已确认尾帧不会发送给视频模型；尾帧仅用于衔接下一段首帧。生成可能产生费用。",
                                        english: "The current \(selectedVideoModel?.modelName ?? "video model") uses the OpenAI-compatible /v1/videos connection. This request sends only the first frame; confirmed tail frames are not sent to the video model and are used only to anchor the next segment's first frame. Charges may apply."))
            }
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

    private var header: some View {
        HStack(spacing: 16) {
            Button { viewModel.backToList() } label: {
                Image(systemName: "chevron.left")
                    .font(.system(size: 13, weight: .semibold))
                    .frame(width: 32, height: 32)
                    .background(Color.primary.opacity(0.055), in: Circle())
            }.buttonStyle(.plain).help(appModel.localized("返回剧情列表", english: "Back to stories"))
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 9) {
                    Text(project.title).font(.title3.weight(.semibold)).lineLimit(1)
                    Text(projectPhase)
                        .font(.caption2.weight(.semibold))
                        .padding(.horizontal, 8).padding(.vertical, 4)
                        .foregroundStyle(projectPhaseColor)
                        .background(projectPhaseColor.opacity(0.1), in: Capsule())
                }
                HStack(spacing: 6) {
                    Image(systemName: hasUnsavedChanges ? "circle.fill" : "checkmark.circle.fill")
                        .font(.system(size: 8))
                    Text(hasUnsavedChanges
                         ? appModel.localized("有未保存修改", english: "Unsaved changes")
                         : appModel.localized("所有更改已保存到本机", english: "All changes saved locally"))
                }.font(.caption).foregroundStyle(hasUnsavedChanges ? Color.orange : .secondary)
            }
            Spacer()
            headerMetric(value: "\(project.resources.count)", label: appModel.localized("素材", english: "Assets"), icon: "photo.on.rectangle.angled")
            headerMetric(value: "\(project.segments.count)", label: appModel.localized("分段", english: "Segments"), icon: "rectangle.stack")
            headerMetric(value: "\(project.totalSeconds)s", label: appModel.localized("计划时长", english: "Runtime"), icon: "clock")
            if viewModel.isBusy {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text(viewModel.operation).lineLimit(1)
                }
                .font(.caption).foregroundStyle(.secondary)
                .padding(.horizontal, 12).padding(.vertical, 8)
                .background(Color.accentColor.opacity(0.08), in: Capsule())
            }
            let activeVideos = viewModel.activeVideoGenerationCount(projectID: project.id)
            if activeVideos > 0 {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text(appModel.localized(
                        "\(activeVideos) 段视频并行生成中",
                        english: "\(activeVideos) videos generating"
                    ))
                }
                .font(.caption).foregroundStyle(.blue)
                .padding(.horizontal, 12).padding(.vertical, 8)
                .background(Color.blue.opacity(0.08), in: Capsule())
            }
            Button { showSettings() } label: {
                Image(systemName: "slider.horizontal.3")
                    .font(.system(size: 14, weight: .semibold))
                    .frame(width: 34, height: 34)
                    .background(Color.primary.opacity(0.055), in: Circle())
            }.buttonStyle(.plain).help(appModel.localized("剧情设置", english: "Story Settings"))
                .disabled(editingLocked)
        }
        .padding(.horizontal, 24).padding(.vertical, 14)
        .background(.ultraThinMaterial)
    }

    private var sectionBar: some View {
        return HStack(spacing: 10) {
            sectionButton(.source, index: 1, "剧情原文", "Story", "doc.text", nil)
            sectionButton(.style, index: 2, "画面风格", "Visual Style", "paintpalette", nil)
            sectionButton(.portraits, index: 3, "角色画像", "Portraits", "person.crop.rectangle.stack", project.resources.count)
            sectionButton(.segments, index: 4, "拍摄分段", "Shot Segments", "rectangle.stack", project.segments.count)
            sectionButton(.graph, index: 5, "关系图谱", "Relationship Graph", "point.3.connected.trianglepath.dotted", project.relations.count)
            Spacer(minLength: 8)
            Button { mediaBatchKind = .pipeline; showsMediaBatch = true } label: {
                Label(appModel.localized("一键制作", english: "Produce All"), systemImage: "sparkles")
                    .fontWeight(.semibold)
                    .padding(.horizontal, 3).padding(.vertical, 4)
            }
            .buttonStyle(.borderedProminent).tint(.indigo)
            .disabled(editingLocked || project.segments.isEmpty)
        }
        .padding(.horizontal, 24).padding(.vertical, 12)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.76))
        .overlay(alignment: .bottom) { Divider().opacity(0.65) }
    }

    private var agentDraftBanner: some View {
        let interrupted = !isAgentDraftVisible ? recoverableAgentRun : nil
        return HStack(spacing: 12) {
            ZStack {
                Circle().fill(Color.purple.opacity(0.13))
                Image(systemName: isAgentDraftVisible ? "waveform" : "doc.badge.clock")
                    .font(.system(size: 14, weight: .semibold)).foregroundStyle(.purple)
            }.frame(width: 34, height: 34)
            VStack(alignment: .leading, spacing: 3) {
                Text(isAgentDraftVisible
                     ? appModel.localized("正在展示实时规划草稿", english: "Showing the Live Planning Draft")
                     : appModel.localized("上次规划已中断，草稿已安全保存", english: "The Previous Planning Run Was Interrupted — Draft Saved"))
                    .font(.callout.weight(.semibold))
                if isAgentDraftVisible {
                    Text(appModel.localized(
                        "已生成的 \(project.characters.count) 个人物、\(project.scenes.count) 个场景、\(project.segments.count) 个分段会实时显示；完整校验通过后再一次性提交。",
                        english: "Live results: \(project.characters.count) characters, \(project.scenes.count) scenes and \(project.segments.count) segments. The project is committed only after full validation."
                    )).font(.caption).foregroundStyle(.secondary)
                } else {
                    Text(appModel.localized(
                        "正式项目已恢复为可用状态。你可以继续其它制作，也可以稍后恢复这份规划草稿。",
                        english: "The project is usable again. Continue other production work now, or resume this planning draft later."
                    )).font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer()
            if let interrupted {
                Button(appModel.localized("放弃草稿", english: "Discard Draft"), role: .destructive) {
                    agentRunToAbandon = interrupted.id
                }.buttonStyle(.bordered)
                Button(appModel.localized("查看草稿", english: "Review Draft")) {
                    withAnimation(.easeOut(duration: 0.16)) { section = .style }
                }.buttonStyle(.bordered).tint(.purple)
                Button(appModel.localized("继续规划", english: "Resume Planning")) {
                    viewModel.resumeAgent(interrupted.id)
                }.buttonStyle(.borderedProminent).tint(.purple)
            } else {
                Button(appModel.localized("查看规划进度", english: "View Planning Progress")) {
                    withAnimation(.easeOut(duration: 0.16)) { section = .style }
                }.buttonStyle(.bordered).tint(.purple)
            }
        }
        .padding(.horizontal, 24).padding(.vertical, 10)
        .background(Color.purple.opacity(0.055))
        .overlay(alignment: .bottom) { Divider().opacity(0.5) }
    }

    private func sectionButton(_ item: Section, index: Int, _ chinese: String, _ english: String,
                               _ icon: String, _ count: Int?) -> some View {
        Button {
            withAnimation(.easeOut(duration: 0.16)) { section = item }
        } label: {
            HStack(spacing: 10) {
                ZStack {
                    Circle().fill(section == item ? sectionColor(item) : Color.primary.opacity(0.06))
                    Text("\(index)").font(.caption2.bold())
                        .foregroundStyle(section == item ? Color.white : .secondary)
                }.frame(width: 25, height: 25)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 5) {
                        Image(systemName: icon).font(.caption)
                        Text(appModel.localized(chinese, english: english))
                            .font(.callout.weight(section == item ? .semibold : .medium))
                    }
                    Text(sectionHint(item)).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                }
                if let count, count > 0 {
                    Text("\(count)").font(.caption2.bold().monospacedDigit())
                        .padding(.horizontal, 6).padding(.vertical, 3)
                        .background(Color.primary.opacity(0.055), in: Capsule())
                }
            }
            .foregroundStyle(section == item ? sectionColor(item) : .primary)
            .padding(.horizontal, 11).padding(.vertical, 9)
            .frame(minWidth: 124, alignment: .leading)
            .background(section == item ? sectionColor(item).opacity(0.09) : Color.clear,
                        in: RoundedRectangle(cornerRadius: 11, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 11, style: .continuous)
                    .stroke(section == item ? sectionColor(item).opacity(0.22) : .clear)
            }
            // Plain buttons otherwise derive their hit-test area from rendered glyphs, so the
            // visible padding between the icon, labels, and count can feel intermittently dead.
            .contentShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
        }.buttonStyle(.plain)
    }

    private var hasUnsavedChanges: Bool {
        source != project.source || style != project.style || ratio != project.ratio
    }

    private var projectPhase: String {
        if project.segments.isEmpty { return appModel.localized("准备中", english: "Setup") }
        if project.completedCount == project.segments.count { return appModel.localized("已完成", english: "Complete") }
        if project.hasUnresolvedJobs { return appModel.localized("任务处理中", english: "Processing") }
        return appModel.localized("制作中", english: "In Production")
    }

    private var projectPhaseColor: Color {
        if project.segments.isEmpty { return .orange }
        if project.completedCount == project.segments.count { return .green }
        return .indigo
    }

    private func headerMetric(value: String, label: String, icon: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: icon).foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 1) {
                Text(value).font(.callout.bold().monospacedDigit())
                Text(label).font(.caption2).foregroundStyle(.secondary)
            }
        }.padding(.horizontal, 10)
    }

    private func sectionColor(_ item: Section) -> Color {
        switch item {
        case .source: .indigo
        case .style: .pink
        case .portraits: .orange
        case .segments: .blue
        case .graph: .teal
        }
    }

    private func sectionHint(_ item: Section) -> String {
        switch item {
        case .source: appModel.localized("故事与叙事", english: "Narrative")
        case .style: appModel.localized("视觉基调", english: "Direction")
        case .portraits: appModel.localized("人物与场景", english: "Assets")
        case .segments: appModel.localized("分段与转场", english: "Shots & transitions")
        case .graph: appModel.localized("引用与关系", english: "Connections")
        }
    }

    @ViewBuilder private var workspaceContent: some View {
        switch section {
        case .source: sourcePanel
        case .style: stylePanel
        case .portraits: portraitsPanel
        case .segments: segmentsPanel
        case .graph:
            StoryRelationGraphView(project: project, selectedSegmentID: viewModel.selectedSegmentID,
                                   openAsset: { imageTarget = .init(assetID: $0, segmentID: nil) },
                                   openSegment: { viewModel.selectedSegmentID = $0; section = .segments })
        }
    }

    private var sourcePanel: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                ViewThatFits(in: .horizontal) {
                    HStack(alignment: .top, spacing: 20) {
                        sourceEditor.frame(minWidth: 620, maxWidth: .infinity)
                        sourceSidebar.frame(width: 292)
                    }
                    VStack(alignment: .leading, spacing: 18) {
                        sourceEditor
                        sourceSidebar
                    }
                }
                if let error = viewModel.errorMessage { Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
            }.padding(1)
        }
    }

    private var sourceEditor: some View {
        VStack(alignment: .leading, spacing: 18) {
            sectionHeading(icon: "doc.text.fill", color: .indigo,
                           title: appModel.localized("剧情原文", english: "Complete Story"),
                           subtitle: appModel.localized("把完整剧情放在这里，先解决人物动机、冲突和叙事节奏。", english: "Shape the complete narrative, motivations, conflict and pacing here."))
            ZStack(alignment: .topLeading) {
                TextEditor(text: $source)
                    .font(.system(size: 14.5, weight: .regular, design: .serif))
                    .lineSpacing(5)
                    .scrollContentBackground(.hidden)
                    .padding(15)
                    .frame(minHeight: 520)
                    .background(Color(nsColor: .textBackgroundColor).opacity(0.72),
                                in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                if source.isEmpty {
                    Text(appModel.localized("在这里粘贴完整剧情……", english: "Paste the complete story here…"))
                        .font(.body).foregroundStyle(.tertiary).padding(.leading, 21).padding(.top, 22)
                        .allowsHitTesting(false)
                }
            }
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(hasUnsavedChanges ? Color.orange.opacity(0.35) : Color.primary.opacity(0.08)))
            .disabled(!project.segments.isEmpty || editingLocked)
            HStack(spacing: 10) {
                Label("\(source.count) / 80,000", systemImage: "character.cursor.ibeam")
                    .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                if !project.segments.isEmpty {
                    Label(appModel.localized("规划后已锁定", english: "Locked after planning"), systemImage: "lock.fill")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button { viewModel.optimizeSourceDraft(source, style: style, target: .source) } label: {
                    Label(appModel.localized("AI 优化", english: "Improve with AI"), systemImage: "wand.and.stars")
                }.disabled(editingLocked || !project.segments.isEmpty || source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button(appModel.localized("保存原文", english: "Save Story")) {
                    viewModel.saveSource(source, style: style, ratio: ratio)
                }.buttonStyle(.borderedProminent).tint(.indigo)
                    .disabled(editingLocked || !project.segments.isEmpty || source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            if viewModel.optimizationTarget == .source { optimizationSuggestion }
        }.padding(22).storySurface()
    }

    private var sourceSidebar: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 8) {
                Label(appModel.localized("项目简介", english: "Project Brief"), systemImage: "text.alignleft")
                    .font(.headline)
                Text(project.description.isEmpty
                     ? appModel.localized("还没有项目描述，可在剧情设置中补充。", english: "No project description yet. Add one in Story Settings.")
                     : project.description)
                    .font(.callout).foregroundStyle(.secondary).lineLimit(7)
            }
            Divider()
            VStack(alignment: .leading, spacing: 12) {
                Text(appModel.localized("规划准备", english: "Planning Readiness")).font(.headline)
                readinessRow(appModel.localized("剧情原文", english: "Story text"), ready: !project.source.isEmpty)
                readinessRow(appModel.localized("画面风格", english: "Visual direction"), ready: !project.style.isEmpty)
                readinessRow(appModel.localized("全剧拆分", english: "Story breakdown"), ready: !project.segments.isEmpty)
            }
            Divider()
            Text(appModel.localized("AI 优化只生成候选文本，不会直接覆盖原文，也不会生成图片或视频。", english: "AI improvement creates a suggestion only. It never overwrites your story or generates media."))
                .font(.caption).foregroundStyle(.secondary)
            Button {
                withAnimation { section = .style }
            } label: {
                HStack {
                    Text(appModel.localized("下一步：画面风格", english: "Next: Visual Style"))
                    Spacer()
                    Image(systemName: "arrow.right")
                }.padding(.vertical, 4)
            }.buttonStyle(.bordered).disabled(project.source.isEmpty && source.isEmpty)
        }.padding(20).storySurface(tint: .indigo)
    }

    private var stylePanel: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                ViewThatFits(in: .horizontal) {
                    HStack(alignment: .top, spacing: 20) {
                        styleEditor.frame(minWidth: 580, maxWidth: .infinity)
                        planningCard.frame(width: 320)
                    }
                    VStack(alignment: .leading, spacing: 18) {
                        styleEditor
                        planningCard
                    }
                }
                if viewModel.latestAgentRun != nil || viewModel.isLoadingAgentRuns {
                    VStack(alignment: .leading, spacing: 12) {
                        Label(appModel.localized("最近的 AI 规划", english: "Recent AI Planning"), systemImage: "sparkles.rectangle.stack")
                            .font(.headline)
                        StoryAgentRunPanel(viewModel: viewModel)
                    }.padding(20).storySurface(tint: .purple)
                }
                if let error = viewModel.errorMessage { Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
            }.padding(1)
        }
    }

    private var styleEditor: some View {
        VStack(alignment: .leading, spacing: 18) {
            sectionHeading(icon: "paintpalette.fill", color: .pink,
                           title: appModel.localized("画面风格", english: "Visual Direction"),
                           subtitle: appModel.localized("建立全剧统一的美术、光线、色彩与镜头质感。", english: "Establish one visual language for art, lighting, color and camera texture."))
            TextEditor(text: $style)
                .font(.body).lineSpacing(4).scrollContentBackground(.hidden).padding(15)
                .frame(minHeight: 360)
                .background(Color(nsColor: .textBackgroundColor).opacity(0.72),
                            in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Color.primary.opacity(0.08)))
                .disabled(!project.segments.isEmpty || editingLocked)
            HStack {
                Text("\(style.count) / 2,000").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                Spacer()
                Button { viewModel.optimizeSourceDraft(source, style: style, target: .style) } label: {
                    Label(appModel.localized("AI 优化风格", english: "Improve Style"), systemImage: "wand.and.stars")
                }.disabled(editingLocked || !project.segments.isEmpty || style.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button(appModel.localized("保存风格", english: "Save Direction")) {
                    viewModel.saveSource(source, style: style, ratio: ratio)
                }.buttonStyle(.borderedProminent).tint(.pink)
                    .disabled(editingLocked || source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            if viewModel.optimizationTarget == .style { optimizationSuggestion }
        }.padding(22).storySurface()
    }

    private var planningCard: some View {
        VStack(alignment: .leading, spacing: 18) {
            Label(appModel.localized("画幅与规划", english: "Format & Planning"), systemImage: "viewfinder")
                .font(.headline)
            Text(appModel.localized("选择最终成片比例。全剧规划会生成角色、场景、道具、2–15秒剧情段和必要的独立转场段。", english: "Choose the final frame. Planning creates characters, scenes, props, 2–15 second story segments, and independent transitions where needed."))
                .font(.callout).foregroundStyle(.secondary)
            HStack(spacing: 8) {
                ForEach(["16:9", "9:16", "1:1"], id: \.self) { option in
                    Button { ratio = option } label: { ratioChoice(option) }.buttonStyle(.plain)
                        .disabled(!project.segments.isEmpty || editingLocked)
                }
            }
            Divider()
            if project.segments.isEmpty {
                VStack(alignment: .leading, spacing: 10) {
                    readinessRow(appModel.localized("原文已保存", english: "Story saved"), ready: !project.source.isEmpty && source == project.source)
                    readinessRow(appModel.localized("风格已保存", english: "Direction saved"), ready: style == project.style && ratio == project.ratio)
                }
                Button {
                    confirmPlanning(.outline, targets: [])
                } label: {
                    HStack {
                        Image(systemName: "sparkles")
                        VStack(alignment: .leading, spacing: 2) {
                            Text(appModel.localized("分析全剧并建立制作计划", english: "Analyze & Build Production Plan")).fontWeight(.semibold)
                            Text(appModel.localized("只生成文字计划，不生成媒体", english: "Text plan only — no media generation")).font(.caption2)
                        }
                        Spacer()
                        Image(systemName: "arrow.right")
                    }.padding(.vertical, 7)
                }.buttonStyle(.borderedProminent).tint(.indigo)
                    .disabled(editingLocked || project.source.isEmpty || source != project.source || style != project.style || ratio != project.ratio)
            } else {
                VStack(alignment: .leading, spacing: 7) {
                    Text(appModel.localized("全剧计划", english: "Production Plan")).font(.caption).foregroundStyle(.secondary)
                    Text("\(project.segments.count) " + appModel.localized("个分段", english: "segments"))
                        .font(.title2.bold().monospacedDigit()).foregroundStyle(.indigo)
                    Text(appModel.localized("总计 \(project.totalSeconds) 秒", english: "\(project.totalSeconds) seconds total"))
                        .font(.callout).foregroundStyle(.secondary)
                    Label(appModel.localized("原文和风格已锁定", english: "Story and direction locked"), systemImage: "lock.fill")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Button { withAnimation { section = .portraits } } label: {
                    HStack { Text(appModel.localized("查看角色与场景", english: "Review Portraits")); Spacer(); Image(systemName: "arrow.right") }
                }.buttonStyle(.bordered)
            }
        }.padding(20).storySurface(tint: .pink)
    }

    @ViewBuilder private var optimizationSuggestion: some View {
        if let suggestion = viewModel.optimizationSuggestion, let target = viewModel.optimizationTarget {
            GroupBox {
                VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        Label(target == .source ? appModel.localized("AI 剧情优化建议", english: "AI Story Suggestion")
                              : appModel.localized("AI 风格优化建议", english: "AI Style Suggestion"), systemImage: "sparkles")
                            .font(.headline).foregroundStyle(.purple)
                        Spacer()
                        Button(appModel.localized("放弃", english: "Discard")) { viewModel.clearOptimizationSuggestion() }
                        Button(appModel.localized("采用建议", english: "Apply Suggestion")) {
                            if target == .source { source = suggestion.optimizedText } else { style = suggestion.optimizedText }
                            viewModel.clearOptimizationSuggestion()
                        }.buttonStyle(.borderedProminent).tint(.purple)
                    }
                    Text(suggestion.rationale).font(.callout).foregroundStyle(.secondary)
                    ScrollView { Text(suggestion.optimizedText).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading) }
                        .frame(maxHeight: 220).padding(10)
                        .background(Color.purple.opacity(0.04), in: RoundedRectangle(cornerRadius: 8))
                }.padding(8)
            }
        }
    }

    private var portraitsPanel: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                HStack(alignment: .center, spacing: 18) {
                    sectionHeading(icon: "person.crop.rectangle.stack.fill", color: .orange,
                                   title: appModel.localized("角色与场景画像", english: "Portrait Library"),
                                   subtitle: appModel.localized("先用文字建立一致性，再为每个角色、场景与道具确认视觉版本。", english: "Define consistency in words, then confirm a visual version for every character, scene and prop."))
                    Spacer(minLength: 12)
                    HStack(spacing: 8) {
                        portraitMetric(project.characters.count, appModel.localized("人物", english: "People"), color: .purple)
                        portraitMetric(project.scenes.count, appModel.localized("场景", english: "Scenes"), color: .orange)
                        portraitMetric(project.props.count, appModel.localized("道具", english: "Props"), color: .green)
                    }
                    Button {
                        mediaBatchKind = .assets; showsMediaBatch = true
                    } label: {
                        Label(appModel.localized("生成全部素材", english: "Generate All Assets"), systemImage: "photo.stack.fill")
                    }.buttonStyle(.borderedProminent).tint(.orange)
                        .disabled(editingLocked || project.resources.isEmpty)
                }.padding(22).storySurface(tint: .orange)
                if !viewModel.projectMediaBatches.isEmpty {
                    StoryMediaBatchPanel(viewModel: viewModel)
                }
                if project.resources.isEmpty {
                    ContentUnavailableView {
                        Label(appModel.localized("尚未生成人物与场景画像", english: "No Portraits Yet"), systemImage: "person.crop.rectangle.stack")
                    } description: {
                        Text(appModel.localized("先在“剧情原文”完成并启动全剧规划。", english: "Complete and plan the story in the Story tab first."))
                    } actions: {
                        Button(appModel.localized("前往剧情原文", english: "Open Story")) { section = .source }
                    }.frame(minHeight: 420).storySurface()
                } else {
                    portraitSection(appModel.localized("人物画像", english: "Characters"), resources: project.resources.filter { $0.kind == .character })
                    portraitSection(appModel.localized("场景画像", english: "Scenes"), resources: project.resources.filter { $0.kind == .scene })
                    portraitSection(appModel.localized("关键道具", english: "Props"), resources: project.resources.filter { $0.kind == .prop })
                }
            }.padding(1)
        }
    }

    @ViewBuilder private func portraitSection(_ title: String, resources: [StoryResource]) -> some View {
        if !resources.isEmpty {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Text(title).font(.title3.bold())
                    Text("\(resources.count)").font(.caption.bold().monospacedDigit())
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 7).padding(.vertical, 3)
                        .background(Color.primary.opacity(0.055), in: Capsule())
                    Spacer()
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 350, maximum: 520), spacing: 16)], spacing: 16) {
                    ForEach(resources) { asset in portraitCard(asset) }
                }
            }.padding(20).storySurface()
        }
    }

    private func portraitCard(_ asset: StoryResource) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            let isGenerating = viewModel.isGeneratingAsset(asset.id, projectID: project.id)
            HStack(alignment: .top, spacing: 16) {
                let displayed = asset.confirmedImage ?? asset.images.last
                Button {
                    if let displayed, let media = viewModel.mediaAsset(displayed, projectID: project.id) {
                        preview = .init(images: [media])
                    }
                } label: {
                    StoryThumbnail(asset: displayed.flatMap { viewModel.mediaAsset($0, projectID: project.id) })
                        .frame(width: 146, height: 146).clipped()
                        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                        .overlay(alignment: .bottomTrailing) {
                            if displayed != nil {
                                Image(systemName: "arrow.up.left.and.arrow.down.right")
                                    .font(.caption2.bold()).padding(7)
                                    .background(.ultraThinMaterial, in: Circle()).padding(7)
                            }
                        }
                }.buttonStyle(.plain).disabled(displayed == nil)
                VStack(alignment: .leading, spacing: 9) {
                    Text(assetKindLabel(asset)).font(.caption2.bold()).foregroundStyle(assetKindColor(asset))
                        .padding(.horizontal, 7).padding(.vertical, 3)
                        .background(assetKindColor(asset).opacity(0.09), in: Capsule())
                    Text(asset.name).font(.title3.weight(.semibold)).lineLimit(2)
                    Label(asset.confirmedImage != nil ? appModel.localized("图片已确认", english: "Image Confirmed")
                          : asset.images.isEmpty ? appModel.localized("等待生成图片", english: "Needs Image")
                          : appModel.localized("请选择图片版本", english: "Choose an Image Version"),
                          systemImage: asset.confirmedImage != nil ? "checkmark.circle.fill" : "circle.dashed")
                        .font(.caption).foregroundStyle(asset.confirmedImage != nil ? Color.green : .secondary)
                    Text(portraitSummary(asset)).font(.caption).foregroundStyle(.secondary).lineLimit(4)
                }
            }
            VStack(alignment: .leading, spacing: 5) {
                Text(appModel.localized("生成提示词", english: "Generation Prompt")).font(.caption2.bold()).foregroundStyle(.secondary)
                Text(asset.prompt).font(.callout).lineLimit(4).textSelection(.enabled)
            }
            .padding(11).frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.primary.opacity(0.03), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            Button {
                imageTarget = .init(assetID: asset.id, segmentID: nil)
            } label: {
                if isGenerating {
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.small)
                        Text(appModel.localized("正在生成此素材，可继续生成其他素材", english: "Generating — Other Assets Remain Available"))
                    }
                } else {
                    Label(asset.confirmedImage == nil ? appModel.localized("生成 / 上传 / 选择图片", english: "Generate / Upload / Choose Image")
                          : appModel.localized("查看与更换图片版本", english: "Review or Change Image"), systemImage: "photo.badge.plus")
                }
            }
            .buttonStyle(.borderedProminent).tint(assetKindColor(asset)).disabled(editingLocked)
            .frame(maxWidth: .infinity)
        }
        .padding(16)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.72),
                    in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Color.primary.opacity(0.07)))
    }

    private func portraitSummary(_ asset: StoryResource) -> String {
        if let profile = asset.characterProfile {
            return [profile.isProtagonist ? appModel.localized("主角", english: "Protagonist") : appModel.localized("人物", english: "Character"),
                    profile.roleInStory, profile.personality, profile.motivation].joined(separator: " · ")
        }
        if let profile = asset.sceneProfile {
            return [profile.roleInStory, profile.setting, profile.atmosphere].joined(separator: " · ")
        }
        return asset.prompt
    }

    private var segmentsPanel: some View {
        VStack(alignment: .leading, spacing: 16) {
            if !viewModel.projectMediaBatches.isEmpty {
                StoryMediaBatchPanel(viewModel: viewModel)
            }
            HStack(alignment: .top, spacing: 20) {
                overview.frame(width: 390)
                detailPanel.frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    private var overview: some View {
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
                Text(appModel.localized("每段单独生成；总时长是计划时长，暂不自动拼接成片。", english: "Segments generate separately. Total duration is planned length, not an automatically assembled movie."))
                    .font(.caption).foregroundStyle(.secondary)
            }
        }.padding(18).storySurface(tint: .blue)
    }

    private func segmentRow(_ segment: StorySegment, index: Int) -> some View {
        let start = project.segments.prefix(index).reduce(0) { $0 + $1.seconds }
        let tint: Color = segment.kind == .transition ? .purple : .blue
        return HStack(spacing: 10) {
            Toggle("", isOn: Binding(get: { viewModel.selectedSegments.contains(segment.id) }, set: { on in
                if on { viewModel.selectedSegments.insert(segment.id) } else { viewModel.selectedSegments.remove(segment.id) }
            })).labelsHidden().toggleStyle(.checkbox).disabled(!segment.isReady || editingLocked)
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

    @ViewBuilder private var detailPanel: some View {
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
                                if let jobID = attempt.jobID {
                                    Text(appModel.localized("任务 ID：\(jobID)", english: "Task ID: \(jobID)"))
                                        .font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                                } else {
                                    Text(appModel.localized(
                                        "正在等待服务返回任务 ID，无需手动填写。",
                                        english: "Waiting for the service to return a task ID. No manual input is needed."
                                    ))
                                    .font(.caption).foregroundStyle(.secondary)
                                }
                            }
                            .padding(12)
                            .background(Color.blue.opacity(0.055), in: RoundedRectangle(cornerRadius: 11, style: .continuous))
                        } else {
                            if let jobID = attempt.jobID {
                                Text(appModel.localized("任务 ID：\(jobID)", english: "Task ID: \(jobID)"))
                                    .font(.caption).textSelection(.enabled)
                                Button(appModel.localized("查询原任务 / 恢复下载", english: "Check Task / Resume Download")) {
                                    viewModel.resumeVideo(segment.id)
                                }
                                .disabled(editingLocked)
                            } else {
                                Text(appModel.localized(
                                    "提交结果未返回任务 ID。客户端不会要求手动填写；请先核对服务商记录，再决定是否允许重试。",
                                    english: "No task ID was returned. The app never asks you to enter one manually; check the provider record before allowing a retry."
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
                        if videoModel.supportsVideoLastFrame {
                            Toggle(appModel.localized("生成视频时使用已确认尾帧", english: "Use Confirmed Last Frame for Video"),
                                   isOn: Binding(get: { segment.useLastFrameForVideo }, set: {
                                viewModel.setUseLastFrameForVideo($0, segmentID: segment.id)
                            }))
                            .disabled(editingLocked || segment.lastFrame == nil)
                        } else {
                            Label(appModel.localized("注意：当前 \(videoModel.modelName) 通过 OpenAI 兼容 /v1/videos 接入。本次视频请求不会发送尾帧，只发送首帧；尾帧仅用于生成下一段的连续首帧。",
                                                     english: "Notice: \(videoModel.modelName) currently uses the OpenAI-compatible /v1/videos connection. Tail frames are not sent with this video request; only first frames are sent. Tail frames are used only to create a continuous first frame for the next segment."),
                                  systemImage: "exclamationmark.triangle.fill")
                                .font(.callout.weight(.semibold)).foregroundStyle(.orange)
                                .padding(11).frame(maxWidth: .infinity, alignment: .leading)
                                .background(Color.orange.opacity(0.09), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                                .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Color.orange.opacity(0.22)))
                        }
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
                        .disabled(editingLocked || project.hasUnresolvedJobs || segment.video != nil)
                        .help(appModel.localized("旧图片版本会保留；新计划应用后需要重新确认首帧与尾帧。",
                                                 english: "Existing image versions are kept; first and last frames must be confirmed again after applying the new plan."))
                    } else {
                        Text(appModel.localized("此段只有剧情概要，尚未展开镜头计划。", english: "This segment has an outline but no detailed shot plan yet."))
                            .font(.callout).foregroundStyle(.secondary)
                        Button(appModel.localized("细化这个 \(segment.seconds) 秒分段", english: "Refine This \(segment.seconds)-second Segment")) { confirmPlanning(.refine, targets: [segment.id]) }
                            .disabled(editingLocked)
                    }
                }.padding(20)
            }.storySurface(tint: .blue)
        } else {
            ContentUnavailableView(appModel.localized("分段详情", english: "Segment Details"), systemImage: "sidebar.right",
                                   description: Text(appModel.localized("选择一段，编辑镜头、素材和首帧。", english: "Select a segment to edit its shots, assets and first frame.")))
                .frame(maxHeight: .infinity).storySurface(tint: .blue)
        }
    }

    private func frameCard(_ segment: StorySegment, role: StoryFrameRole) -> some View {
        let collection = segment.frames(for: role)
        let frame = collection.confirmedImage ?? collection.images.last
        let isFirst = role == .first
        let previousTail = isFirst ? StoryContinuityContext.previousTail(project, segmentID: segment.id) : nil
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
                }
            }
            if directlyInherited {
                Label(appModel.localized("已直接承接上一段尾帧 · 未调用图片模型",
                                         english: "Directly inherited from the previous tail · No image model call"),
                      systemImage: "link.circle.fill")
                    .font(.caption2.weight(.semibold)).foregroundStyle(.green)
            }
            Button {
                if let frame, let asset = viewModel.mediaAsset(frame, projectID: project.id) { preview = .init(images: [asset]) }
            } label: {
                StoryThumbnail(asset: frame.flatMap { viewModel.mediaAsset($0, projectID: project.id) })
                    .frame(height: 118).frame(maxWidth: .infinity).clipped().clipShape(RoundedRectangle(cornerRadius: 8))
            }.buttonStyle(.plain).disabled(frame == nil)
            Button(isFirst
                   ? appModel.localized("选择 / 上传 / 确认首帧", english: "Choose / Upload / Confirm First Frame")
                   : appModel.localized("选择 / 上传 / 确认尾帧", english: "Choose / Upload / Confirm Last Frame")) {
                imageTarget = .init(assetID: nil, segmentID: segment.id, frameRole: role)
            }.disabled(editingLocked || segment.attempt != nil)
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
        .padding(10).frame(maxWidth: .infinity)
        .background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 11))
        .overlay(RoundedRectangle(cornerRadius: 11).stroke(Color.primary.opacity(0.06)))
    }

    @ViewBuilder private func videoActionPanel(_ segment: StorySegment) -> some View {
        if segment.video == nil, segment.attempt == nil, segment.detail != nil {
            let supportsLastFrame = mediaStudio.models.first(where: { $0.id == project.models.videoModelID })?
                .supportsVideoLastFrame == true
            let hasGeneratedFirst = !segment.firstFrames.images.isEmpty
            let hasUnconfirmedFirst = segment.firstFrame == nil && hasGeneratedFirst
            let hasUnconfirmedLast = segment.lastFrame == nil && !segment.lastFrames.images.isEmpty

            VStack(alignment: .leading, spacing: 10) {
                if hasUnconfirmedFirst {
                    Button {
                        viewModel.confirmLatestFrames(segment.id,
                                                      useConfirmedLastFrameForVideo: supportsLastFrame)
                    } label: {
                        Label(hasUnconfirmedLast
                              ? appModel.localized("确认最新首尾帧", english: "Confirm Latest First & Last Frames")
                              : appModel.localized("确认最新首帧", english: "Confirm Latest First Frame"),
                              systemImage: "checkmark.circle.fill")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent).tint(.green)
                    .disabled(editingLocked)
                    Text(appModel.localized("图片已经生成，但尚未选定使用版本。确认不会再次调用模型或产生费用。",
                                            english: "Images exist but no version is selected. Confirming does not call a model or incur cost."))
                        .font(.caption).foregroundStyle(.secondary)
                } else if segment.firstFrame != nil {
                    if hasUnconfirmedLast {
                        Button {
                            viewModel.confirmLatestFrames(segment.id,
                                                          useConfirmedLastFrameForVideo: supportsLastFrame)
                        } label: {
                            Label(appModel.localized("确认最新尾帧", english: "Confirm Latest Last Frame"),
                                  systemImage: "checkmark.circle")
                        }
                        .buttonStyle(.bordered)
                        .disabled(editingLocked)
                    }
                    Button {
                        viewModel.selectedSegments = [segment.id]
                        confirmsBatch = true
                    } label: {
                        Label(appModel.localized("生成本段视频…", english: "Generate This Segment Video…"),
                              systemImage: "play.rectangle.fill")
                            .font(.headline).frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent).tint(.blue).controlSize(.large)
                    .disabled(editingLocked)
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

    private var batchBar: some View {
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
            if viewModel.isBusy {
                ProgressView().controlSize(.small)
                Button(viewModel.pauseRequested ? appModel.localized("将在当前步骤结束后暂停", english: "Pausing after this step") : appModel.localized("暂停后续任务", english: "Pause Remaining Tasks")) { viewModel.requestPause() }
                    .disabled(viewModel.pauseRequested)
            } else {
                if activeVideos > 0 { ProgressView().controlSize(.small) }
                Text("\(selectedReady.count) " + appModel.localized("段已选", english: "selected") + " · \(selectedReadySeconds)s").font(.callout)
                Button(appModel.localized("选择全部就绪段", english: "Select Ready Segments")) {
                    viewModel.selectedSegments = Set(project.segments.filter(\.isReady).map(\.id))
                }.disabled(editingLocked)
                Button(appModel.localized("批量生成所选视频", english: "Generate Selected Videos")) { confirmsBatch = true }
                    .buttonStyle(.borderedProminent).tint(.blue).disabled(editingLocked || selectedReady.isEmpty)
            }
        }
        .padding(.horizontal, 24).padding(.vertical, 12)
        .background(.ultraThinMaterial)
        .overlay(alignment: .top) { Divider().opacity(0.65) }
    }

    private func sectionHeading(icon: String, color: Color, title: String, subtitle: String) -> some View {
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

    private func readinessRow(_ title: String, ready: Bool) -> some View {
        HStack(spacing: 9) {
            Image(systemName: ready ? "checkmark.circle.fill" : "circle")
                .foregroundStyle(ready ? Color.green : .secondary)
            Text(title).font(.callout)
            Spacer()
            Text(ready ? appModel.localized("完成", english: "Ready") : appModel.localized("待完成", english: "Pending"))
                .font(.caption2).foregroundStyle(.secondary)
        }
    }

    private func ratioChoice(_ option: String) -> some View {
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

    private func portraitMetric(_ value: Int, _ title: String, color: Color) -> some View {
        VStack(spacing: 2) {
            Text("\(value)").font(.headline.monospacedDigit()).foregroundStyle(color)
            Text(title).font(.caption2).foregroundStyle(.secondary)
        }.frame(minWidth: 42)
    }

    private func assetKindLabel(_ asset: StoryResource) -> String {
        switch asset.kind {
        case .character: appModel.localized("人物", english: "CHARACTER")
        case .scene: appModel.localized("场景", english: "SCENE")
        case .prop: appModel.localized("道具", english: "PROP")
        }
    }

    private func assetKindColor(_ asset: StoryResource) -> Color {
        switch asset.kind {
        case .character: .purple
        case .scene: .orange
        case .prop: .green
        }
    }

    private func hasConfirmedAssets(_ segment: StorySegment) -> Bool {
        segment.resourceIDs.allSatisfy { project.resource(id: $0)?.confirmedImage != nil }
    }
    private func frameGenerationTitle(_ collection: StoryImageCollection, role: StoryFrameRole) -> String {
        if !collection.images.isEmpty {
            return role == .first
                ? appModel.localized("重新生成首帧", english: "Regenerate First Frame")
                : appModel.localized("重新生成尾帧", english: "Regenerate Last Frame")
        }
        return role == .first
            ? appModel.localized("选择素材并生成首帧", english: "Choose Assets and Generate First Frame")
            : appModel.localized("选择素材并生成尾帧", english: "Choose Assets and Generate Last Frame")
    }
    private func status(_ segment: StorySegment) -> String {
        if segment.video != nil { return appModel.localized("视频已完成", english: "Video Complete") }
        if viewModel.isGeneratingVideo(segment.id, projectID: project.id) {
            return appModel.localized("正在生成视频", english: "Generating Video")
        }
        if segment.attempt != nil { return appModel.localized("已提交 · 查询原任务", english: "Submitted · Check Existing Task") }
        if segment.detail == nil { return appModel.localized("待细化镜头计划", english: "Needs Shot Plan") }
        if segment.firstFrame == nil {
            return segment.firstFrames.images.isEmpty
                ? appModel.localized("待生成首帧", english: "Needs First Frame")
                : appModel.localized("首帧已生成 · 待确认", english: "First Frame Generated · Needs Confirmation")
        }
        return appModel.localized("就绪 · 可以生成视频", english: "Ready to Generate")
    }
    private func time(_ seconds: Int) -> String { String(format: "%02d:%02d", seconds / 60, seconds % 60) }
    private func rememberDraft() { viewModel.rememberSourceDraft(projectID: project.id, source: source, style: style, ratio: ratio) }
    private func confirmPlanning(_ stage: StoryAgentRun.Stage, targets: [String]) {
        if viewModel.supportsAgentPlanning { planningConfirmation = .init(stage: stage, targets: targets, project: project) }
        else if stage == .outline { viewModel.planOutline() }
        else { viewModel.refineSegments(targets) }
    }
}

private struct StorySurfaceModifier: ViewModifier {
    let tint: Color?
    func body(content: Content) -> some View {
        content
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .strokeBorder(
                        LinearGradient(colors: [
                            (tint ?? .primary).opacity(tint == nil ? 0.09 : 0.18),
                            Color.primary.opacity(0.045),
                        ], startPoint: .topLeading, endPoint: .bottomTrailing)
                    )
            }
            .shadow(color: Color.black.opacity(0.035), radius: 12, y: 4)
    }
}

private extension View {
    func storySurface(tint: Color? = nil) -> some View {
        modifier(StorySurfaceModifier(tint: tint))
    }
}

struct StoryThumbnail: View {
    let asset: GeneratedMediaAsset?
    @State private var image: NSImage?
    var body: some View {
        ZStack {
            Color.primary.opacity(0.045)
            if let image { Image(nsImage: image).resizable().scaledToFit() }
            else { Image(systemName: "photo").foregroundStyle(.tertiary) }
        }.task(id: asset?.id) {
            image = nil
            guard let asset else { return }
            if let data = try? await MediaStudioImageLoader.data(for: asset), !Task.isCancelled { image = NSImage(data: data) }
        }
    }
}
