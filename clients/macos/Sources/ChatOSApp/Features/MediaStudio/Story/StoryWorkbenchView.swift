import AppKit
import AVKit
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
    @State private var recoveryJobID = ""
    @State private var player: AVPlayer?
    @State private var showsVideo = false
    @State private var planningConfirmation: StoryPlanningConfirmation?
    @State private var showsMediaBatch = false
    @State private var mediaBatchKind: StoryMediaBatch.Kind = .pipeline
    @State private var section: Section = .source

    private var selected: StorySegment? { project.segments.first { $0.id == viewModel.selectedSegmentID } }
    private var selectedReady: [StorySegment] { project.segments.filter { viewModel.selectedSegments.contains($0.id) && $0.isReady } }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            sectionBar
            Divider()
            workspaceContent.padding(18)
            Divider()
            batchBar
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
        .sheet(isPresented: $showsVideo, onDismiss: { player?.pause(); player = nil }) {
            VStack { VideoPlayer(player: player).frame(minWidth: 720, minHeight: 420)
                Button(appModel.localized("关闭", english: "Close")) { showsVideo = false }.padding()
            }
        }
        .confirmationDialog(appModel.localized("确认批量生成视频？", english: "Generate the selected videos?"), isPresented: $confirmsBatch, titleVisibility: .visible) {
            Button(appModel.localized("确认生成", english: "Confirm Generation")) { viewModel.generateBatch(availableModels: mediaStudio.models) }
        } message: {
            Text("\(selectedReady.count) " + appModel.localized("段，共", english: "segments, totaling") + " \(selectedReady.count * 15)s。"
                 + appModel.localized("将调用所选视频模型并可能产生费用。失败时暂停，不自动重试。", english: "Uses your video model and may incur charges. Pauses on failure without automatic retries."))
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
    }

    private var header: some View {
        HStack(spacing: 14) {
            Button { viewModel.backToList() } label: {
                Label(appModel.localized("剧情列表", english: "Stories"), systemImage: "chevron.left")
            }.buttonStyle(.plain)
            Divider().frame(height: 20)
            Text(project.title).font(.headline).lineLimit(1)
            Text(source != project.source || style != project.style || ratio != project.ratio
                 ? appModel.localized("原文有未保存修改", english: "Unsaved story changes")
                 : appModel.localized("已保存到本机", english: "Saved locally")).font(.caption).foregroundStyle(.secondary)
            Spacer()
            Button { showSettings() } label: {
                Label(appModel.localized("剧情设置", english: "Story Settings"), systemImage: "slider.horizontal.3")
            }.disabled(viewModel.isBusy)
        }.padding(.horizontal, 20).frame(height: 54)
    }

    private var sectionBar: some View {
        HStack(spacing: 8) {
            sectionButton(.source, "剧情原文", "Story", "doc.text", nil)
            sectionButton(.style, "画面风格", "Visual Style", "paintpalette", nil)
            sectionButton(.portraits, "角色画像", "Portraits", "person.crop.rectangle.stack", project.resources.count)
            sectionButton(.segments, "拍摄分段", "Shot Segments", "rectangle.stack", project.segments.count)
            sectionButton(.graph, "关系图谱", "Relationship Graph", "point.3.connected.trianglepath.dotted", project.relations.count)
            Spacer(minLength: 12)
            Button { mediaBatchKind = .pipeline; showsMediaBatch = true } label: {
                Label(appModel.localized("一键全部生成", english: "Generate Everything"), systemImage: "sparkles.rectangle.stack")
            }
            .buttonStyle(.borderedProminent).tint(.purple)
            .disabled(viewModel.isBusy || project.segments.isEmpty)
        }.padding(.horizontal, 18).padding(.vertical, 10).background(.bar)
    }

    private func sectionButton(_ item: Section, _ chinese: String, _ english: String,
                               _ icon: String, _ count: Int?) -> some View {
        Button {
            section = item
        } label: {
            HStack(spacing: 7) {
                Image(systemName: icon)
                Text(appModel.localized(chinese, english: english)).fontWeight(section == item ? .semibold : .regular)
                if let count { Text("\(count)").font(.caption.monospacedDigit()).foregroundStyle(.secondary) }
            }
            .padding(.horizontal, 13).padding(.vertical, 8)
            .background(section == item ? Color.purple.opacity(0.13) : Color.clear, in: RoundedRectangle(cornerRadius: 9))
        }.buttonStyle(.plain)
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
                HStack(alignment: .firstTextBaseline) {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(appModel.localized("剧情原文", english: "Complete Story")).font(.title2.bold())
                        Text(appModel.localized("集中打磨完整剧情、人物动机和叙事节奏；画面风格在单独页面设置。", english: "Refine the narrative, character motivations and pacing here. Visual direction has its own page."))
                            .font(.callout).foregroundStyle(.secondary)
                    }
                    Spacer()
                    if viewModel.isBusy { ProgressView().controlSize(.small); Text(viewModel.operation).font(.caption) }
                }
                TextEditor(text: $source).font(.body).frame(minHeight: 480)
                    .padding(12).background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 10))
                    .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color.primary.opacity(0.08)))
                    .disabled(!project.segments.isEmpty || viewModel.isBusy)
                HStack {
                    Text("\(source.count) / 80000").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                    Spacer()
                    Button { viewModel.optimizeSourceDraft(source, style: style, target: .source) } label: {
                        Label(appModel.localized("AI 优化剧情", english: "Improve Story with AI"), systemImage: "wand.and.stars")
                    }.disabled(viewModel.isBusy || !project.segments.isEmpty || source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    Button(appModel.localized("保存剧情原文", english: "Save Story Text")) {
                        viewModel.saveSource(source, style: style, ratio: ratio)
                    }.buttonStyle(.borderedProminent).tint(.purple)
                        .disabled(viewModel.isBusy || !project.segments.isEmpty || source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
                if viewModel.optimizationTarget == .source { optimizationSuggestion }
                if let error = viewModel.errorMessage { Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
                Text(appModel.localized("AI 优化只生成候选文本，必须由你点击采用；不会直接覆盖原文，也不会生成图片或视频。", english: "AI improvement creates a suggestion only. You must apply it explicitly; it never overwrites the source or generates images or video."))
                    .font(.caption).foregroundStyle(.secondary)
            }.padding(16)
        }.background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
    }

    private var stylePanel: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack(alignment: .firstTextBaseline) {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(appModel.localized("画面风格", english: "Visual Style")).font(.title2.bold())
                        Text(appModel.localized("单独定义全剧的美术方向、光线、色彩、镜头质感和画幅。", english: "Define the film-wide art direction, lighting, color, camera texture and aspect ratio."))
                            .font(.callout).foregroundStyle(.secondary)
                    }
                    Spacer()
                    if viewModel.isBusy { ProgressView().controlSize(.small); Text(viewModel.operation).font(.caption) }
                }
                TextEditor(text: $style).font(.body).frame(minHeight: 360)
                    .padding(12).background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 10))
                    .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color.primary.opacity(0.08)))
                    .disabled(!project.segments.isEmpty || viewModel.isBusy)
                HStack {
                    Text("\(style.count) / 2000").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                    Spacer()
                    Button { viewModel.optimizeSourceDraft(source, style: style, target: .style) } label: {
                        Label(appModel.localized("AI 优化风格", english: "Improve Style with AI"), systemImage: "wand.and.stars")
                    }.disabled(viewModel.isBusy || !project.segments.isEmpty || style.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
                GroupBox(appModel.localized("画幅与全剧规划", english: "Format and Story Planning")) {
                    HStack(alignment: .center, spacing: 18) {
                        Picker(appModel.localized("比例", english: "Ratio"), selection: $ratio) {
                            ForEach(["16:9", "9:16", "1:1"], id: \.self) { Text($0).tag($0) }
                        }.frame(width: 220)
                        Spacer()
                        if project.segments.isEmpty {
                            Button(appModel.localized("保存画面风格", english: "Save Visual Style")) {
                                viewModel.saveSource(source, style: style, ratio: ratio)
                            }.disabled(viewModel.isBusy || source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                            Button(appModel.localized("分析全剧并拆分计划", english: "Analyze and Plan Entire Story")) {
                                confirmPlanning(.outline, targets: [])
                            }.buttonStyle(.borderedProminent).tint(.purple)
                                .disabled(viewModel.isBusy || project.source.isEmpty || source != project.source || style != project.style || ratio != project.ratio)
                        } else {
                            Text("\(project.segments.count) × 15s = \(project.totalSeconds)s")
                                .font(.headline.monospacedDigit()).foregroundStyle(.purple)
                        }
                    }.padding(8)
                }
                if viewModel.optimizationTarget == .style { optimizationSuggestion }
                HStack(alignment: .top, spacing: 18) {
                    StoryAgentRunPanel(viewModel: viewModel).frame(maxWidth: .infinity, alignment: .topLeading)
                    StoryMediaBatchPanel(viewModel: viewModel).frame(maxWidth: .infinity, alignment: .topLeading)
                }
                if !project.segments.isEmpty {
                    Text(appModel.localized("规划完成后剧情和风格会锁定，避免已生成画像与分段失去一致性依据。", english: "Story and style are locked after planning so generated portraits and segments retain a stable consistency basis."))
                        .font(.caption).foregroundStyle(.secondary)
                }
                if let error = viewModel.errorMessage { Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
            }.padding(16)
        }.background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
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
                HStack(alignment: .firstTextBaseline) {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(appModel.localized("角色与场景画像", english: "Character and Scene Portraits")).font(.title2.bold())
                        Text(appModel.localized("文字画像定义一致性，确认的图片版本才会作为分段生成素材。", english: "Written portraits define consistency; only confirmed image versions become segment references."))
                            .font(.callout).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button {
                        mediaBatchKind = .assets; showsMediaBatch = true
                    } label: {
                        Label(appModel.localized("一键生成全部素材图", english: "Generate All Asset Images"), systemImage: "photo.stack")
                    }.buttonStyle(.borderedProminent).tint(.purple)
                        .disabled(viewModel.isBusy || project.resources.isEmpty)
                }
                if project.resources.isEmpty {
                    ContentUnavailableView {
                        Label(appModel.localized("尚未生成人物与场景画像", english: "No Portraits Yet"), systemImage: "person.crop.rectangle.stack")
                    } description: {
                        Text(appModel.localized("先在“剧情原文”完成并启动全剧规划。", english: "Complete and plan the story in the Story tab first."))
                    } actions: {
                        Button(appModel.localized("前往剧情原文", english: "Open Story")) { section = .source }
                    }.frame(minHeight: 360)
                } else {
                    portraitSection(appModel.localized("人物画像", english: "Characters"), resources: project.resources.filter { $0.kind == .character })
                    portraitSection(appModel.localized("场景画像", english: "Scenes"), resources: project.resources.filter { $0.kind == .scene })
                    portraitSection(appModel.localized("关键道具", english: "Props"), resources: project.resources.filter { $0.kind == .prop })
                }
            }.padding(16)
        }.background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
    }

    @ViewBuilder private func portraitSection(_ title: String, resources: [StoryResource]) -> some View {
        if !resources.isEmpty {
            VStack(alignment: .leading, spacing: 12) {
                Text(title).font(.title3.bold())
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 330, maximum: 480), spacing: 14)], spacing: 14) {
                    ForEach(resources) { asset in portraitCard(asset) }
                }
            }
        }
    }

    private func portraitCard(_ asset: StoryResource) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 14) {
                let displayed = asset.confirmedImage ?? asset.images.last
                Button {
                    if let displayed, let media = viewModel.mediaAsset(displayed, projectID: project.id) {
                        preview = .init(images: [media])
                    }
                } label: {
                    StoryThumbnail(asset: displayed.flatMap { viewModel.mediaAsset($0, projectID: project.id) })
                        .frame(width: 132, height: 132).clipped().clipShape(RoundedRectangle(cornerRadius: 10))
                }.buttonStyle(.plain).disabled(displayed == nil)
                VStack(alignment: .leading, spacing: 7) {
                    Text(asset.name).font(.headline)
                    Label(asset.confirmedImage != nil ? appModel.localized("图片已确认", english: "Image Confirmed")
                          : asset.images.isEmpty ? appModel.localized("等待生成图片", english: "Needs Image")
                          : appModel.localized("请选择图片版本", english: "Choose an Image Version"),
                          systemImage: asset.confirmedImage != nil ? "checkmark.circle.fill" : "circle.dashed")
                        .font(.caption).foregroundStyle(asset.confirmedImage != nil ? Color.green : .secondary)
                    Text(portraitSummary(asset)).font(.caption).foregroundStyle(.secondary).lineLimit(5)
                }
            }
            Text(asset.prompt).font(.callout).lineLimit(4).textSelection(.enabled)
                .padding(10).frame(maxWidth: .infinity, alignment: .leading)
                .background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 8))
            Button {
                imageTarget = .init(assetID: asset.id, segmentID: nil)
            } label: {
                Label(asset.confirmedImage == nil ? appModel.localized("生成 / 上传 / 选择图片", english: "Generate / Upload / Choose Image")
                      : appModel.localized("查看与更换图片版本", english: "Review or Change Image"), systemImage: "photo.badge.plus")
            }.buttonStyle(.borderedProminent).tint(.purple).disabled(viewModel.isBusy)
        }.padding(16).background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.primary.opacity(0.08)))
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
        HStack(alignment: .top, spacing: 18) {
            overview.frame(width: 410)
            detailPanel.frame(maxWidth: .infinity)
        }
    }

    private var overview: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                VStack(alignment: .leading, spacing: 6) {
                    Text(appModel.localized("全剧分段总览", english: "Full Story Plan")).font(.title3.bold())
                    Text("\(project.segments.count) × 15s = \(project.totalSeconds)s")
                        .font(.callout.monospacedDigit()).foregroundStyle(.purple)
                }
                Spacer()
                Menu(appModel.localized("计划操作", english: "Plan Actions")) {
                    Button(appModel.localized("批量制作 / 一键执行", english: "Batch Production / Full Pipeline")) {
                        mediaBatchKind = .pipeline; showsMediaBatch = true
                    }
                    Button(appModel.localized("逐段细化未完成计划", english: "Refine Unfinished Plans")) {
                        confirmPlanning(.refine, targets: project.segments.filter { $0.detail == nil && $0.attempt == nil && $0.video == nil }.map(\.id))
                    }.disabled(project.segments.isEmpty)
                    Button(appModel.localized("添加 15 秒分段", english: "Add 15-second Segment")) { viewModel.addSegment() }
                        .disabled(project.source.isEmpty || project.hasUnresolvedJobs || project.completedCount > 0)
                }.disabled(viewModel.isBusy)
            }
            if project.segments.isEmpty {
                ContentUnavailableView {
                    Label(appModel.localized("先规划整个故事", english: "Plan the Entire Story"), systemImage: "list.bullet.rectangle")
                } description: {
                    Text(appModel.localized("请先到“剧情原文”保存完整故事并规划，之后这里会展示所有连续的 15 秒分段。", english: "Save and plan the complete story in the Story tab; every consecutive 15-second segment will then appear here."))
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
        }.padding(16).background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
    }

    private func segmentRow(_ segment: StorySegment, index: Int) -> some View {
        HStack(spacing: 10) {
            Toggle("", isOn: Binding(get: { viewModel.selectedSegments.contains(segment.id) }, set: { on in
                if on { viewModel.selectedSegments.insert(segment.id) } else { viewModel.selectedSegments.remove(segment.id) }
            })).labelsHidden().toggleStyle(.checkbox).disabled(!segment.isReady || viewModel.isBusy)
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text(String(format: "%02d", index + 1)).foregroundStyle(.purple).monospacedDigit()
                    Text(segment.title).fontWeight(.semibold).lineLimit(1)
                    Spacer(minLength: 4)
                    Text("\(time(index * 15))–\(time((index + 1) * 15))").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                }
                Text(segment.synopsis).font(.caption).lineLimit(2).foregroundStyle(.secondary)
                HStack {
                    Text(status(segment)).font(.caption2).foregroundStyle(segment.video == nil ? Color.purple : .green)
                    Spacer()
                    Text("15s").font(.caption2).foregroundStyle(.secondary)
                }
            }.contentShape(Rectangle()).onTapGesture { viewModel.selectedSegmentID = segment.id }
        }.padding(12)
        .background(viewModel.selectedSegmentID == segment.id ? Color.purple.opacity(0.08) : Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 9))
        .overlay(RoundedRectangle(cornerRadius: 9).stroke(viewModel.selectedSegmentID == segment.id ? Color.purple.opacity(0.6) : .clear))
        .contextMenu {
            Button(appModel.localized("编辑分段", english: "Edit Segment")) { editSegment = segment }.disabled(viewModel.isBusy || segment.attempt != nil)
            Button(appModel.localized("上移（重新细化衔接）", english: "Move Up and Re-plan")) { viewModel.moveSegment(segment.id, offset: -1) }
                .disabled(viewModel.isBusy || index == 0 || project.hasUnresolvedJobs || project.completedCount > 0)
            Button(appModel.localized("下移（重新细化衔接）", english: "Move Down and Re-plan")) { viewModel.moveSegment(segment.id, offset: 1) }
                .disabled(viewModel.isBusy || index + 1 == project.segments.count || project.hasUnresolvedJobs || project.completedCount > 0)
            Button(appModel.localized("删除分段", english: "Remove Segment"), role: .destructive) { segmentToDelete = segment.id }
                .disabled(viewModel.isBusy || project.hasUnresolvedJobs || project.completedCount > 0)
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
                            .disabled(viewModel.isBusy || segment.attempt != nil)
                    }
                    Text(status(segment)).font(.caption).foregroundStyle(.secondary)
                    if let video = segment.video, let url = viewModel.videoURL(video, projectID: project.id) {
                        Button { player = AVPlayer(url: url); showsVideo = true } label: {
                            Label(appModel.localized("播放生成视频", english: "Play Generated Video"), systemImage: "play.rectangle.fill")
                        }.buttonStyle(.borderedProminent).tint(.purple)
                    } else if let attempt = segment.attempt {
                        if viewModel.isBusy && viewModel.activeSegmentID == segment.id {
                            if let percent = viewModel.progress?.percent {
                                ProgressView(value: min(100, max(0, percent)), total: 100)
                                Text("\(Int(min(100, max(0, percent))))%").font(.caption.monospacedDigit())
                            } else { ProgressView().controlSize(.small) }
                        }
                        Text(attempt.jobID ?? appModel.localized("提交结果待核实", english: "Submission needs verification"))
                            .font(.caption).textSelection(.enabled)
                        Button(appModel.localized("查询原任务 / 恢复下载", english: "Check Task / Resume Download")) { viewModel.resumeVideo(segment.id) }
                            .disabled(viewModel.isBusy || attempt.jobID == nil)
                        if attempt.jobID == nil {
                            Text(appModel.localized("请核对服务商任务记录；未确认前不重复提交，避免再次扣费。", english: "Check your provider's task history. Resubmission is blocked to avoid duplicate charges."))
                                .font(.caption).foregroundStyle(.orange)
                            TextField(appModel.localized("填入已核实的原任务 ID", english: "Verified original task ID"), text: $recoveryJobID)
                            Button(appModel.localized("绑定原任务 ID", english: "Attach Original Task ID")) {
                                viewModel.attachVerifiedJobID(recoveryJobID, segmentID: segment.id); recoveryJobID = ""
                            }.disabled(viewModel.isBusy || recoveryJobID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                        }
                        Button(appModel.localized("核对后允许重试…", english: "Allow Retry after Verification…")) { segmentToRetry = segment.id }
                            .disabled(viewModel.isBusy)
                    }
                    if let error = segment.error { Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
                    let frame = segment.firstFrame ?? segment.firstFrames.images.last
                    Button {
                        if let frame, let asset = viewModel.mediaAsset(frame, projectID: project.id) { preview = .init(images: [asset]) }
                    } label: {
                        StoryThumbnail(asset: frame.flatMap { viewModel.mediaAsset($0, projectID: project.id) })
                            .frame(height: 148).frame(maxWidth: .infinity).clipped().clipShape(RoundedRectangle(cornerRadius: 8))
                    }.buttonStyle(.plain).disabled(frame == nil)
                    Button(appModel.localized("选择 / 上传 / 确认首帧", english: "Choose / Upload / Confirm Frame")) {
                        imageTarget = .init(assetID: nil, segmentID: segment.id)
                    }.disabled(viewModel.isBusy || segment.attempt != nil)
                    Button(appModel.localized("用引用素材生成首帧", english: "Generate Frame from Assets")) {
                        viewModel.generateFirstFrame(segment.id)
                    }.disabled(viewModel.isBusy || segment.detail == nil || segment.attempt != nil || !hasConfirmedAssets(segment))
                    if !hasConfirmedAssets(segment) {
                        Text(appModel.localized("先确认本段引用的角色与场景素材。", english: "Confirm this segment's referenced assets first."))
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
                    } else {
                        Text(appModel.localized("此段只有剧情概要，尚未展开镜头计划。", english: "This segment has an outline but no detailed shot plan yet."))
                            .font(.callout).foregroundStyle(.secondary)
                        Button(appModel.localized("细化这个 15 秒分段", english: "Refine This 15-second Segment")) { confirmPlanning(.refine, targets: [segment.id]) }
                            .disabled(viewModel.isBusy)
                    }
                }.padding(16)
            }.background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        } else {
            ContentUnavailableView(appModel.localized("分段详情", english: "Segment Details"), systemImage: "sidebar.right",
                                   description: Text(appModel.localized("选择一段，编辑镜头、素材和首帧。", english: "Select a segment to edit its shots, assets and first frame.")))
                .frame(maxHeight: .infinity).background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        }
    }

    private var batchBar: some View {
        HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 5) {
                Text("\(project.completedCount) / \(project.segments.count) " + appModel.localized("段已完成", english: "segments complete")).font(.callout.bold())
                Text(viewModel.isBusy ? viewModel.operation : appModel.localized("先确认素材与首帧，再批量生成视频", english: "Confirm assets and first frames before generating videos"))
                    .font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if viewModel.isBusy {
                ProgressView().controlSize(.small)
                Button(viewModel.pauseRequested ? appModel.localized("将在当前步骤结束后暂停", english: "Pausing after this step") : appModel.localized("暂停后续任务", english: "Pause Remaining Tasks")) { viewModel.requestPause() }
                    .disabled(viewModel.pauseRequested)
            } else {
                Text("\(selectedReady.count) " + appModel.localized("段已选", english: "selected") + " · \(selectedReady.count * 15)s").font(.callout)
                Button(appModel.localized("选择全部就绪段", english: "Select Ready Segments")) {
                    viewModel.selectedSegments = Set(project.segments.filter(\.isReady).map(\.id))
                }
                Button(appModel.localized("批量生成所选视频", english: "Generate Selected Videos")) { confirmsBatch = true }
                    .buttonStyle(.borderedProminent).tint(.purple).disabled(selectedReady.isEmpty)
            }
        }.padding(.horizontal, 20).padding(.vertical, 14).background(.bar)
    }
    private func hasConfirmedAssets(_ segment: StorySegment) -> Bool {
        segment.resourceIDs.allSatisfy { project.resource(id: $0)?.confirmedImage != nil }
    }
    private func status(_ segment: StorySegment) -> String {
        if segment.video != nil { return appModel.localized("视频已完成", english: "Video Complete") }
        if segment.attempt != nil { return appModel.localized("已提交 · 查询原任务", english: "Submitted · Check Existing Task") }
        if segment.detail == nil { return appModel.localized("待细化镜头计划", english: "Needs Shot Plan") }
        if segment.firstFrame == nil { return appModel.localized("待制作 / 确认首帧", english: "Needs Confirmed First Frame") }
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
