import SwiftUI

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
