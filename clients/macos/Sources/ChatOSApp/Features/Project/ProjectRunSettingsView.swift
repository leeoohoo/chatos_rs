import ChatOSCore
import SwiftUI

struct ProjectRunSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: ProjectRunSettingsViewModel
    @ObservedObject private var petPreferences: PetPreferencesStore
    @State private var projectDeletionAlert: ProjectSettingsDeletionAlert?
    @State private var isDeletingProject = false
    let projectID: String
    let projectName: String
    let rootPath: String?

    init(
        projectID: String,
        projectName: String,
        rootPath: String?,
        service: any ProjectRunServicing,
        petPreferences: PetPreferencesStore
    ) {
        self.projectID = projectID
        self.projectName = projectName
        self.rootPath = rootPath
        self.petPreferences = petPreferences
        _viewModel = StateObject(wrappedValue: ProjectRunSettingsViewModel(projectID: projectID, service: service))
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                pageHeader
                petQuickAccessSection
                if let error = viewModel.catalog?.errorMessage { notice(error, color: .red, icon: "exclamationmark.triangle.fill") }
                if let error = viewModel.errorMessage { notice(error, color: .red, icon: "exclamationmark.triangle.fill") }
                if let statusNotice = viewModel.notice {
                    notice(noticeText(statusNotice), color: .green, icon: "checkmark.circle.fill")
                }
                preflightSection
                targetsSection
                ProjectRunInstancesSection(viewModel: viewModel)
                ProjectRunEnvironmentSection(viewModel: viewModel)
                configurationFilesSection
                projectManagementSection
            }
            .padding(24)
            .frame(maxWidth: 980, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .center)
        }
        .workspaceFill(alignment: .top)
        .task {
            await viewModel.load()
            await viewModel.monitorRuns()
        }
        .overlay { if viewModel.isLoading { ProgressView().controlSize(.large) } }
        .alert(item: $projectDeletionAlert) { alert in
            switch alert {
            case .confirmation:
                Alert(
                    title: Text(model.localized("删除项目？", english: "Delete Project?")),
                    message: Text(model.localized(
                        "“\(projectName)”会从 ChatOS 中移除，本机项目文件夹不会被删除。",
                        english: "\(projectName) will be removed from ChatOS. Its local folder will not be deleted."
                    )),
                    primaryButton: .destructive(Text(model.localized("删除", english: "Delete"))) {
                        Task { await deleteProject() }
                    },
                    secondaryButton: .cancel(Text(model.localized("取消", english: "Cancel")))
                )
            case let .failure(message):
                Alert(
                    title: Text(model.localized("项目删除失败", english: "Project Deletion Failed")),
                    message: Text(message),
                    dismissButton: .default(Text(model.localized("好", english: "OK")))
                )
            }
        }
    }

    private var petQuickAccessSection: some View {
        SettingsCard(
            title: model.localized("宠物快捷访问", english: "Pet Quick Access"),
            systemImage: "pawprint.fill"
        ) {
            HStack(alignment: .center, spacing: 18) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.localized("设为常用项目", english: "Add to Favorite Projects"))
                        .appFont(.headline)
                    Text(model.localized(
                        "开启后，单击桌面宠物即可查看最近消息并直接发送新消息。",
                        english: "When enabled, click the desktop pet to view recent messages and send new ones."
                    ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 20)
                Toggle("", isOn: favoriteBinding)
                    .labelsHidden()
                    .toggleStyle(.switch)
            }
        }
    }

    private var favoriteBinding: Binding<Bool> {
        Binding(
            get: { petPreferences.isFavorite(projectID: projectID) },
            set: { petPreferences.setFavorite($0, projectID: projectID) }
        )
    }

    private var pageHeader: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 5) {
                Text(projectName).appFont(.title2.weight(.semibold))
                Text(rootPath ?? model.localized("未配置项目目录", english: "Project Folder Not Configured"))
                    .appFont(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                HStack {
                    StatusCapsule(title: runStatusTitle, color: runStatusColor)
                    StatusCapsule(
                        title: model.localized(
                            "运行目标 \(viewModel.targets.count)",
                            english: "\(viewModel.targets.count) Run Targets"
                        ),
                        color: .secondary
                    )
                }
            }
            Spacer()
            Button("重新分析", systemImage: "wand.and.stars") { Task { await viewModel.analyze() } }
                .disabled(viewModel.isMutating)
            Button("刷新", systemImage: "arrow.clockwise") { Task { await viewModel.load() } }
                .disabled(viewModel.isLoading || viewModel.isMutating)
        }
    }

    private var preflightSection: some View {
        SettingsCard(title: "运行预检", systemImage: "checklist") {
            let issues = viewModel.environment?.validationIssues ?? []
            if issues.isEmpty {
                Label("没有发现阻塞问题", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
            } else {
                ForEach(issues) { issue in
                    VStack(alignment: .leading, spacing: 5) {
                        Label(issue.message, systemImage: issue.kind == "warning" ? "exclamationmark.triangle" : "xmark.octagon")
                            .foregroundStyle(issue.kind == "warning" ? .orange : .red)
                        if let path = issue.path { Text(path).appFont(.caption.monospaced()).foregroundStyle(.secondary) }
                        if let hint = issue.hint { Text(hint).appFont(.caption).foregroundStyle(.secondary) }
                    }
                    .padding(.vertical, 5)
                }
            }
        }
    }

    private var targetsSection: some View {
        SettingsCard(title: "运行目标", systemImage: "play.rectangle") {
            if viewModel.targets.isEmpty {
                ContentUnavailableView("没有识别到运行目标", systemImage: "play.slash", description: Text("点击“重新分析”扫描项目入口和清单文件。"))
                    .frame(minHeight: 130)
            } else {
                Picker("默认目标", selection: targetBinding) {
                    ForEach(viewModel.targets) { target in Text(target.label).tag(Optional(target.id)) }
                }
                if let target = viewModel.selectedTarget {
                    Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 9) {
                        settingRow(model.localized("命令", english: "Command"), target.command)
                        settingRow(model.localized("工作目录", english: "Working Directory"), target.cwd)
                        if let language = target.language {
                            settingRow(model.localized("语言", english: "Language"), language)
                        }
                        settingRow(model.localized("来源", english: "Source"), target.source)
                        if let manifest = target.manifestPath {
                            settingRow(model.localized("清单", english: "Manifest"), manifest)
                        }
                        if let entrypoint = target.entrypoint {
                            settingRow(model.localized("入口", english: "Entrypoint"), entrypoint)
                        }
                    }
                    .appFont(.caption)
                    .textSelection(.enabled)
                    HStack {
                        Spacer()
                        Button(
                            viewModel.isMutating
                                ? model.localized("启动中…", english: "Starting…")
                                : model.localized("启动新实例", english: "Start New Instance"),
                            systemImage: "play.fill"
                        ) { Task { await viewModel.start() } }
                            .buttonStyle(.borderedProminent)
                            .disabled(viewModel.isMutating || !(viewModel.environment?.validationIssues.isEmpty ?? true))
                    }
                }
            }
        }
    }

    private var configurationFilesSection: some View {
        SettingsCard(title: "项目配置文件", systemImage: "doc.text.magnifyingglass") {
            let files = viewModel.environment?.configurationFiles ?? []
            if files.isEmpty {
                Text("没有发现与当前运行目标相关的配置文件。").foregroundStyle(.secondary)
            } else {
                ForEach(files) { file in
                    DisclosureGroup {
                        if let preview = file.preview, !preview.isEmpty {
                            Text(preview).appFont(.caption.monospaced()).textSelection(.enabled).padding(.top, 8)
                        }
                    } label: {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(file.label).appFont(.subheadline.weight(.semibold))
                            Text(file.path).appFont(.caption.monospaced()).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
    }

    private var projectManagementSection: some View {
        SettingsCard(
            title: model.localized("项目管理", english: "Project Management"),
            systemImage: "folder.badge.minus"
        ) {
            HStack(alignment: .center, spacing: 18) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.localized("从 ChatOS 删除项目", english: "Remove Project from ChatOS"))
                        .appFont(.headline)
                    Text(model.localized(
                        "只删除项目记录和运行状态，不会删除本机项目文件夹。",
                        english: "Removes the project record and run state without deleting the local project folder."
                    ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                }
                Spacer(minLength: 20)
                Button(
                    isDeletingProject
                        ? model.localized("删除中…", english: "Deleting…")
                        : model.localized("删除项目", english: "Delete Project"),
                    systemImage: "trash",
                    role: .destructive
                ) {
                    projectDeletionAlert = .confirmation
                }
                .disabled(isDeletingProject)
            }
        }
    }

    private func deleteProject() async {
        isDeletingProject = true
        do {
            try await model.deleteProject(id: projectID)
        } catch {
            isDeletingProject = false
            projectDeletionAlert = .failure(error.localizedDescription)
        }
    }

    private func notice(_ text: String, color: Color, icon: String) -> some View {
        Label(text, systemImage: icon)
            .foregroundStyle(color)
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(color.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
    }

    private var targetBinding: Binding<String?> {
        Binding(get: { viewModel.selectedTargetID }, set: { value in Task { await viewModel.selectTarget(value) } })
    }

    private func settingRow(_ title: String, _ value: String) -> some View {
        GridRow { Text(title).foregroundStyle(.secondary); Text(value).appFont(.caption.monospaced()) }
    }

    private var runStatusTitle: String { localizedRunStatus(viewModel.state?.status ?? viewModel.catalog?.status ?? "loading") }
    private var runStatusColor: Color { viewModel.state?.isRunning == true ? .green : .secondary }
    private func localizedRunStatus(_ status: String) -> String {
        switch status.lowercased() {
        case "running": model.localized("运行中", english: "Running")
        case "ready": model.localized("就绪", english: "Ready")
        case "stopped", "exited": model.localized("已停止", english: "Stopped")
        case "error", "failed": model.localized("异常", english: "Error")
        case "idle": model.localized("空闲", english: "Idle")
        case "loading": model.localized("加载中", english: "Loading")
        default: status
        }
    }

    private func noticeText(_ notice: ProjectRunSettingsViewModel.Notice) -> String {
        switch notice {
        case .analyzed:
            model.localized("项目运行目标已重新分析。", english: "Project run targets were analyzed again.")
        case .defaultTargetSaved:
            model.localized("默认运行目标已保存。", english: "The default run target was saved.")
        case .environmentSaved:
            model.localized("工具链和环境变量已保存。", english: "Toolchains and environment variables were saved.")
        case .instanceStarted:
            model.localized("运行实例已启动。", english: "The run instance was started.")
        case .stopRequested:
            model.localized("已向实例发送停止信号。", english: "A stop signal was sent to the instance.")
        case .instanceDeleted:
            model.localized("运行实例已删除。", english: "The run instance was deleted.")
        }
    }
}

private enum ProjectSettingsDeletionAlert: Identifiable {
    case confirmation
    case failure(String)

    var id: String {
        switch self {
        case .confirmation: "confirmation"
        case let .failure(message): "failure-\(message)"
        }
    }
}

struct SettingsCard<Content: View>: View {
    let title: String
    let systemImage: String
    @ViewBuilder let content: () -> Content
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Label(LocalizedStringKey(title), systemImage: systemImage).appFont(.headline)
            content()
        }
        .padding(17)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 13))
        .overlay { RoundedRectangle(cornerRadius: 13).stroke(AppPalette.border.opacity(0.78), lineWidth: 1) }
    }
}
