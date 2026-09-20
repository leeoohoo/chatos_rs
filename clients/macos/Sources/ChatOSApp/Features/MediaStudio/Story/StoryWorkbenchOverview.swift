import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

extension StoryWorkbenchView {
    var header: some View {
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

    var sectionBar: some View {
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

    var agentDraftBanner: some View {
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

    func sectionButton(_ item: Section, index: Int, _ chinese: String, _ english: String,
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

    var hasUnsavedChanges: Bool {
        source != project.source || style != project.style || ratio != project.ratio
    }

    var projectPhase: String {
        if project.segments.isEmpty { return appModel.localized("准备中", english: "Setup") }
        if project.completedCount == project.segments.count { return appModel.localized("已完成", english: "Complete") }
        if project.hasUnresolvedJobs { return appModel.localized("任务处理中", english: "Processing") }
        return appModel.localized("制作中", english: "In Production")
    }

    var projectPhaseColor: Color {
        if project.segments.isEmpty { return .orange }
        if project.completedCount == project.segments.count { return .green }
        return .indigo
    }

    func headerMetric(value: String, label: String, icon: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: icon).foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 1) {
                Text(value).font(.callout.bold().monospacedDigit())
                Text(label).font(.caption2).foregroundStyle(.secondary)
            }
        }.padding(.horizontal, 10)
    }

    func sectionColor(_ item: Section) -> Color {
        switch item {
        case .source: .indigo
        case .style: .pink
        case .portraits: .orange
        case .segments: .blue
        case .graph: .teal
        }
    }

    func sectionHint(_ item: Section) -> String {
        switch item {
        case .source: appModel.localized("故事与叙事", english: "Narrative")
        case .style: appModel.localized("视觉基调", english: "Direction")
        case .portraits: appModel.localized("人物与场景", english: "Assets")
        case .segments: appModel.localized("分段与转场", english: "Shots & transitions")
        case .graph: appModel.localized("引用与关系", english: "Connections")
        }
    }

    @ViewBuilder var workspaceContent: some View {
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

    var sourcePanel: some View {
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

    var sourceEditor: some View {
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

    var sourceSidebar: some View {
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

    var stylePanel: some View {
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

    var styleEditor: some View {
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

    var planningCard: some View {
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

    @ViewBuilder var optimizationSuggestion: some View {
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

    var portraitsPanel: some View {
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

    @ViewBuilder func portraitSection(_ title: String, resources: [StoryResource]) -> some View {
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

    func portraitCard(_ asset: StoryResource) -> some View {
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

    func portraitSummary(_ asset: StoryResource) -> String {
        if let profile = asset.characterProfile {
            return [profile.isProtagonist ? appModel.localized("主角", english: "Protagonist") : appModel.localized("人物", english: "Character"),
                    profile.roleInStory, profile.personality, profile.motivation].joined(separator: " · ")
        }
        if let profile = asset.sceneProfile {
            return [profile.roleInStory, profile.setting, profile.atmosphere].joined(separator: " · ")
        }
        return asset.prompt
    }

}
