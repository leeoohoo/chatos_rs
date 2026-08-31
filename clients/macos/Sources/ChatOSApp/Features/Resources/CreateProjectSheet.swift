import ChatOSCore
import SwiftUI

struct CreateProjectSheetHost: View {
    @StateObject private var viewModel: CreateProjectViewModel
    let onCreated: (WorkspaceProject) -> Void

    init(
        connectorStatus: LocalConnectorStatus?,
        defaultContact: WorkspaceContact?,
        filesystemService: any ProjectFilesystemServicing,
        creationService: any WorkspaceResourceCreating,
        onCreated: @escaping (WorkspaceProject) -> Void
    ) {
        _viewModel = StateObject(wrappedValue: CreateProjectViewModel(
            connectorStatus: connectorStatus,
            defaultContact: defaultContact,
            filesystemService: filesystemService,
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
        .frame(minWidth: 680, idealWidth: 720, minHeight: 570, idealHeight: 620)
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
            .frame(maxWidth: .infinity, minHeight: 300)

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
