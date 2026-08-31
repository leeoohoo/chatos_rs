import ChatOSCore
import SwiftUI

struct CreateProjectDirectoryBrowser: View {
    @ObservedObject var viewModel: CreateProjectViewModel
    @Binding var showingNewFolderPrompt: Bool

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            Divider()
            content
        }
        .background(AppPalette.surface)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(AppPalette.border, lineWidth: 1)
        }
    }

    private var toolbar: some View {
        HStack(spacing: 8) {
            Button {
                Task { await viewModel.goToParentDirectory() }
            } label: {
                Image(systemName: "chevron.left")
            }
            .buttonStyle(.borderless)
            .disabled(viewModel.parentPath == nil || viewModel.isLoadingDirectory)
            .help("返回上一级")

            Image(systemName: "folder.fill")
                .foregroundStyle(Color.accentColor)
            Text(viewModel.displayedLocation)
                .appFont(.body.weight(.medium))
                .lineLimit(1)
                .truncationMode(.middle)

            Spacer()

            Button {
                viewModel.toggleHiddenDirectories()
            } label: {
                Image(systemName: viewModel.showsHiddenDirectories ? "eye" : "eye.slash")
            }
            .buttonStyle(.borderless)
            .help(viewModel.showsHiddenDirectories ? "隐藏隐藏与系统目录" : "显示隐藏与系统目录")

            Button {
                showingNewFolderPrompt = true
            } label: {
                Label("新建文件夹", systemImage: "folder.badge.plus")
                    .labelStyle(.iconOnly)
            }
            .buttonStyle(.borderless)
            .disabled(viewModel.currentPath.isEmpty || viewModel.isLoadingDirectory)
            .help("在当前目录新建文件夹")

            Button {
                Task { await viewModel.refreshDirectory() }
            } label: {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.borderless)
            .disabled(viewModel.currentPath.isEmpty || viewModel.isLoadingDirectory)
            .help("刷新目录")
        }
        .padding(.horizontal, 12)
        .frame(height: 44)
    }

    @ViewBuilder
    private var content: some View {
        if viewModel.selectedWorkspace == nil {
            ContentUnavailableView(
                "没有可用的本机工作区",
                systemImage: "externaldrive.badge.exclamationmark",
                description: Text("请先在设置中确认本机设备已经连接网关。")
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if viewModel.isLoadingDirectory && viewModel.entries.isEmpty {
            VStack(spacing: 10) {
                ProgressView()
                Text("正在读取本机目录…")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if viewModel.entries.isEmpty {
            ContentUnavailableView(
                "当前目录没有子文件夹",
                systemImage: "folder",
                description: Text("可以直接使用当前目录，或在这里新建文件夹。")
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            List(viewModel.entries) { entry in
                Button {
                    Task { await viewModel.openDirectory(entry) }
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "folder.fill")
                            .foregroundStyle(Color.accentColor)
                            .frame(width: 18)
                        Text(entry.name)
                            .appFont(.body)
                            .foregroundStyle(.primary)
                            .lineLimit(1)
                        Spacer()
                        Image(systemName: "chevron.right")
                            .appFont(.caption)
                            .foregroundStyle(.tertiary)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            .listStyle(.inset)
        }
    }
}
