import AppKit
import ChatOSCore
import SwiftUI

struct CreateProjectSheetHost: View {
    @StateObject private var viewModel: CreateProjectViewModel
    let onCreated: (WorkspaceProject) -> Void

    init(
        creationService: any LocalProjectCreating,
        onCreated: @escaping (WorkspaceProject) -> Void
    ) {
        _viewModel = StateObject(wrappedValue: CreateProjectViewModel(creationService: creationService))
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

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            form
                .disabled(viewModel.isSaving)
            Divider()
            footer
        }
        .frame(minWidth: 620, idealWidth: 660, minHeight: 390, idealHeight: 430)
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
                Text("项目保存在本机。Git 由本机管理，聊天可在创建后单独准备。")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(18)
    }

    private var form: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 8) {
                Text("项目目录")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                HStack(spacing: 10) {
                    Image(systemName: "folder.fill")
                        .foregroundStyle(Color.accentColor)
                    Text(viewModel.selectedDirectoryPath ?? "尚未选择文件夹")
                        .appFont(.body)
                        .foregroundStyle(viewModel.selectedDirectoryPath == nil ? .secondary : .primary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: 8)
                    Button(viewModel.selectedDirectoryPath == nil ? "选择文件夹…" : "更改…") {
                        chooseDirectory()
                    }
                }
                .padding(12)
                .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay {
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .stroke(AppPalette.border, lineWidth: 1)
                }
                Text("可以选择这台 Mac 上当前账户有权访问的任意文件夹。")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }

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

            Label("不会上传代码或创建托管仓库，也不要求 Git remote 或默认联系人。", systemImage: "internaldrive")
                .appFont(.caption)
                .foregroundStyle(.secondary)

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

    private func chooseDirectory() {
        let panel = NSOpenPanel()
        panel.title = "选择项目文件夹"
        panel.prompt = "选择"
        panel.message = "选择这台 Mac 上的一个文件夹作为项目目录。"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = true
        panel.directoryURL = viewModel.selectedDirectoryPath.map { URL(fileURLWithPath: $0) }
            ?? FileManager.default.homeDirectoryForCurrentUser
        guard panel.runModal() == .OK, let url = panel.url else { return }
        viewModel.selectDirectory(url)
    }

    private var footer: some View {
        HStack {
            Spacer()
            Button("取消") { dismiss() }
                .keyboardShortcut(.cancelAction)
                .disabled(viewModel.isSaving)
            Button("创建项目") {
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
