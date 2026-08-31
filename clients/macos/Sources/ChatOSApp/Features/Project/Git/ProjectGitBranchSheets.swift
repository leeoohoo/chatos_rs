import ChatOSCore
import SwiftUI

struct ProjectGitCreateBranchSheet: View {
    @Environment(\.dismiss) private var dismiss
    let onCreate: (String, Bool) -> Void
    @State private var name = ""
    @State private var switchToBranch = true

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 10) {
                Image(systemName: "arrow.triangle.branch")
                    .appFont(.title2)
                    .foregroundStyle(AppPalette.ai)
                Text("新建分支")
                    .appFont(.title2.weight(.semibold))
            }
            TextField("例如 feature/git-workbench", text: $name)
                .textFieldStyle(.roundedBorder)
            Toggle("创建后立即切换到新分支", isOn: $switchToBranch)
            HStack {
                Spacer()
                Button("取消") { dismiss() }
                Button("创建") {
                    let value = name.trimmingCharacters(in: .whitespacesAndNewlines)
                    dismiss()
                    onCreate(value, switchToBranch)
                }
                .buttonStyle(.borderedProminent)
                .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(24)
        .frame(width: 430)
    }
}

struct ProjectGitMergeSheet: View {
    @Environment(\.dismiss) private var dismiss
    let branch: String
    let currentBranch: String
    let onMerge: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 10) {
                Image(systemName: "arrow.triangle.merge")
                    .appFont(.title2)
                    .foregroundStyle(AppPalette.ai)
                Text("合并分支")
                    .appFont(.title2.weight(.semibold))
            }
            Text("将“\(branch)”合并到“\(currentBranch)”。")
                .appFont(.body)
            Label(
                "如果存在冲突，工作台会保留冲突文件供你处理，不会自动覆盖内容。",
                systemImage: "info.circle"
            )
            .appFont(.caption)
            .foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("取消") { dismiss() }
                Button("开始合并") {
                    dismiss()
                    onMerge()
                }
                .buttonStyle(.borderedProminent)
            }
        }
        .padding(24)
        .frame(width: 460)
    }
}

struct ProjectGitRemoteEditorSheet: View {
    @Environment(\.dismiss) private var dismiss
    let remote: ProjectGitRemote?
    let onSave: (String?, String, String) -> Void
    let onRemove: ((String) -> Void)?

    @State private var name: String
    @State private var url: String
    @State private var confirmsRemoval = false

    init(
        remote: ProjectGitRemote?,
        onSave: @escaping (String?, String, String) -> Void,
        onRemove: ((String) -> Void)? = nil
    ) {
        self.remote = remote
        self.onSave = onSave
        self.onRemove = onRemove
        _name = State(initialValue: remote?.name ?? "origin")
        _url = State(initialValue: remote?.url ?? "")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 10) {
                Image(systemName: "network")
                    .appFont(.title2)
                    .foregroundStyle(AppPalette.ai)
                Text(remote == nil ? "添加远程仓库" : "编辑远程仓库")
                    .appFont(.title2.weight(.semibold))
            }

            VStack(alignment: .leading, spacing: 7) {
                Text("名称").appFont(.caption.weight(.semibold))
                TextField("origin", text: $name)
                    .textFieldStyle(.roundedBorder)
            }
            VStack(alignment: .leading, spacing: 7) {
                Text("仓库地址").appFont(.caption.weight(.semibold))
                TextField("https://… 或 git@…", text: $url)
                    .textFieldStyle(.roundedBorder)
            }

            HStack {
                if remote != nil, onRemove != nil {
                    Button("删除远程", role: .destructive) {
                        confirmsRemoval = true
                    }
                }
                Spacer()
                Button("取消") { dismiss() }
                Button("保存") {
                    let originalName = remote?.name
                    let cleanName = name.trimmingCharacters(in: .whitespacesAndNewlines)
                    let cleanURL = url.trimmingCharacters(in: .whitespacesAndNewlines)
                    dismiss()
                    onSave(originalName, cleanName, cleanURL)
                }
                .buttonStyle(.borderedProminent)
                .disabled(
                    name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        || url.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                )
            }
        }
        .padding(24)
        .frame(width: 500)
        .confirmationDialog(
            "删除远程仓库“\(remote?.name ?? "")”？",
            isPresented: $confirmsRemoval
        ) {
            Button("删除远程", role: .destructive) {
                guard let remote else { return }
                dismiss()
                onRemove?(remote.name)
            }
        } message: {
            Text("只会删除本地 Git 配置，不会删除远程服务器上的仓库或分支。")
        }
    }
}
