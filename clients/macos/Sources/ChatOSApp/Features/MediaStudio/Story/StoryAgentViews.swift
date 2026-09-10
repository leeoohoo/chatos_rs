import ChatOSAgentRuntime
import ChatOSCore
import SwiftUI

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
    @State private var policy: AgentRunPolicy?
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(confirmation.stage == .outline ? appModel.localized("分步分析完整剧情", english: "Plan the Story Step by Step")
                 : appModel.localized("逐段生成镜头提示词", english: "Generate Segment Shot Prompts")).font(.title2.bold())
            Text(confirmation.project.title).font(.headline)
            Text(appModel.localized("将使用本剧情的文本模型进行多轮工具调用，可能产生文本模型费用。本次只规划文字，不生成图片或视频。", english: "Uses this story's text model for multiple tool-calling rounds and may incur text-model costs. This run plans text only, without generating images or videos."))
            if confirmation.stage == .outline {
                Text(appModel.localized("包含人物文字画像、场景文字画像、道具描述与多个连续的 15 秒分段。", english: "Includes written character and scene profiles, prop descriptions and consecutive 15-second segments."))
            } else {
                Text(appModel.localized("仅细化选定的 \(confirmation.targets.count) 个未完成分段。", english: "Refines only the \(confirmation.targets.count) selected unfinished segments."))
            }
            if let policy {
                Text(appModel.localized("本次调用上限：\(policy.maximumModelCalls) 次（包括重试）", english: "Call limit: \(policy.maximumModelCalls), including retries"))
                    .font(.callout.monospacedDigit())
                Text(appModel.localized("可在 设置 → Agent 运行 中调整。", english: "Adjust this in Settings → Agent Runtime.")).font(.caption).foregroundStyle(.secondary)
            }
            Divider()
            Label(appModel.localized("默认使用 Memory Engine 记忆与上下文压缩", english: "Memory Engine Memory and Compaction Are Used by Default"), systemImage: "brain.head.profile")
                .font(.callout.weight(.medium))
            Text(appModel.localized("会向当前 ChatOS 服务同步已读剧情片段、人物和场景画像、提示词及工具记录，用于长剧情的上下文压缩与恢复；不会同步图片/视频二进制或模型密钥。摘要使用服务端配置的摘要 Agent，可能额外计费，不包含在上述调用上限内。", english: "Read story excerpts, character and scene profiles, prompts and tool records are synced to the current ChatOS service for long-story compaction and recovery. Image/video binaries and model keys are never synced. The server's summary Agent may incur additional costs outside this call limit."))
                .font(.caption).foregroundStyle(.secondary)
            if let error { Text(error).font(.caption).foregroundStyle(.orange) }
            HStack {
                Spacer()
                Button(appModel.localized("取消", english: "Cancel")) { dismiss() }
                Button(appModel.localized("确认并开始规划", english: "Confirm and Start Planning")) {
                    do {
                        guard let project = viewModel.project, project.id == confirmation.project.id,
                              try StoryAgentRun.digest(project) == StoryAgentRun.digest(confirmation.project) else { throw StoryAgentError.projectChanged }
                        if confirmation.stage == .outline { viewModel.planOutline() }
                        else { viewModel.refineSegments(confirmation.targets) }
                        dismiss()
                    } catch { self.error = error.localizedDescription }
                }.buttonStyle(.borderedProminent).disabled(policy == nil || !viewModel.canCreate)
            }
        }.padding(24).frame(width: 620)
        .onAppear { do { policy = try viewModel.effectiveAgentPolicy() } catch { self.error = error.localizedDescription } }
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
                    Text("\(run.checkpoint.modelCalls) / \(run.policy.maximumModelCalls)").font(.caption.monospacedDigit())
                }
                Text(label(run)).font(.caption).foregroundStyle(run.applied ? Color.green : .secondary)
                if let event = run.events.last { Text(event.detail).font(.caption).foregroundStyle(.secondary).lineLimit(2) }
                Text(appModel.localized("草稿：\(run.draft.resources.count) 个素材 · \(run.draft.segments.count) 段 · \(run.draft.totalSeconds) 秒", english: "Draft: \(run.draft.resources.count) assets · \(run.draft.segments.count) segments · \(run.draft.totalSeconds) seconds"))
                    .font(.caption)
                if let reason = run.checkpoint.stopReason { Text(reason).font(.caption).foregroundStyle(.orange).lineLimit(3) }
                HStack {
                    Button(appModel.localized("草稿 / 运行记录", english: "Draft / Run History")) { showsHistory = true }
                    Spacer()
                    if !run.applied {
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
                Text(appModel.localized("继续使用 Memory Engine、原文本模型和目标，并采用设置中当前调用预算；已有调用仍计入总数，文本及摘要调用可能继续计费。已保存的步骤不会重复执行，已完成草稿只应用到项目。", english: "Continues with Memory Engine, the original text model and targets, using the current settings budget. Previous calls still count; text and summary calls may incur further costs. Saved steps are not repeated, and a completed draft is only applied to the project."))
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
                                Text("\(run.checkpoint.modelCalls) / \(run.policy.maximumModelCalls) · \(run.draft.totalSeconds)s").monospacedDigit()
                                Text(appModel.localized("使用 Memory Engine 记忆与压缩", english: "Uses Memory Engine memory and compaction"))
                                    .font(.caption).foregroundStyle(.secondary)
                                if !run.applied && !viewModel.isBusy {
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
                                            Text(segment.title + " · 15s").fontWeight(.medium)
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
