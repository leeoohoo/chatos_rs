import ChatOSCore
import SwiftUI

struct StoryStudioView: View {
    @EnvironmentObject private var appModel: AppModel
    @ObservedObject var viewModel: StoryStudioViewModel
    @ObservedObject var mediaStudio: MediaStudioViewModel
    @State private var showsCreate = false
    @State private var showsPrompts = false
    @State private var settingsProject: StoryProject?

    var body: some View {
        VStack(spacing: 0) {
            if let message = viewModel.errorMessage {
                HStack(alignment: .top) {
                    Image(systemName: "exclamationmark.triangle").foregroundStyle(.orange)
                    Text(message).font(.callout).textSelection(.enabled)
                    Spacer()
                    Button { viewModel.dismissError() } label: { Image(systemName: "xmark") }
                        .help(appModel.localized("关闭提示", english: "Dismiss"))
                }.padding(12).background(Color.orange.opacity(0.08))
            }
            if let project = viewModel.project {
                StoryWorkbenchView(viewModel: viewModel, mediaStudio: mediaStudio,
                                   project: viewModel.presentationProject(for: project)) {
                    settingsProject = project
                }.id(project.id)
            } else {
                projectList
            }
        }
        .sheet(isPresented: $showsCreate) {
            StoryProjectForm(viewModel: viewModel, mediaStudio: mediaStudio, project: nil)
                .environmentObject(appModel)
        }
        .sheet(isPresented: $showsPrompts) {
            StoryPromptInspectorView()
                .environmentObject(appModel)
        }
        .sheet(item: $settingsProject) { project in
            StoryProjectForm(viewModel: viewModel, mediaStudio: mediaStudio, project: project)
                .environmentObject(appModel)
        }
        .onChange(of: appModel.authentication.phase) { _, _ in
            showsCreate = false; showsPrompts = false; settingsProject = nil
        }
    }

    private var projectList: some View {
        ZStack {
            LinearGradient(colors: [Color(nsColor: .windowBackgroundColor), Color.indigo.opacity(0.045)],
                           startPoint: .topLeading, endPoint: .bottomTrailing).ignoresSafeArea()
            VStack(alignment: .leading, spacing: 22) {
                HStack(spacing: 18) {
                    ZStack {
                        RoundedRectangle(cornerRadius: 16, style: .continuous)
                            .fill(LinearGradient(colors: [.indigo, .purple], startPoint: .topLeading, endPoint: .bottomTrailing))
                        Image(systemName: "film.stack.fill").font(.system(size: 27, weight: .semibold)).foregroundStyle(.white)
                    }.frame(width: 62, height: 62).shadow(color: Color.indigo.opacity(0.2), radius: 12, y: 5)
                    VStack(alignment: .leading, spacing: 6) {
                        Text(appModel.localized("剧情工作室", english: "Story Studio")).font(.largeTitle.bold())
                        Text(appModel.localized("从完整故事出发，建立角色、场景、分镜与视频制作计划。", english: "Turn a complete story into characters, scenes, shots and a video production plan."))
                            .font(.callout).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button { showsPrompts = true } label: {
                        Label(appModel.localized("提示词中心", english: "Prompt Center"), systemImage: "text.quote")
                            .fontWeight(.semibold).padding(.vertical, 5)
                    }
                    .buttonStyle(.bordered).tint(.indigo)
                    .help(appModel.localized("查看剧情模式实际使用的提示词、上下文与工具协议",
                                             english: "Inspect the prompts, context and tool contracts used by Story Studio"))
                    VStack(alignment: .trailing, spacing: 3) {
                        Text("\(viewModel.projects.count)").font(.title2.bold().monospacedDigit()).foregroundStyle(.indigo)
                        Text(appModel.localized("个剧情项目", english: "story projects")).font(.caption).foregroundStyle(.secondary)
                    }.padding(.trailing, 8)
                    Button { showsCreate = true } label: {
                        Label(appModel.localized("新建剧情", english: "New Story"), systemImage: "plus")
                            .fontWeight(.semibold).padding(.vertical, 5)
                    }.buttonStyle(.borderedProminent).tint(.indigo).disabled(!viewModel.canCreate)
                }
                .padding(24)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Color.indigo.opacity(0.13)))
                .shadow(color: Color.black.opacity(0.035), radius: 14, y: 5)
                if viewModel.isBusy {
                    HStack { ProgressView().controlSize(.small); Text(viewModel.operation); Spacer()
                        Button(appModel.localized("完成当前步骤后暂停", english: "Pause after current step")) { viewModel.requestPause() }
                            .disabled(viewModel.pauseRequested)
                    }.font(.callout).padding(14)
                        .background(Color.indigo.opacity(0.07), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
                if viewModel.isLoading {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if viewModel.projects.isEmpty {
                    ContentUnavailableView {
                        Label(appModel.localized("从一个故事开始", english: "Start with a Story"), systemImage: "film.stack")
                    } description: {
                        Text(appModel.localized("创建剧情，选择三类模型，再把完整故事拆成2–15秒剧情段与必要的独立转场。", english: "Create a story, choose three models, then plan 2–15 second story segments and independent transitions where needed."))
                    } actions: {
                        Button(appModel.localized("创建第一个剧情", english: "Create Your First Story")) { showsCreate = true }
                            .buttonStyle(.borderedProminent).tint(.indigo).disabled(!viewModel.canCreate)
                    }.frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                } else {
                    HStack {
                        Text(appModel.localized("我的剧情", english: "My Stories")).font(.title2.bold())
                        Text(appModel.localized("最近更新", english: "Recently updated")).font(.caption).foregroundStyle(.secondary)
                        Spacer()
                    }
                    ScrollView {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 320, maximum: 500), spacing: 18)], spacing: 18) {
                            ForEach(viewModel.projects) { project in
                                Button { viewModel.open(project.id) } label: { projectCard(project) }
                                    .buttonStyle(.plain).disabled(viewModel.isBusy && viewModel.activeProjectID != project.id)
                            }
                        }
                    }.contentMargins(.vertical, 2)
                }
            }.padding(26)
        }
    }

    private func projectCard(_ project: StoryProject) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                ZStack {
                    RoundedRectangle(cornerRadius: 10, style: .continuous).fill(projectStatusColor(project).opacity(0.11))
                    Image(systemName: "film.stack.fill").font(.system(size: 17, weight: .semibold)).foregroundStyle(projectStatusColor(project))
                }.frame(width: 38, height: 38)
                Spacer()
                Text(projectStatus(project)).font(.caption.weight(.semibold)).foregroundStyle(projectStatusColor(project))
                    .padding(.horizontal, 8).padding(.vertical, 4)
                    .background(projectStatusColor(project).opacity(0.09), in: Capsule())
            }
            Text(project.title).font(.title3.weight(.semibold)).lineLimit(1)
            Text(project.description.isEmpty ? appModel.localized("暂无描述", english: "No description") : project.description)
                .font(.callout).foregroundStyle(.secondary).lineLimit(2).frame(height: 36, alignment: .top)
            if !project.segments.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Text(appModel.localized("制作进度", english: "Production Progress")).font(.caption).foregroundStyle(.secondary)
                        Spacer()
                        Text("\(project.completedCount) / \(project.segments.count)").font(.caption.bold().monospacedDigit())
                    }
                    ProgressView(value: Double(project.completedCount), total: Double(max(1, project.segments.count)))
                        .tint(projectStatusColor(project))
                }
            }
            HStack {
                Label("\(project.segments.count) " + appModel.localized("段", english: "segments"), systemImage: "rectangle.stack")
                Label("\(project.totalSeconds)s", systemImage: "clock")
                Spacer()
                Text(project.updatedAt, format: .dateTime.month().day().hour().minute())
                Image(systemName: "chevron.right").font(.caption.bold())
            }.font(.caption).foregroundStyle(.secondary)
        }
        .padding(20).frame(maxWidth: .infinity, alignment: .leading)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(projectStatusColor(project).opacity(0.13)))
        .shadow(color: Color.black.opacity(0.035), radius: 12, y: 4)
        .contentShape(Rectangle())
    }

    private func projectStatus(_ project: StoryProject) -> String {
        if project.segments.isEmpty { return appModel.localized("待规划", english: "Draft") }
        if project.completedCount == project.segments.count { return appModel.localized("已完成", english: "Complete") }
        if project.hasUnresolvedJobs { return appModel.localized("任务待核对", english: "Review Tasks") }
        return appModel.localized("制作中", english: "In Production")
    }

    private func projectStatusColor(_ project: StoryProject) -> Color {
        if project.segments.isEmpty { return .orange }
        if project.completedCount == project.segments.count { return .green }
        if project.hasUnresolvedJobs { return .red }
        return .indigo
    }
}

struct StoryProjectForm: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: StoryStudioViewModel
    @ObservedObject var mediaStudio: MediaStudioViewModel
    let project: StoryProject?
    @State private var title = ""
    @State private var description = ""
    @State private var textModel = ""
    @State private var imageModel = ""
    @State private var videoModel = ""

    private var isValid: Bool {
        !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && title.count <= 120 && description.count <= 4_000
            && [textModel, imageModel, videoModel].allSatisfy { id in mediaStudio.models.contains { $0.id == id } }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 14) {
                ZStack {
                    RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.indigo.opacity(0.11))
                    Image(systemName: project == nil ? "plus.rectangle.on.folder.fill" : "slider.horizontal.3")
                        .font(.system(size: 20, weight: .semibold)).foregroundStyle(.indigo)
                }
                .frame(width: 48, height: 48)
                VStack(alignment: .leading, spacing: 4) {
                    Text(project == nil ? appModel.localized("新建剧情", english: "New Story") : appModel.localized("剧情设置", english: "Story Settings"))
                        .font(.title2.bold())
                    Text(appModel.localized("先建立项目和模型组合，创建后再进入工作台粘贴完整剧情。", english: "Set up the project and model stack, then paste the full story in the workspace."))
                        .font(.callout).foregroundStyle(.secondary)
                }
            }
            VStack(alignment: .leading, spacing: 14) {
                Text(appModel.localized("项目信息", english: "Project Details")).font(.headline)
                LabeledContent(appModel.localized("标题", english: "Title")) {
                    TextField(appModel.localized("给这个剧情一个容易识别的名字", english: "A memorable project name"), text: $title)
                        .textFieldStyle(.roundedBorder)
                }
                LabeledContent(appModel.localized("描述", english: "Description")) {
                    TextEditor(text: $description).scrollContentBackground(.hidden).padding(8).frame(height: 82)
                        .background(Color(nsColor: .textBackgroundColor).opacity(0.72), in: RoundedRectangle(cornerRadius: 8))
                        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.primary.opacity(0.09)))
                }
            }.padding(18).background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 12))
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(appModel.localized("模型组合", english: "Model Stack")).font(.headline)
                        Text(appModel.localized("每一种模型只负责自己的制作阶段。", english: "Each model is used only for its production stage."))
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    if mediaStudio.isLoadingModels { ProgressView().controlSize(.small) }
                    Button(appModel.localized("刷新", english: "Refresh")) { mediaStudio.reloadModels() }
                        .disabled(mediaStudio.isLoadingModels)
                }
                modelPicker(appModel.localized("文本规划", english: "Text Planning"), icon: "text.bubble", selection: $textModel)
                modelPicker(appModel.localized("图片素材", english: "Image Assets"), icon: "photo", selection: $imageModel)
                    .disabled(project?.hasUnresolvedImageJobs == true)
                modelPicker(appModel.localized("视频生成", english: "Video Generation"), icon: "video", selection: $videoModel)
                    .disabled(project?.hasUnresolvedVideoJobs == true)
                Label(appModel.localized("文本模型需支持工具调用；视频模型支持的时长需覆盖所选分段。", english: "The text model must support tool calling; video-model durations must cover the selected segments."),
                      systemImage: "info.circle")
                    .font(.caption).foregroundStyle(.secondary)
            }.padding(18).background(Color.indigo.opacity(0.045), in: RoundedRectangle(cornerRadius: 12))
            if let error = viewModel.errorMessage ?? mediaStudio.errorMessage { Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
            Divider()
            HStack {
                Label(appModel.localized("这里只保存项目，不会启动 AI 或产生费用。", english: "This only saves the project. It does not run AI or incur charges."), systemImage: "checkmark.shield")
                    .font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button(appModel.localized("取消", english: "Cancel")) { dismiss() }.keyboardShortcut(.cancelAction)
                Button(project == nil ? appModel.localized("创建并进入", english: "Create and Open") : appModel.localized("保存设置", english: "Save Settings")) {
                    var draft = project ?? StoryProject(title: title, description: description,
                        models: .init(textModelID: textModel, imageModelID: imageModel, videoModelID: videoModel))
                    draft.title = title; draft.description = description
                    draft.models = .init(textModelID: textModel, imageModelID: imageModel, videoModelID: videoModel)
                    Task {
                        let saved = if project == nil { await viewModel.create(draft, availableModels: mediaStudio.models) }
                            else { await viewModel.updateSettings(draft, availableModels: mediaStudio.models) }
                        if saved { dismiss() }
                    }
                }.buttonStyle(.borderedProminent).tint(.indigo).disabled(!isValid || viewModel.isBusy || mediaStudio.isLoadingModels)
            }
        }.padding(26).frame(width: 720)
        .onAppear {
            if let project {
                title = project.title; description = project.description
                textModel = project.models.textModelID; imageModel = project.models.imageModelID; videoModel = project.models.videoModelID
            }
        }
        .interactiveDismissDisabled(viewModel.isBusy)
    }

    private func modelPicker(_ label: String, icon: String, selection: Binding<String>) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icon).foregroundStyle(.indigo).frame(width: 22)
            Text(label).font(.callout.weight(.medium)).frame(width: 96, alignment: .leading)
            Picker("", selection: selection) {
                Text(appModel.localized("请选择模型", english: "Choose a model")).tag("")
                if !selection.wrappedValue.isEmpty && !mediaStudio.models.contains(where: { $0.id == selection.wrappedValue }) {
                    Text(appModel.localized("原模型不可用，请重新选择", english: "Previous model unavailable — choose again")).tag(selection.wrappedValue)
                }
                ForEach(mediaStudio.models) { model in
                    Text("\(model.name) / \(model.modelName)").tag(model.id)
                }
            }
            .labelsHidden().frame(maxWidth: .infinity)
        }.padding(.vertical, 2)
    }
}
