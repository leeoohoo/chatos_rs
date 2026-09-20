import AppKit
import ChatOSConnector
import ChatOSCore
import CryptoKit
import SwiftUI

struct AgentRemoteArtifactLibraryView: View {
    let service: NativeAgentGroupChatService

    @State private var artifacts: [AgentArtifactRemoteItem] = []
    @State private var nextCursor: String?
    @State private var isLoading = false
    @State private var loadingArtifactIDs: Set<String> = []
    @State private var previewedDocument: AgentMarkdownPreviewItem?
    @State private var errorMessage: String?

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            content
        }
        .task {
            guard artifacts.isEmpty else { return }
            await load(reset: true)
        }
        .sheet(item: $previewedDocument) { item in
            AgentMarkdownAttachmentPreview(item: item)
        }
        .alert("云端文档操作失败", isPresented: Binding(
            get: { errorMessage != nil },
            set: { if !$0 { errorMessage = nil } }
        )) {
            Button("好", role: .cancel) { errorMessage = nil }
        } message: {
            Text(errorMessage ?? "")
        }
    }

    private var header: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text("云端 Agent 文档")
                    .font(.title3.weight(.semibold))
                Text("显示当前账户已完成同步的 Markdown；用于其他设备发现和预览，不代表本地消息记录已跨设备同步。")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button("刷新", systemImage: "arrow.clockwise") {
                Task { await load(reset: true) }
            }
            .disabled(isLoading)
        }
        .padding(16)
    }

    @ViewBuilder
    private var content: some View {
        if isLoading, artifacts.isEmpty {
            ProgressView("正在读取云端文档…")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if artifacts.isEmpty {
            ContentUnavailableView {
                Label("还没有云端 Agent 文档", systemImage: "doc.text.magnifyingglass")
            } description: {
                Text("Agent 创建并成功同步的 Markdown 会出现在这里。")
            }
        } else {
            List {
                ForEach(artifacts) { artifact in
                    artifactRow(artifact)
                }
                if nextCursor != nil {
                    HStack {
                        Spacer()
                        Button(isLoading ? "正在加载…" : "加载更多") {
                            Task { await load(reset: false) }
                        }
                        .disabled(isLoading)
                        Spacer()
                    }
                    .listRowSeparator(.hidden)
                }
            }
            .listStyle(.inset)
        }
    }

    private func artifactRow(_ artifact: AgentArtifactRemoteItem) -> some View {
        HStack(spacing: 12) {
            Image(systemName: "doc.text")
                .font(.title3)
                .foregroundStyle(AppPalette.ai)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 3) {
                Text(artifact.name)
                    .font(.body.weight(.medium))
                    .lineLimit(1)
                Text("\(formattedSize(artifact.size)) · \(formattedDate(artifact.createdAtUnixMs))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("SHA-256 \(artifact.sha256.prefix(12))…")
                    .font(.caption2.monospaced())
                    .foregroundStyle(.tertiary)
            }
            Spacer()
            if loadingArtifactIDs.contains(artifact.id) {
                ProgressView().controlSize(.small)
            }
            Button("预览", systemImage: "eye") {
                preview(artifact)
            }
            .disabled(loadingArtifactIDs.contains(artifact.id))
            Button("另存为", systemImage: "square.and.arrow.down") {
                save(artifact)
            }
            .labelStyle(.iconOnly)
            .help("另存为…")
            .disabled(loadingArtifactIDs.contains(artifact.id))
        }
        .padding(.vertical, 5)
    }

    private func load(reset: Bool) async {
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let page = try await service.remoteAgentArtifacts(
                limit: 50,
                cursor: reset ? nil : nextCursor
            )
            if reset {
                artifacts = page.artifacts
            } else {
                let existing = Set(artifacts.map(\.id))
                artifacts.append(contentsOf: page.artifacts.filter { !existing.contains($0.id) })
            }
            nextCursor = page.nextCursor
        } catch is CancellationError {
            return
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func preview(_ artifact: AgentArtifactRemoteItem) {
        guard loadingArtifactIDs.insert(artifact.id).inserted else { return }
        Task {
            defer { loadingArtifactIDs.remove(artifact.id) }
            do {
                let data = try await validatedData(artifact)
                guard let markdown = String(data: data, encoding: .utf8) else {
                    throw AgentRemoteArtifactPresentationError.invalidUTF8
                }
                previewedDocument = .init(
                    id: artifact.id,
                    name: artifact.name,
                    size: artifact.size,
                    markdown: markdown
                )
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    private func save(_ artifact: AgentArtifactRemoteItem) {
        guard loadingArtifactIDs.insert(artifact.id).inserted else { return }
        Task {
            defer { loadingArtifactIDs.remove(artifact.id) }
            do {
                let data = try await validatedData(artifact)
                let panel = NSSavePanel()
                panel.nameFieldStringValue = artifact.name
                panel.canCreateDirectories = true
                guard panel.runModal() == .OK, let url = panel.url else { return }
                try data.write(to: url, options: .atomic)
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    private func validatedData(_ artifact: AgentArtifactRemoteItem) async throws -> Data {
        let data = try await service.remoteAgentArtifactData(artifactID: artifact.id)
        guard data.count == artifact.size,
              SHA256.hash(data: data).map({ String(format: "%02x", $0) }).joined()
                == artifact.sha256 else {
            throw AgentRemoteArtifactPresentationError.integrityMismatch
        }
        return data
    }

    private func formattedSize(_ bytes: Int) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: Int64(bytes))
    }

    private func formattedDate(_ unixMilliseconds: Int64) -> String {
        Date(timeIntervalSince1970: TimeInterval(unixMilliseconds) / 1_000)
            .formatted(date: .abbreviated, time: .shortened)
    }
}

private enum AgentRemoteArtifactPresentationError: LocalizedError {
    case integrityMismatch
    case invalidUTF8

    var errorDescription: String? {
        switch self {
        case .integrityMismatch: "云端文档的大小或 SHA-256 与元数据不一致。"
        case .invalidUTF8: "云端 Markdown 不是有效的 UTF-8 文本。"
        }
    }
}
