import ChatOSCore
import ChatOSConnector
import SwiftUI
import UniformTypeIdentifiers

struct LocalProjectImportSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    let ownerUserID: String
    @State private var choosesFile = false
    @State private var sourceID: String?
    @State private var candidates: [LocalProjectImportCandidate] = []
    @State private var selected: Set<String> = []
    @State private var busy = false
    @State private var error: String?
    @State private var result: ProjectRegistryImportResult?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("导入本地项目清单").font(.title2)
            Text("保留原项目 ID，确认本机目录后一次性导入。不会连接旧项目服务、上传代码或覆盖已有项目。")
                .foregroundStyle(.secondary)
            Button("选择 JSON 清单…") { choosesFile = true }.disabled(busy)
            if busy { ProgressView() }
            List(candidates) { candidate in
                HStack {
                    Toggle(isOn: Binding(
                        get: { selected.contains(candidate.id) },
                        set: { if $0 { selected.insert(candidate.id) } else { selected.remove(candidate.id) } }
                    )) { Text(candidate.project.name) }
                    .disabled(candidate.record == nil || busy || result != nil)
                    VStack(alignment: .leading) {
                        Text(candidate.binding?.absolutePath ?? candidate.project.rootPath ?? "无目录")
                            .font(.caption).textSelection(.enabled)
                        if let error = candidate.error { Text(error).font(.caption).foregroundStyle(.orange) }
                    }
                }
            }
            if let error { Text(error).foregroundStyle(.orange).textSelection(.enabled) }
            if let result {
                Text("已导入 \(result.insertedIDs.count) 个；已有项目或删除记录跳过 \(result.skippedIDs.count) 个。")
            }
            HStack {
                Text("其他设备、目录缺失或越界的记录不会自动改绑。")
                    .font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button(result == nil ? "取消" : "完成") { dismiss() }.disabled(busy)
                if result == nil {
                    Button("确认导入 \(selected.count) 个项目") { Task { await importSelection() } }
                        .buttonStyle(.borderedProminent)
                        .disabled(busy || selected.isEmpty || sourceID == nil)
                }
            }
        }
        .padding(20).frame(width: 760, height: 500)
        .interactiveDismissDisabled(busy)
        .fileImporter(isPresented: $choosesFile, allowedContentTypes: [.json]) { response in
            Task { await load(response) }
        }
    }

    private func load(_ response: Result<URL, Error>) async {
        guard model.localProjectOwnerUserID == ownerUserID else { dismiss(); return }
        busy = true
        defer { busy = false }
        error = nil
        result = nil
        candidates = []
        selected = []
        sourceID = nil
        do {
            let url = try response.get()
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            let handle = try FileHandle(forReadingFrom: url)
            defer { try? handle.close() }
            let data = try handle.read(upToCount: 10 * 1_024 * 1_024 + 1) ?? Data()
            guard data.count <= 10 * 1_024 * 1_024 else { throw ProjectRegistryError.invalidField("文件大于 10 MiB") }
            let document = try JSONDecoder().decode(LocalProjectImportDocument.self, from: data)
            try document.validate(ownerUserID: ownerUserID)
            let preview = await model.localProjectsService.preview(ownerUserID: ownerUserID, projects: document.projects.map {
                WorkspaceProject(id: $0.id, name: $0.name, rootPath: $0.rootPath, latestConversationID: nil)
            })
            guard model.localProjectOwnerUserID == ownerUserID else { dismiss(); return }
            candidates = preview
            selected = Set(preview.filter { $0.record != nil }.map(\.id))
            sourceID = document.sourceId
        } catch { self.error = error.localizedDescription }
    }

    private func importSelection() async {
        guard let sourceID, model.localProjectOwnerUserID == ownerUserID else { return }
        busy = true
        defer { busy = false }
        error = nil
        do {
            result = try await model.localProjectsService.importConfirmed(ownerUserID: ownerUserID, sourceID: sourceID,
                candidates: candidates.filter { selected.contains($0.id) })
            guard model.localProjectOwnerUserID == ownerUserID else { dismiss(); return }
            model.refreshWorkspace()
        } catch { self.error = error.localizedDescription }
    }
}

struct RenameLocalProjectSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    let projectID: String
    @State private var name = ""
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("重命名本地项目").font(.headline)
            TextField("项目名称", text: $name)
            if let error { Text(error).foregroundStyle(.orange) }
            HStack {
                Spacer()
                Button("取消") { dismiss() }.disabled(busy)
                Button("保存") {
                    busy = true
                    Task {
                        defer { busy = false }
                        do {
                            try await model.renameLocalProject(id: projectID, name: name)
                            dismiss()
                        } catch { self.error = error.localizedDescription }
                    }
                }.disabled(busy || name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }.padding(20).frame(width: 420)
            .onAppear { name = model.workspaceProject(id: projectID)?.name ?? "" }
            .interactiveDismissDisabled(busy)
    }
}
