import ChatOSCore
import SwiftUI

struct CreateProjectSheetHost: View {
    @StateObject private var viewModel: CreateProjectViewModel
    let onCreated: (WorkspaceProject) -> Void

    init(
        connectorStatus: LocalConnectorStatus?,
        defaultContact: WorkspaceContact?,
        filesystemService: any ProjectFilesystemServicing,
        gitService: any ProjectGitServicing,
        creationService: any WorkspaceResourceCreating,
        onCreated: @escaping (WorkspaceProject) -> Void
    ) {
        _viewModel = StateObject(wrappedValue: CreateProjectViewModel(
            connectorStatus: connectorStatus,
            defaultContact: defaultContact,
            filesystemService: filesystemService,
            gitService: gitService,
            creationService: creationService
        ))
        self.onCreated = onCreated
    }

    var body: some View {
        CreateProjectSheet(viewModel: viewModel, onCreated: onCreated)
    }
}

struct CreateProjectSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: CreateProjectViewModel
    let onCreated: (WorkspaceProject) -> Void

    @State private var showingNewFolderPrompt = false
    @State private var newFolderName = ""

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            form
            Divider()
            footer
        }
        .frame(minWidth: 700, idealWidth: 740, minHeight: 650, idealHeight: 700)
        .task { await viewModel.loadInitialDirectory() }
        .alert("新建文件夹", isPresented: $showingNewFolderPrompt) {
            TextField("文件夹名称", text: $newFolderName)
            Button("取消", role: .cancel) { newFolderName = "" }
            Button("创建") {
                let name = newFolderName
                newFolderName = ""
                Task { await viewModel.createDirectory(named: name) }
            }
        } message: {
            Text("文件夹将创建在当前所选目录中。")
        }
    }

    private var header: some View {
        HStack(spacing: 12) {
            Image(systemName: "folder.badge.plus")
                .appFont(.title2)
                .foregroundStyle(Color.accentColor)
                .frame(width: 34, height: 34)
                .background(Color.accentColor.opacity(0.1), in: RoundedRectangle(cornerRadius: 9))
            VStack(alignment: .leading, spacing: 2) {
                Text("新建项目")
                    .appFont(.title3.weight(.semibold))
                Text("选择本机目录，项目会自动连接网关并绑定“叽咕狸”。")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(18)
    }

    private var form: some View {
        VStack(alignment: .leading, spacing: 16) {
            if viewModel.selectedWorkspace == nil {
                Label("本机网关没有提供可用工作区", systemImage: "externaldrive.badge.exclamationmark")
                    .foregroundStyle(.orange)
            }

            if !viewModel.hasDefaultContact {
                Label("没有找到默认联系人“叽咕狸”，刷新资源后再试。", systemImage: "person.crop.circle.badge.exclamationmark")
                    .foregroundStyle(.orange)
            }

            CreateProjectDirectoryBrowser(
                viewModel: viewModel,
                showingNewFolderPrompt: $showingNewFolderPrompt
            )
            .frame(maxWidth: .infinity, minHeight: 250)

            VStack(alignment: .leading, spacing: 7) {
                Text("项目名称")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                TextField(
                    "项目名称",
                    text: Binding(
                        get: { viewModel.projectName },
                        set: { value in viewModel.updateProjectName(value) }
                    )
                )
                .textFieldStyle(.roundedBorder)
            }

            repositoryModeSection

            if let errorMessage = viewModel.errorMessage {
                Label {
                    Text(errorMessage)
                        .appFont(.caption)
                        .fixedSize(horizontal: false, vertical: true)
                } icon: {
                    Image(systemName: "exclamationmark.triangle.fill")
                }
                .foregroundStyle(.orange)
            }
        }
        .padding(18)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }

    private var repositoryModeSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("代码托管方式")
                .appFont(.caption.weight(.semibold))
                .foregroundStyle(.secondary)

            HStack(spacing: 10) {
                repositoryModeCard(
                    mode: .external,
                    title: "使用现有 Git",
                    detail: "沿用当前目录的远程仓库和本机 Git 凭据",
                    icon: "point.3.connected.trianglepath.dotted"
                )
                repositoryModeCard(
                    mode: .managed,
                    title: "ChatOS 托管 Git",
                    detail: "把所选目录的代码复制到你的 ChatOS Harness 空间",
                    icon: "externaldrive.badge.icloud"
                )
            }

            repositoryModeDetail
        }
    }

    private func repositoryModeCard(
        mode: LocalProjectRepositoryMode,
        title: LocalizedStringKey,
        detail: LocalizedStringKey,
        icon: String
    ) -> some View {
        let isSelected = viewModel.repositoryMode == mode
        return Button {
            viewModel.selectRepositoryMode(mode)
        } label: {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: icon)
                    .appFont(.body.weight(.semibold))
                    .foregroundStyle(isSelected ? Color.accentColor : .secondary)
                    .frame(width: 22)
                VStack(alignment: .leading, spacing: 3) {
                    Text(title)
                        .appFont(.subheadline.weight(.semibold))
                        .foregroundStyle(.primary)
                    Text(detail)
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 0)
                Image(systemName: isSelected ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(isSelected ? Color.accentColor : Color.secondary.opacity(0.7))
            }
            .padding(12)
            .frame(maxWidth: .infinity, minHeight: 72, alignment: .topLeading)
            .background(
                isSelected ? Color.accentColor.opacity(0.08) : Color.primary.opacity(0.025),
                in: RoundedRectangle(cornerRadius: 10)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .stroke(isSelected ? Color.accentColor.opacity(0.65) : Color.primary.opacity(0.1))
            }
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private var repositoryModeDetail: some View {
        switch viewModel.repositoryMode {
        case .managed:
            Label(
                "创建项目后会将所选目录的源代码上传到 ChatOS Harness。",
                systemImage: "info.circle"
            )
            .appFont(.caption)
            .foregroundStyle(.secondary)
        case .external:
            if viewModel.isInspectingGit {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("正在读取当前目录的 Git 配置…")
                }
                .appFont(.caption)
                .foregroundStyle(.secondary)
            } else if !viewModel.detectedGitRemotes.isEmpty {
                HStack(spacing: 10) {
                    Text("远程仓库")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                    Picker(
                        "远程仓库",
                        selection: Binding(
                            get: { viewModel.selectedGitRemoteName ?? "" },
                            set: { viewModel.selectGitRemote(named: $0) }
                        )
                    ) {
                        ForEach(viewModel.detectedGitRemotes) { remote in
                            Text("\(remote.name) · \(remote.url)").tag(remote.name)
                        }
                    }
                    .labelsHidden()
                    .frame(maxWidth: .infinity)
                }
                Text("ChatOS 不会复制代码到 Harness；拉取和推送继续使用本机 Git 的凭据配置。")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            } else if let message = viewModel.gitInspectionMessage {
                Label(message, systemImage: "exclamationmark.triangle")
                    .appFont(.caption)
                    .foregroundStyle(.orange)
            }
        case nil:
            Text("请选择一种代码托管方式后再创建项目。")
                .appFont(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var footer: some View {
        HStack {
            if viewModel.pendingCreatedProject != nil {
                Label("项目主体已创建，只需重新准备默认会话", systemImage: "checkmark.circle")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button("取消") { dismiss() }
                .keyboardShortcut(.cancelAction)
                .disabled(viewModel.isSaving)
            Button(viewModel.saveButtonTitle) {
                Task {
                    if let project = await viewModel.save() {
                        onCreated(project)
                        dismiss()
                    }
                }
            }
            .keyboardShortcut(.defaultAction)
            .buttonStyle(.borderedProminent)
            .disabled(!viewModel.canCreate)
        }
        .padding(14)
    }
}
