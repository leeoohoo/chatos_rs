import ChatOSCore
import SwiftUI

struct ProjectGitWorkbenchView: View {
    @ObservedObject var viewModel: ProjectGitViewModel
    let onOpenFile: (ProjectGitChange) -> Void

    @State private var showsCreateBranch = false
    @State private var branchToMerge: ProjectGitBranch?
    @State private var remoteToEdit: ProjectGitRemote?
    @State private var showsNewRemote = false

    var body: some View {
        Group {
            if viewModel.projectRoot == nil {
                ContentUnavailableView(
                    "没有项目目录",
                    systemImage: "folder.badge.questionmark",
                    description: Text("请先为项目连接本机目录。")
                )
            } else if viewModel.isLoading && !viewModel.snapshot.isRepository {
                ProgressView("正在读取 Git 状态…")
            } else if !viewModel.snapshot.isRepository {
                ContentUnavailableView {
                    Label("尚未启用 Git", systemImage: "arrow.triangle.branch")
                } description: {
                    Text("可以在当前项目目录创建 Git 仓库。")
                } actions: {
                    Button("初始化仓库") {
                        Task { await viewModel.initializeRepository() }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(viewModel.isPerformingOperation)
                }
            } else {
                repositoryContent
            }
        }
        .workspaceFill()
        .background(AppPalette.canvas)
        .sheet(isPresented: $showsCreateBranch) {
            ProjectGitCreateBranchSheet { name, switchToBranch in
                Task { await viewModel.createBranch(name: name, switchToBranch: switchToBranch) }
            }
        }
        .sheet(item: $branchToMerge) { branch in
            ProjectGitMergeSheet(
                branch: branch.name,
                currentBranch: viewModel.snapshot.currentBranch ?? "当前分支"
            ) {
                Task { await viewModel.mergeBranch(branch.name) }
            }
        }
        .sheet(isPresented: $showsNewRemote) {
            remoteEditor(remote: nil)
        }
        .sheet(item: $remoteToEdit) { remote in
            remoteEditor(remote: remote)
        }
        .alert("Git 操作失败", isPresented: errorPresented) {
            Button("好") { viewModel.dismissError() }
        } message: {
            Text(viewModel.errorMessage ?? "未知错误")
        }
    }

    private var repositoryContent: some View {
        VStack(spacing: 0) {
            repositoryToolbar
            Divider()
            ScrollView {
                LazyVStack(spacing: 0) {
                    commitComposer
                    Divider()
                    ProjectGitChangeListView(
                        title: "已暂存",
                        changes: viewModel.stagedChanges,
                        staged: true,
                        isBusy: viewModel.isPerformingOperation,
                        onOpenDiff: { change in
                            Task { await viewModel.showDiff(for: change, staged: true) }
                        },
                        onToggleStage: { change in
                            Task { await viewModel.unstage(change) }
                        },
                        onToggleAll: {
                            Task { await viewModel.unstageAll() }
                        }
                    )
                    if viewModel.stagedChanges.isEmpty {
                        emptyChangeHint("暂存区为空")
                    }
                    Divider()
                    ProjectGitChangeListView(
                        title: "更改",
                        changes: viewModel.workingTreeChanges,
                        staged: false,
                        isBusy: viewModel.isPerformingOperation,
                        onOpenDiff: { change in
                            Task { await viewModel.showDiff(for: change, staged: false) }
                        },
                        onToggleStage: { change in
                            Task { await viewModel.stage(change) }
                        },
                        onToggleAll: {
                            Task { await viewModel.stageAll() }
                        }
                    )
                    if viewModel.workingTreeChanges.isEmpty {
                        emptyChangeHint("工作区没有未暂存修改")
                    }
                    Divider()
                    ProjectGitHistoryView(commits: viewModel.snapshot.commits)
                }
            }
            .overlay(alignment: .top) {
                if viewModel.isPerformingOperation {
                    ProgressView()
                        .controlSize(.small)
                        .padding(8)
                        .background(.regularMaterial, in: Capsule())
                        .padding(.top, 8)
                }
            }
        }
    }

    private var repositoryToolbar: some View {
        HStack(spacing: 8) {
            Menu {
                Section("切换分支") {
                    ForEach(viewModel.snapshot.branches) { branch in
                        Button {
                            Task { await viewModel.switchBranch(branch.name) }
                        } label: {
                            if branch.isCurrent {
                                Label(branch.name, systemImage: "checkmark")
                            } else {
                                Text(branch.name)
                            }
                        }
                        .disabled(branch.isCurrent || viewModel.isPerformingOperation)
                    }
                }
            } label: {
                Label(branchTitle, systemImage: "arrow.triangle.branch")
                    .lineLimit(1)
            }
            .menuStyle(.borderlessButton)
            .help("切换分支")

            Spacer(minLength: 4)

            Button {
                showsCreateBranch = true
            } label: {
                Image(systemName: "plus")
            }
            .buttonStyle(.plain)
            .help("新建分支")
            .disabled(viewModel.isPerformingOperation)

            Menu {
                let candidates = viewModel.snapshot.branches.filter { !$0.isCurrent }
                if candidates.isEmpty {
                    Text("没有可合并的其他分支")
                } else {
                    ForEach(candidates) { branch in
                        Button(branch.name) { branchToMerge = branch }
                    }
                }
            } label: {
                Image(systemName: "arrow.triangle.merge")
            }
            .menuStyle(.borderlessButton)
            .help("合并分支到当前分支")
            .disabled(viewModel.isPerformingOperation)

            Button {
                if viewModel.snapshot.remotes.isEmpty {
                    showsNewRemote = true
                } else {
                    Task { await viewModel.pull() }
                }
            } label: {
                Image(systemName: "arrow.down.circle")
            }
            .buttonStyle(.plain)
            .help(viewModel.snapshot.remotes.isEmpty ? "先配置远程仓库" : "拉取远程更新（仅快进）")
            .disabled(viewModel.isPerformingOperation)

            Button {
                if viewModel.snapshot.remotes.isEmpty {
                    showsNewRemote = true
                } else {
                    Task { await viewModel.push() }
                }
            } label: {
                Image(systemName: "arrow.up.circle")
            }
            .buttonStyle(.plain)
            .help(pushHelp)
            .disabled(viewModel.isPerformingOperation)

            Menu {
                Button("添加远程仓库", systemImage: "plus") {
                    showsNewRemote = true
                }
                if !viewModel.snapshot.remotes.isEmpty {
                    Divider()
                    Section("远程仓库") {
                        ForEach(viewModel.snapshot.remotes) { remote in
                            Button(remote.name) { remoteToEdit = remote }
                        }
                    }
                }
            } label: {
                Image(systemName: "network")
            }
            .menuStyle(.borderlessButton)
            .help("管理远程仓库")

            Button {
                Task { await viewModel.load() }
            } label: {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.plain)
            .help("刷新 Git 状态")
            .disabled(viewModel.isLoading || viewModel.isPerformingOperation)
        }
        .padding(.horizontal, 10)
        .frame(height: 42)
        .background(.bar)
    }

    private var commitComposer: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("提交说明", text: $viewModel.commitMessage, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .lineLimit(1...4)
                .onSubmit {
                    if canCommit { Task { await viewModel.commit() } }
                }
            Button {
                Task { await viewModel.commit() }
            } label: {
                Label(commitButtonTitle, systemImage: "checkmark.circle.fill")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .disabled(!canCommit)

            if viewModel.snapshot.aheadCount > 0 || viewModel.snapshot.behindCount > 0 {
                HStack(spacing: 10) {
                    if viewModel.snapshot.aheadCount > 0 {
                        Label("领先 \(viewModel.snapshot.aheadCount)", systemImage: "arrow.up")
                    }
                    if viewModel.snapshot.behindCount > 0 {
                        Label("落后 \(viewModel.snapshot.behindCount)", systemImage: "arrow.down")
                    }
                }
                .appFont(.caption)
                .foregroundStyle(.secondary)
            }
        }
        .padding(10)
    }

    private func emptyChangeHint(_ title: String) -> some View {
        Text(title)
            .appFont(.caption)
            .foregroundStyle(.tertiary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 30)
            .padding(.vertical, 9)
    }

    private var branchTitle: String {
        viewModel.snapshot.currentBranch
            ?? viewModel.snapshot.detachedCommit.map { "HEAD \($0)" }
            ?? "尚无提交"
    }

    private var canCommit: Bool {
        !viewModel.commitMessage.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !viewModel.stagedChanges.isEmpty
            && !viewModel.isPerformingOperation
    }

    private var commitButtonTitle: String {
        viewModel.stagedChanges.isEmpty
            ? "请先暂存修改"
            : "提交 \(viewModel.stagedChanges.count) 个文件"
    }

    private var pushHelp: String {
        if viewModel.snapshot.remotes.isEmpty { return "先配置远程仓库" }
        return viewModel.snapshot.upstream == nil ? "发布当前分支" : "推送当前分支"
    }

    private func remoteEditor(remote: ProjectGitRemote?) -> some View {
        ProjectGitRemoteEditorSheet(
            remote: remote,
            onSave: { originalName, name, url in
                Task {
                    await viewModel.saveRemote(
                        originalName: originalName,
                        name: name,
                        url: url
                    )
                }
            },
            onRemove: remote == nil ? nil : { name in
                Task { await viewModel.removeRemote(name) }
            }
        )
    }

    private var errorPresented: Binding<Bool> {
        Binding(
            get: { viewModel.errorMessage != nil },
            set: { if !$0 { viewModel.dismissError() } }
        )
    }
}
