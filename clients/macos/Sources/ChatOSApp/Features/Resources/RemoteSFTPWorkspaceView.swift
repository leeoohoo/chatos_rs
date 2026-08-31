import AppKit
import ChatOSCore
import SwiftUI

struct RemoteSFTPWorkspaceView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: RemoteSFTPViewModel
    @State private var overwriteAction: RemoteSFTPOverwriteAction?
    @State private var showingCreateDirectory = false
    @State private var newDirectoryName = ""
    @State private var showingRename = false
    @State private var renamedEntryName = ""
    @State private var showingDeleteConfirmation = false

    init(connectionID: String, service: any RemoteFileServicing) {
        _viewModel = StateObject(
            wrappedValue: RemoteSFTPViewModel(connectionID: connectionID, service: service)
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            workspaceToolbar
            Divider()
            statusArea
            HSplitView {
                localPane
                    .frame(minWidth: 360)
                remotePane
                    .frame(minWidth: 420)
            }
        }
        .background(AppPalette.canvas)
        .task {
            viewModel.interfaceLanguage = model.interfaceLanguage
            await viewModel.load()
        }
        .onChange(of: model.interfaceLanguage) { _, language in
            viewModel.interfaceLanguage = language
        }
        .alert("新建远端目录", isPresented: $showingCreateDirectory) {
            TextField("目录名称", text: $newDirectoryName)
            Button("取消", role: .cancel) {}
            Button("创建") {
                let name = newDirectoryName
                Task { await viewModel.createRemoteDirectory(name: name) }
            }
        }
        .alert("重命名", isPresented: $showingRename) {
            TextField("新名称", text: $renamedEntryName)
            Button("取消", role: .cancel) {}
            Button("保存") {
                let name = renamedEntryName
                Task { await viewModel.renameSelectedRemote(to: name) }
            }
        }
        .confirmationDialog(
            overwriteAction?.title(language: model.interfaceLanguage)
                ?? model.localized("覆盖文件？", english: "Overwrite File?"),
            isPresented: overwritePresented,
            titleVisibility: .visible
        ) {
            Button("覆盖", role: .destructive) {
                let action = overwriteAction
                overwriteAction = nil
                Task { await performOverwrite(action) }
            }
            Button("取消", role: .cancel) { overwriteAction = nil }
        } message: {
            Text(overwriteAction?.message(language: model.interfaceLanguage) ?? "")
        }
        .confirmationDialog(
            "删除远端项目？",
            isPresented: $showingDeleteConfirmation,
            titleVisibility: .visible
        ) {
            Button("删除", role: .destructive) {
                Task { await viewModel.deleteSelectedRemote() }
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("此操作会直接删除服务器上的文件或目录。")
        }
    }

    private var workspaceToolbar: some View {
        HStack(spacing: 10) {
            Label("SFTP", systemImage: "externaldrive.connected.to.line.below")
                .appFont(.headline)
            Spacer()
            TextField("筛选文件", text: $viewModel.searchText)
                .textFieldStyle(.roundedBorder)
                .frame(width: 210)
            Toggle("显示隐藏文件", isOn: $viewModel.showsHiddenFiles)
                .toggleStyle(.button)
                .labelStyle(.iconOnly)
                .help(viewModel.showsHiddenFiles
                      ? model.localized("隐藏点开头的文件", english: "Hide dotfiles")
                      : model.localized("显示隐藏文件", english: "Show hidden files"))
        }
        .padding(.horizontal, 12)
        .frame(height: 44)
        .background(AppPalette.surface)
    }

    private var statusArea: some View {
        VStack(spacing: 0) {
            if viewModel.isTransferring {
                HStack(spacing: 9) {
                    ProgressView().controlSize(.small)
                    Text(viewModel.transferLabel
                         ?? model.localized("正在传输…", english: "Transferring…"))
                        .appFont(.caption)
                    Spacer()
                }
                .padding(.horizontal, 14)
                .frame(height: 34)
                .background(Color.accentColor.opacity(0.08))
            }
            if let notice = viewModel.notice {
                notificationRow(notice, icon: "checkmark.circle.fill", color: .green)
            }
            if let error = viewModel.errorMessage {
                notificationRow(error, icon: "exclamationmark.triangle.fill", color: .orange)
            }
        }
    }

    private func notificationRow(_ text: String, icon: String, color: Color) -> some View {
        HStack(spacing: 8) {
            Image(systemName: icon).foregroundStyle(color)
            Text(text).appFont(.caption).lineLimit(2)
            Spacer()
            Button("关闭", systemImage: "xmark") {
                viewModel.notice = nil
                viewModel.errorMessage = nil
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(color.opacity(0.07))
    }

    private var localPane: some View {
        VStack(spacing: 0) {
            SFTPBrowserHeader(
                title: model.localized("本机", english: "Local"),
                path: viewModel.localPath.path,
                canGoUp: viewModel.localPath.path != "/",
                isLoading: viewModel.isLoadingLocal,
                onChooseLocation: chooseLocalDirectory,
                onGoUp: { Task { await viewModel.goToLocalParent() } },
                onRefresh: { Task { await viewModel.reloadLocal() } },
                trailingActions: nil
            )
            Divider()
            SFTPColumnHeader()
            Divider()
            if viewModel.visibleLocalEntries.isEmpty {
                SFTPEmptyDirectoryView(isLoading: viewModel.isLoadingLocal)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(viewModel.visibleLocalEntries) { entry in
                            SFTPFileRow(
                                name: entry.name,
                                isDirectory: entry.isDirectory,
                                size: entry.size,
                                modifiedAt: entry.modifiedAt,
                                isSelected: viewModel.selectedLocalPath == entry.id,
                                onOpen: { Task { await viewModel.openLocal(entry) } },
                                onSelect: { viewModel.selectedLocalPath = entry.id }
                            )
                            Divider().padding(.leading, 38)
                        }
                    }
                }
            }
            transferBar(direction: .upload)
        }
        .background(AppPalette.surface)
    }

    private var remotePane: some View {
        VStack(spacing: 0) {
            SFTPBrowserHeader(
                title: model.localized("服务器", english: "Server"),
                path: viewModel.remotePath,
                canGoUp: viewModel.remoteParentPath != nil,
                isLoading: viewModel.isLoadingRemote,
                onChooseLocation: nil,
                onGoUp: { Task { await viewModel.goToRemoteParent() } },
                onRefresh: { Task { await viewModel.reloadRemote() } },
                trailingActions: AnyView(remoteActions)
            )
            Divider()
            SFTPColumnHeader()
            Divider()
            if viewModel.visibleRemoteEntries.isEmpty {
                SFTPEmptyDirectoryView(isLoading: viewModel.isLoadingRemote)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(viewModel.visibleRemoteEntries) { entry in
                            SFTPFileRow(
                                name: entry.name,
                                isDirectory: entry.isDirectory,
                                size: entry.size,
                                modifiedAt: entry.modifiedAt,
                                isSelected: viewModel.selectedRemotePath == entry.path,
                                onOpen: { Task { await viewModel.openRemote(entry) } },
                                onSelect: { viewModel.selectedRemotePath = entry.path }
                            )
                            .contextMenu { remoteContextMenu(entry) }
                            Divider().padding(.leading, 38)
                        }
                    }
                }
            }
            transferBar(direction: .download)
        }
        .background(AppPalette.surface)
    }

    private var remoteActions: some View {
        HStack(spacing: 7) {
            Button("新建目录", systemImage: "folder.badge.plus") {
                newDirectoryName = ""
                showingCreateDirectory = true
            }
            .labelStyle(.iconOnly)
            .help("新建远端目录")
            Menu {
                Button("重命名", systemImage: "pencil") { beginRename() }
                    .disabled(viewModel.selectedRemoteEntry == nil)
                Button("删除", systemImage: "trash", role: .destructive) {
                    showingDeleteConfirmation = true
                }
                .disabled(viewModel.selectedRemoteEntry == nil)
            } label: {
                Image(systemName: "ellipsis.circle")
            }
        }
        .buttonStyle(.borderless)
    }

    private enum TransferDirection { case upload, download }

    private func transferBar(direction: TransferDirection) -> some View {
        HStack {
            switch direction {
            case .upload:
                Text(viewModel.selectedLocalEntry?.name
                     ?? model.localized("选择本机文件", english: "Select a local file"))
                    .lineLimit(1)
                Spacer()
                Button("上传到服务器", systemImage: "arrow.right") { beginUpload() }
                    .buttonStyle(.borderedProminent)
                    .disabled(!viewModel.canUpload)
            case .download:
                Text(viewModel.selectedRemoteEntry?.name
                     ?? model.localized("选择远端文件", english: "Select a remote file"))
                    .lineLimit(1)
                Spacer()
                Button("下载到本机", systemImage: "arrow.left") { beginDownload() }
                    .buttonStyle(.borderedProminent)
                    .disabled(!viewModel.canDownload)
            }
        }
        .appFont(.caption)
        .foregroundStyle(.secondary)
        .padding(.horizontal, 12)
        .frame(height: 46)
        .background(AppPalette.surfaceSubtle)
    }

    @ViewBuilder
    private func remoteContextMenu(_ entry: RemoteFileEntry) -> some View {
        Button("重命名", systemImage: "pencil") {
            viewModel.selectedRemotePath = entry.path
            beginRename()
        }
        Button("删除", systemImage: "trash", role: .destructive) {
            viewModel.selectedRemotePath = entry.path
            showingDeleteConfirmation = true
        }
    }

    private func chooseLocalDirectory() {
        let panel = NSOpenPanel()
        panel.title = model.localized("选择本机目录", english: "Choose Local Folder")
        panel.directoryURL = viewModel.localPath
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Task { await viewModel.setLocalDirectory(url) }
    }

    private func beginUpload() {
        if let action = viewModel.overwriteActionForUpload() {
            overwriteAction = action
        } else {
            Task { await viewModel.uploadSelected(overwrite: false) }
        }
    }

    private func beginDownload() {
        if let action = viewModel.overwriteActionForDownload() {
            overwriteAction = action
        } else {
            Task { await viewModel.downloadSelected(overwrite: false) }
        }
    }

    private func beginRename() {
        guard let entry = viewModel.selectedRemoteEntry else { return }
        renamedEntryName = entry.name
        showingRename = true
    }

    private func performOverwrite(_ action: RemoteSFTPOverwriteAction?) async {
        switch action {
        case .upload: await viewModel.uploadSelected(overwrite: true)
        case .download: await viewModel.downloadSelected(overwrite: true)
        case nil: break
        }
    }

    private var overwritePresented: Binding<Bool> {
        Binding(
            get: { overwriteAction != nil },
            set: { if !$0 { overwriteAction = nil } }
        )
    }
}
