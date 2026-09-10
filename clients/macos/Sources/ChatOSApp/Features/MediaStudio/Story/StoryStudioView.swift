import ChatOSCore
import SwiftUI

struct StoryStudioView: View {
    @EnvironmentObject private var appModel: AppModel
    @ObservedObject var viewModel: StoryStudioViewModel
    @ObservedObject var mediaStudio: MediaStudioViewModel
    @State private var showsCreate = false
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
                StoryWorkbenchView(viewModel: viewModel, mediaStudio: mediaStudio, project: project) {
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
        .sheet(item: $settingsProject) { project in
            StoryProjectForm(viewModel: viewModel, mediaStudio: mediaStudio, project: project)
                .environmentObject(appModel)
        }
        .onChange(of: appModel.authentication.phase) { _, _ in showsCreate = false; settingsProject = nil }
    }

    private var projectList: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack {
                VStack(alignment: .leading, spacing: 6) {
                    Text(appModel.localized("我的剧情", english: "My Stories")).font(.title2.bold())
                    Text(appModel.localized("每个剧情独立保存模型、素材和分段计划", english: "Each story keeps its own models, assets and segment plans"))
                        .font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                Button { showsCreate = true } label: {
                    Label(appModel.localized("新建剧情", english: "New Story"), systemImage: "plus")
                }.buttonStyle(.borderedProminent).tint(.purple).disabled(!viewModel.canCreate)
            }
            if viewModel.isBusy {
                HStack { ProgressView().controlSize(.small); Text(viewModel.operation); Spacer()
                    Button(appModel.localized("完成当前步骤后暂停", english: "Pause after current step")) { viewModel.requestPause() }
                        .disabled(viewModel.pauseRequested)
                }.font(.callout)
            }
            if viewModel.isLoading {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if viewModel.projects.isEmpty {
                ContentUnavailableView {
                    Label(appModel.localized("从一个故事开始", english: "Start with a Story"), systemImage: "film.stack")
                } description: {
                    Text(appModel.localized("创建剧情，选择三类模型，再把完整故事拆成连续的 15 秒视频计划。", english: "Create a story, choose three models, then plan a sequence of 15-second videos."))
                } actions: {
                    Button(appModel.localized("创建第一个剧情", english: "Create Your First Story")) { showsCreate = true }
                        .disabled(!viewModel.canCreate)
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 300, maximum: 480), spacing: 18)], spacing: 18) {
                        ForEach(viewModel.projects) { project in
                            Button { viewModel.open(project.id) } label: { projectCard(project) }
                                .buttonStyle(.plain).disabled(viewModel.isBusy && viewModel.activeProjectID != project.id)
                        }
                    }.padding(2)
                }
            }
        }.padding(24)
    }

    private func projectCard(_ project: StoryProject) -> some View {
        VStack(alignment: .leading, spacing: 15) {
            HStack {
                Image(systemName: "film.stack.fill").font(.title2).foregroundStyle(.purple)
                Spacer()
                Text(project.segments.isEmpty ? appModel.localized("待规划", english: "Draft")
                     : project.completedCount == project.segments.count ? appModel.localized("已完成", english: "Complete")
                     : project.hasUnresolvedJobs ? appModel.localized("任务待查询", english: "Check Tasks")
                     : appModel.localized("创作中", english: "In Progress"))
                    .font(.caption).foregroundStyle(.secondary)
            }
            Text(project.title).font(.headline).lineLimit(1)
            Text(project.description.isEmpty ? appModel.localized("暂无描述", english: "No description") : project.description)
                .font(.callout).foregroundStyle(.secondary).lineLimit(2).frame(height: 36, alignment: .top)
            Divider()
            HStack {
                Text("\(project.segments.count) " + appModel.localized("段", english: "segments") + " · \(project.totalSeconds)s")
                Spacer()
                Text("\(project.completedCount) / \(project.segments.count) " + appModel.localized("已完成", english: "complete"))
            }.font(.caption).foregroundStyle(.secondary)
            Text(project.updatedAt, format: .dateTime.month().day().hour().minute())
                .font(.caption2).foregroundStyle(.tertiary)
        }
        .padding(20).frame(maxWidth: .infinity, alignment: .leading)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(Color.primary.opacity(0.07)))
        .contentShape(Rectangle())
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
        VStack(alignment: .leading, spacing: 18) {
            Text(project == nil ? appModel.localized("新建剧情", english: "New Story") : appModel.localized("剧情设置", english: "Story Settings"))
                .font(.title2.bold())
            Form {
                TextField(appModel.localized("标题", english: "Title"), text: $title)
                LabeledContent(appModel.localized("描述", english: "Description")) {
                    TextEditor(text: $description).frame(height: 72)
                        .overlay(RoundedRectangle(cornerRadius: 5).stroke(Color.secondary.opacity(0.2)))
                }
                modelPicker(appModel.localized("文本模型", english: "Text Model"), selection: $textModel)
                modelPicker(appModel.localized("图片模型", english: "Image Model"), selection: $imageModel)
                    .disabled(project?.hasUnresolvedImageJobs == true)
                modelPicker(appModel.localized("视频模型", english: "Video Model"), selection: $videoModel)
                    .disabled(project?.hasUnresolvedVideoJobs == true)
            }.formStyle(.grouped)
            HStack {
                if mediaStudio.isLoadingModels { ProgressView().controlSize(.small) }
                Text(appModel.localized("展示所有可用模型，由你选择用途；不要求开启任务使用。", english: "Choose from all available models; task usage is not required."))
                    .font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button(appModel.localized("刷新模型", english: "Refresh Models")) { mediaStudio.reloadModels() }
                    .disabled(mediaStudio.isLoadingModels)
            }
            Text(appModel.localized("文本规划需支持 OpenAI 兼容工具调用；视频生成需支持 15 秒。描述不是剧情原文，创建后再粘贴完整故事。", english: "Planning requires compatible tool calling; video generation requires 15-second support. Paste the complete story after creation."))
                .font(.caption).foregroundStyle(.secondary)
            if let error = viewModel.errorMessage ?? mediaStudio.errorMessage { Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
            HStack {
                Text(appModel.localized("创建只保存项目，不启动 AI 生成。", english: "Creating only saves the project; no AI generation starts."))
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
                }.buttonStyle(.borderedProminent).tint(.purple).disabled(!isValid || viewModel.isBusy || mediaStudio.isLoadingModels)
            }
        }.padding(24).frame(width: 660)
        .onAppear {
            if let project {
                title = project.title; description = project.description
                textModel = project.models.textModelID; imageModel = project.models.imageModelID; videoModel = project.models.videoModelID
            }
        }
        .interactiveDismissDisabled(viewModel.isBusy)
    }

    private func modelPicker(_ label: String, selection: Binding<String>) -> some View {
        Picker(label, selection: selection) {
            Text(appModel.localized("请选择模型", english: "Choose a model")).tag("")
            if !selection.wrappedValue.isEmpty && !mediaStudio.models.contains(where: { $0.id == selection.wrappedValue }) {
                Text(appModel.localized("原模型不可用，请重新选择", english: "Previous model unavailable — choose again")).tag(selection.wrappedValue)
            }
            ForEach(mediaStudio.models) { model in
                Text("\(model.name) / \(model.modelName)").tag(model.id)
            }
        }
    }
}
