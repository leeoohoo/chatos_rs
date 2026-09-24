import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import SwiftUI

struct TeamAssetsView: View {
    let assets: [LocalAgentTeamAsset]
    let onCreate: () -> Void
    let onEdit: (LocalAgentTeamAsset) -> Void
    let onHistory: (LocalAgentTeamAsset) -> Void
    let onArchive: (LocalAgentTeamAsset) -> Void

    @State private var expandedAssetIDs: Set<String> = []
    @State private var page = 0
    @State private var pageSize = 20

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("团队共享资产").appFont(.headline)
                    Text("任务启动时会固定读取当时的 revision。")
                        .appFont(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button(action: onCreate) {
                    Label("新建资产", systemImage: "plus")
                }
                .buttonStyle(.borderedProminent)
            }
            .padding(16)
            Divider()
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    if assets.isEmpty {
                        ContentUnavailableView(
                            "还没有共享资产",
                            systemImage: "doc.richtext",
                            description: Text("在这里维护项目背景、进度、技术栈和架构决策。")
                        )
                        .padding(.top, 60)
                    }
                    ForEach(assets.agentPage(index: page, size: pageSize)) { asset in
                        assetCard(asset)
                    }
                    if !assets.isEmpty {
                        AgentListPaginationBar(
                            totalCount: assets.count,
                            page: $page,
                            pageSize: $pageSize
                        )
                    }
                }
                .padding(18)
            }
        }
    }

    private func assetCard(_ asset: LocalAgentTeamAsset) -> some View {
        let isExpanded = expandedAssetIDs.contains(asset.id)

        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                Button {
                    withAnimation(.easeInOut(duration: 0.18)) {
                        if isExpanded {
                            expandedAssetIDs.remove(asset.id)
                        } else {
                            expandedAssetIDs.insert(asset.id)
                        }
                    }
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                            .appFont(.caption.weight(.bold))
                            .foregroundStyle(.secondary)
                            .frame(width: 14)

                        Text(asset.title)
                            .appFont(.headline)
                            .foregroundStyle(.primary)
                            .lineLimit(1)

                        Text(asset.category.displayName)
                            .appFont(.caption2.weight(.medium))
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(AppPalette.surfaceSubtle, in: Capsule())

                        Spacer(minLength: 12)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)

                Text("r\(asset.revision)")
                    .appFont(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)

                Menu {
                    Button("编辑") { onEdit(asset) }
                    Button("版本历史") { onHistory(asset) }
                    Button("归档", role: .destructive) { onArchive(asset) }
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
                .menuStyle(.borderlessButton)
            }
            .padding(.horizontal, 14)
            .frame(minHeight: 50)

            if isExpanded {
                Divider()
                MarkdownDocumentView(markdown: asset.markdown)
                    .padding(14)
            }
        }
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(AppPalette.border.opacity(0.75), lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.025), radius: 3, y: 1)
    }
}

struct TeamAssetHistorySheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: AgentGroupChatViewModel
    let asset: LocalAgentTeamAsset
    @State private var selectedRevision: Int?
    @State private var page = 0
    @State private var pageSize = 20

    private var revisions: [LocalAgentTeamAssetRevision] {
        viewModel.teamAssetRevisions[asset.id] ?? []
    }

    private var selected: LocalAgentTeamAssetRevision? {
        if let selectedRevision,
           let revision = revisions.first(where: { $0.revision == selectedRevision }) {
            return revision
        }
        return revisions.first
    }

    private var pagedRevisions: [LocalAgentTeamAssetRevision] {
        revisions.agentPage(index: page, size: pageSize)
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("版本历史").appFont(.title3.weight(.semibold))
                    Text(asset.title).appFont(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button("完成") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(16)
            Divider()
            if viewModel.loadingTeamAssetRevisionIDs.contains(asset.id), revisions.isEmpty {
                ProgressView("正在读取版本历史…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if revisions.isEmpty {
                ContentUnavailableView(
                    "没有版本记录",
                    systemImage: "clock.arrow.circlepath",
                    description: Text("保存资产后，历史版本会显示在这里。")
                )
            } else {
                HStack(spacing: 0) {
                    VStack(spacing: 0) {
                        List(pagedRevisions, selection: $selectedRevision) { revision in
                            VStack(alignment: .leading, spacing: 5) {
                                HStack {
                                    Text("r\(revision.revision)")
                                        .appFont(.body.monospacedDigit().weight(.semibold))
                                    if revision.revision == asset.revision {
                                        Text("当前")
                                            .appFont(.caption2.weight(.semibold))
                                            .padding(.horizontal, 6)
                                            .padding(.vertical, 2)
                                            .background(Color.accentColor.opacity(0.14), in: Capsule())
                                    }
                                }
                                Text(revision.title).appFont(.caption).lineLimit(2)
                                Text(Self.timestamp(revision.createdAtUnixMs))
                                    .appFont(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                            .padding(.vertical, 4)
                            .tag(Optional(revision.revision))
                        }
                        AgentListPaginationBar(
                            totalCount: revisions.count,
                            page: $page,
                            pageSize: $pageSize,
                            compact: true
                        )
                        .padding(10)
                    }
                    .frame(width: 270)
                    Divider()
                    if let selected {
                        VStack(alignment: .leading, spacing: 0) {
                            HStack(alignment: .firstTextBaseline) {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(selected.title).appFont(.headline)
                                    Text("r\(selected.revision) · \(editorName(selected)) · \(Self.timestamp(selected.createdAtUnixMs))")
                                        .appFont(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                            }
                            .padding(20)
                            Divider()
                            MarkdownReaderView(markdown: selected.markdown)
                                .padding(20)
                        }
                    }
                }
            }
        }
        .frame(minWidth: 920, minHeight: 620)
        .task {
            await viewModel.loadTeamAssetRevisions(asset)
            if selectedRevision == nil {
                selectedRevision = viewModel.teamAssetRevisions[asset.id]?.first?.revision
            }
        }
        .onChange(of: revisions.map(\.revision)) { _, values in
            if let selectedRevision, values.contains(selectedRevision) {
                return
            } else {
                selectedRevision = values.first
            }
        }
        .onChange(of: page) { _, _ in
            selectedRevision = pagedRevisions.first?.revision
        }
    }

    private func editorName(_ revision: LocalAgentTeamAssetRevision) -> String {
        guard let editorAgentID = revision.editorAgentID else { return "你" }
        return viewModel.profilesByID[editorAgentID]?.draft.name ?? "Agent"
    }

    private static func timestamp(_ unixMs: Int64) -> String {
        Date(timeIntervalSince1970: Double(unixMs) / 1_000)
            .formatted(date: .abbreviated, time: .shortened)
    }
}

struct TeamAssetEditorSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: AgentGroupChatViewModel
    let asset: LocalAgentTeamAsset?
    @State private var category: LocalAgentTeamAssetCategory
    @State private var title: String
    @State private var markdown: String
    @State private var isSaving = false

    init(viewModel: AgentGroupChatViewModel, asset: LocalAgentTeamAsset?) {
        self.viewModel = viewModel
        self.asset = asset
        _category = State(initialValue: asset?.category ?? .overview)
        _title = State(initialValue: asset?.title ?? "")
        _markdown = State(initialValue: asset?.markdown ?? "# 项目上下文\n\n")
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(asset == nil ? "新建共享资产" : "编辑共享资产").appFont(.title3.weight(.semibold))
                Spacer()
                Button("取消") { dismiss() }
                Button("保存") {
                    isSaving = true
                    Task {
                        if await viewModel.saveTeamAsset(
                            existing: asset,
                            category: category,
                            title: title,
                            markdown: markdown
                        ) { dismiss() }
                        isSaving = false
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isSaving)
            }
            .padding(16)
            Divider()
            HStack(spacing: 0) {
                VStack(alignment: .leading, spacing: 10) {
                    Picker("分类", selection: $category) {
                        ForEach(LocalAgentTeamAssetCategory.allCases, id: \.self) {
                            Text($0.displayName).tag($0)
                        }
                    }
                    TextField("标题", text: $title)
                    TextEditor(text: $markdown)
                        .font(.body.monospaced())
                        .scrollContentBackground(.hidden)
                        .padding(8)
                        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
                }
                .padding(16)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                Divider()
                MarkdownReaderView(markdown: markdown)
                    .padding(18)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(minWidth: 900, minHeight: 620)
    }
}
