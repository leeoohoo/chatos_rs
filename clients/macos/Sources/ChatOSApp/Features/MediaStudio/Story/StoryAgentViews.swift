import ChatOSAgentRuntime
import ChatOSCore
import SwiftUI

struct StoryAdditionalIdeasField: View {
    @EnvironmentObject private var appModel: AppModel
    @Binding var text: String
    let help: String
    let maximumLength: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(appModel.localized("补充你的想法（选填）", english: "Add Your Ideas (Optional)"))
                .font(.headline)
            Text(help).font(.caption).foregroundStyle(.secondary)
            ZStack(alignment: .topLeading) {
                TextEditor(text: $text)
                    .font(.body)
                    .padding(5)
                    .scrollContentBackground(.hidden)
                if text.isEmpty {
                    Text(appModel.localized("写下希望特别呈现或保留的内容…", english: "Add anything you want emphasized or preserved…"))
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 8)
                        .allowsHitTesting(false)
                }
            }
            .frame(height: 100)
            .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.primary.opacity(0.12)))
            .onChange(of: text) { _, value in
                if value.count > maximumLength { text = String(value.prefix(maximumLength)) }
            }
        }
    }
}

struct StoryPlanningConfirmation: Identifiable {
    let id = UUID()
    let stage: StoryAgentRun.Stage
    let targets: [String]
    let project: StoryProject
}

struct StoryAgentStartView: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: StoryStudioViewModel
    let confirmation: StoryPlanningConfirmation
    @State private var isReady = false
    @State private var additionalIdeas = ""
    @State private var error: String?

    private var replacesExistingPlan: Bool {
        confirmation.stage == .refine && confirmation.targets.contains { id in
            confirmation.project.segments.contains { $0.id == id && $0.detail != nil }
        }
    }
    private var selectedSegments: [StorySegment] {
        confirmation.project.segments.filter { confirmation.targets.contains($0.id) }
    }
    private var storySummary: String {
        let values = [confirmation.project.summary, confirmation.project.description,
                      confirmation.project.source]
        return values.first { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty } ?? ""
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(confirmation.stage == .outline ? appModel.localized("分步分析完整剧情", english: "Plan the Story Step by Step")
                 : appModel.localized("逐段生成镜头提示词", english: "Generate Segment Shot Prompts")).font(.title2.bold())
            Text(confirmation.project.title).font(.headline)
            if confirmation.stage == .outline {
                VStack(alignment: .leading, spacing: 8) {
                    Text(appModel.localized("剧情概要", english: "Story Summary"))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Text(storySummary)
                        .font(.callout)
                        .lineLimit(6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .padding(12)
                .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 11, style: .continuous))
            } else {
                VStack(alignment: .leading, spacing: 9) {
                    Text(selectedSegments.count == 1
                         ? appModel.localized("这个片段的概要", english: "Segment Summary")
                         : appModel.localized("本次片段概要", english: "Selected Segment Summaries"))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    ScrollView {
                        LazyVStack(spacing: 8) {
                            ForEach(Array(selectedSegments.enumerated()), id: \.element.id) { index, segment in
                                HStack(alignment: .top, spacing: 11) {
                                    Text(String(format: "%02d", index + 1))
                                        .font(.caption2.bold().monospacedDigit())
                                        .foregroundStyle(.white)
                                        .frame(width: 27, height: 27)
                                        .background(segment.kind == .transition ? Color.purple : Color.blue, in: Circle())
                                    VStack(alignment: .leading, spacing: 5) {
                                        HStack(spacing: 7) {
                                            Text(segment.title).font(.callout.weight(.semibold))
                                            if segment.kind == .transition {
                                                Text(appModel.localized("转场", english: "Transition"))
                                                    .font(.caption2.weight(.semibold))
                                                    .foregroundStyle(.purple)
                                            }
                                            Spacer()
                                            Text("\(segment.seconds)s")
                                                .font(.caption.monospacedDigit())
                                                .foregroundStyle(.secondary)
                                        }
                                        Text(segment.synopsis)
                                            .font(.callout)
                                            .foregroundStyle(.secondary)
                                            .fixedSize(horizontal: false, vertical: true)
                                    }
                                }
                                .padding(11)
                                .background(Color.primary.opacity(0.035),
                                            in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                            }
                        }
                    }
                    .frame(maxHeight: selectedSegments.count == 1 ? 150 : 230)
                }
            }

            StoryAdditionalIdeasField(
                text: $additionalIdeas,
                help: confirmation.stage == .outline
                    ? appModel.localized("例如：重点突出某个人物、调整叙事节奏，或注明不能删减的情节。", english: "For example: emphasize a character, adjust the pacing, or note story beats that must be kept.")
                    : appModel.localized("例如：指定画面氛围、景别、运镜，或希望特别保留的细节。", english: "For example: specify the mood, framing, camera movement, or details that must be kept."),
                maximumLength: StoryAgentTools.maximumUserIdeasLength
            )
            if let error { Text(error).font(.caption).foregroundStyle(.orange) }
            HStack {
                Spacer()
                Button(appModel.localized("取消", english: "Cancel")) { dismiss() }
                Button(confirmation.stage == .outline
                       ? appModel.localized("开始整理剧情", english: "Start Planning Story")
                       : appModel.localized("开始生成镜头计划", english: "Create Shot Plan")) {
                    do {
                        guard let project = viewModel.project, project.id == confirmation.project.id,
                              try StoryAgentRun.digest(project) == StoryAgentRun.digest(confirmation.project) else { throw StoryAgentError.projectChanged }
                        if confirmation.stage == .outline { viewModel.planOutline(userIdeas: additionalIdeas) }
                        else if replacesExistingPlan { viewModel.regenerateSegmentPlans(confirmation.targets, userIdeas: additionalIdeas) }
                        else { viewModel.refineSegments(confirmation.targets, userIdeas: additionalIdeas) }
                        dismiss()
                    } catch { self.error = error.localizedDescription }
                }.buttonStyle(.borderedProminent).disabled(!isReady || !viewModel.canCreate)
            }
        }.padding(24).frame(width: 620)
        .onAppear {
            guard viewModel.supportsAgentPlanning else {
                isReady = true
                return
            }
            do {
                _ = try viewModel.effectiveAgentPolicy()
                isReady = true
            } catch {
                self.error = appModel.localized("暂时无法开始规划，请稍后再试。", english: "Planning can't start right now. Please try again later.")
            }
        }
    }
}

struct StoryAgentRunPanel: View {
    @EnvironmentObject private var appModel: AppModel
    @ObservedObject var viewModel: StoryStudioViewModel
    @State private var showsHistory = false
    @State private var resumeID: UUID?

    var body: some View {
        if let run = viewModel.latestAgentRun {
            VStack(alignment: .leading, spacing: 9) {
                HStack {
                    Label(appModel.localized("AI 规划运行", english: "AI Planning Run"), systemImage: "arrow.triangle.2.circlepath").font(.headline)
                    Spacer()
                }
                Text(label(run)).font(.caption).foregroundStyle(run.applied ? Color.green : .secondary)
                if let event = run.events.last { Text(event.detail).font(.caption).foregroundStyle(.secondary).lineLimit(2) }
                if viewModel.isBusy, viewModel.activeProjectID == run.projectID,
                   !viewModel.streamingModelText.isEmpty {
                    VStack(alignment: .leading, spacing: 6) {
                        Label(appModel.localized("模型实时输出", english: "Live Model Output"), systemImage: "waveform")
                            .font(.caption.weight(.semibold)).foregroundStyle(.purple)
                        ScrollView {
                            Text(viewModel.streamingModelText)
                                .font(.caption.monospaced()).textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }.frame(maxHeight: 110)
                    }.padding(10).background(Color.purple.opacity(0.06), in: RoundedRectangle(cornerRadius: 9))
                }
                Text(appModel.localized("草稿：\(run.draft.resources.count) 个素材 · \(run.draft.segments.count) 段 · \(run.draft.totalSeconds) 秒", english: "Draft: \(run.draft.resources.count) assets · \(run.draft.segments.count) segments · \(run.draft.totalSeconds) seconds"))
                    .font(.caption)
                if let reason = run.checkpoint.stopReason { Text(reason).font(.caption).foregroundStyle(.orange).lineLimit(3) }
                HStack {
                    Button(appModel.localized("草稿 / 运行记录", english: "Draft / Run History")) { showsHistory = true }
                    Spacer()
                    if !run.applied && run.abandonedAt == nil {
                        Button(run.checkpoint.status == .completed ? appModel.localized("应用草稿", english: "Apply Draft") : appModel.localized("恢复规划", english: "Resume Planning")) { resumeID = run.id }
                            .disabled(viewModel.isBusy || viewModel.isLoadingAgentRuns)
                    }
                }
            }.padding(12).background(Color.purple.opacity(0.05), in: RoundedRectangle(cornerRadius: 10))
            .sheet(isPresented: $showsHistory) { history }
            .confirmationDialog(appModel.localized("继续这次规划？", english: "Continue This Planning Run?"), isPresented: Binding(get: { resumeID != nil }, set: { if !$0 { resumeID = nil } }), titleVisibility: .visible) {
                Button(appModel.localized("确认继续", english: "Confirm and Continue")) {
                    if let resumeID { viewModel.resumeAgent(resumeID) }
                    resumeID = nil
                }
            } message: {
                Text(appModel.localized("将从上次保存的进度继续，已经完成的内容不会重复生成。规划完成后才会应用到项目。", english: "Continues from the last saved progress without regenerating completed work. The result is applied to the project only after planning finishes."))
            }
        } else if viewModel.isLoadingAgentRuns {
            ProgressView().controlSize(.small)
        }
    }

    private var history: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text(appModel.localized("规划草稿与运行记录", english: "Planning Drafts & Run History")).font(.title2.bold())
                Spacer()
                Button(appModel.localized("关闭", english: "Close")) { showsHistory = false }
            }
            Text(appModel.localized("草稿逐步保存；只有完整校验通过后才应用，不会覆盖已修改的项目。", english: "Drafts are saved incrementally and applied only after validation, without overwriting a changed project."))
                .font(.caption).foregroundStyle(.secondary)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    ForEach(viewModel.projectAgentRuns) { run in
                        GroupBox {
                            VStack(alignment: .leading, spacing: 10) {
                                Text(run.updatedAt, style: .date).font(.caption)
                                Text(label(run)).font(.headline)
                                Text(appModel.localized("规划时长：\(run.draft.totalSeconds) 秒", english: "Planned duration: \(run.draft.totalSeconds) seconds"))
                                    .monospacedDigit()
                                if !run.applied && run.abandonedAt == nil && !viewModel.isBusy {
                                    Button(appModel.localized("恢复 / 应用这一条记录", english: "Resume / Apply This Run")) { showsHistory = false; resumeID = run.id }
                                }
                                DisclosureGroup(appModel.localized("人物文字画像", english: "Written Character Profiles")) {
                                    ForEach(run.draft.characters) { character in
                                        VStack(alignment: .leading, spacing: 5) {
                                            Text(character.name).font(.headline)
                                            StoryCharacterProfileView(profile: character.profile)
                                        }.padding(.vertical, 6)
                                    }
                                }
                                DisclosureGroup(appModel.localized("场景文字画像", english: "Written Scene Profiles")) {
                                    ForEach(run.draft.scenes) { scene in
                                        VStack(alignment: .leading, spacing: 5) {
                                            Text(scene.name).font(.headline)
                                            StorySceneProfileView(profile: scene.profile)
                                        }.padding(.vertical, 6)
                                    }
                                }
                                DisclosureGroup(appModel.localized("分段草稿", english: "Segment Drafts")) {
                                    ForEach(run.draft.segments) { segment in
                                        VStack(alignment: .leading, spacing: 4) {
                                            Text(segment.title + " · " + (segment.kind == .transition
                                                 ? appModel.localized("转场", english: "Transition")
                                                 : appModel.localized("剧情", english: "Story"))
                                                 + " · \(segment.seconds)s").fontWeight(.medium)
                                            Text(segment.synopsis)
                                            if let detail = segment.detail { Text(detail.videoPrompt) }
                                        }.font(.caption).textSelection(.enabled).padding(.vertical, 6)
                                    }
                                }
                                DisclosureGroup(appModel.localized("最近 100 条动作（完整记录保存在本机）", english: "Last 100 Actions (Full Log Saved Locally)")) {
                                    ForEach(Array(run.events.suffix(100))) { event in
                                        HStack(alignment: .top) {
                                            Text(event.date, style: .time).foregroundStyle(.secondary)
                                            Text(event.detail).textSelection(.enabled)
                                        }.font(.caption).padding(.vertical, 3)
                                    }
                                }
                            }.frame(maxWidth: .infinity, alignment: .leading).padding(8)
                        }
                    }
                }
            }
        }.padding(24).frame(width: 760, height: 650)
    }
    private func label(_ run: StoryAgentRun) -> String {
        if run.applied { return appModel.localized("规划完成 · 已应用", english: "Planning Complete · Applied") }
        if run.abandonedAt != nil { return appModel.localized("中断草稿 · 已放弃", english: "Interrupted Draft · Discarded") }
        if run.checkpoint.status == .completed { return appModel.localized("草稿已完成 · 待应用", english: "Draft Complete · Awaiting Application") }
        if viewModel.isBusy && viewModel.activeProjectID == run.projectID && viewModel.latestAgentRun?.id == run.id {
            return appModel.localized("正在分步规划 · 草稿自动保存", english: "Planning Step by Step · Draft Autosaved")
        }
        return appModel.localized("已暂停 / 中断 · 可查看并恢复", english: "Paused / Interrupted · Review or Resume")
    }
}

struct StoryCharacterProfileView: View {
    @EnvironmentObject private var appModel: AppModel
    let profile: StoryCharacterProfile
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(profile.isProtagonist ? appModel.localized("主角 · 文字画像", english: "Protagonist · Written Profile") : appModel.localized("人物 · 文字画像", english: "Character · Written Profile"))
                .font(.caption.bold()).foregroundStyle(.purple)
            field("剧情身份", "Story Role", profile.roleInStory)
            field("外貌", "Appearance", profile.appearance)
            field("性格", "Personality", profile.personality)
            field("动机", "Motivation", profile.motivation)
            field("人物关系", "Relationships", profile.relationships)
            field("服装", "Costume", profile.costume)
            field("一致性约束", "Consistency", profile.consistencyNotes)
        }
    }
    private func field(_ chinese: String, _ english: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(appModel.localized(chinese, english: english)).font(.caption.bold())
            Text(value).font(.caption).textSelection(.enabled)
        }
    }
}

struct StorySceneProfileView: View {
    @EnvironmentObject private var appModel: AppModel
    let profile: StorySceneProfile
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(appModel.localized("场景 · 文字画像", english: "Scene · Written Profile")).font(.caption.bold()).foregroundStyle(.purple)
            field("剧情作用", "Story Role", profile.roleInStory)
            field("时代、地点与环境", "Period, Location & Environment", profile.setting)
            field("空间布局", "Spatial Layout", profile.spatialLayout)
            field("光线与色调", "Lighting & Palette", profile.lightingAndPalette)
            field("关键陈设", "Key Elements", profile.keyElements)
            field("氛围", "Atmosphere", profile.atmosphere)
            field("一致性约束", "Consistency", profile.consistencyNotes)
        }
    }
    private func field(_ chinese: String, _ english: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(appModel.localized(chinese, english: english)).font(.caption.bold())
            Text(value).font(.caption).textSelection(.enabled)
        }
    }
}
