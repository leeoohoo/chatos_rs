import ChatOSCore
import Foundation

@MainActor
final class CreateProjectViewModel: ObservableObject {
    @Published private(set) var selectedDirectoryPath: String?
    @Published private(set) var isSaving = false
    @Published var projectName = ""
    @Published var errorMessage: String?

    private let creationService: any LocalProjectCreating
    private var userEditedProjectName = false

    init(creationService: any LocalProjectCreating) {
        self.creationService = creationService
    }

    var canCreate: Bool {
        !normalizedProjectName.isEmpty && selectedDirectoryPath != nil && !isSaving
    }

    func selectDirectory(_ url: URL) {
        let resolved = url.standardizedFileURL.resolvingSymlinksInPath()
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: resolved.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            errorMessage = "请选择一个存在的本机文件夹。"
            return
        }
        selectedDirectoryPath = resolved.path
        errorMessage = nil
        guard !userEditedProjectName else { return }
        projectName = resolved.lastPathComponent.isEmpty ? resolved.path : resolved.lastPathComponent
    }

    func updateProjectName(_ value: String) {
        projectName = value
        userEditedProjectName = true
    }

    func save() async -> WorkspaceProject? {
        guard !isSaving else { return nil }
        guard let selectedDirectoryPath else {
            errorMessage = "请选择本机项目目录。"
            return nil
        }
        let draft = LocalProjectDraft(name: normalizedProjectName, rootPath: selectedDirectoryPath)
        isSaving = true
        defer { isSaving = false }
        errorMessage = nil
        do {
            try draft.validate()
            return try await creationService.createProject(draft)
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    private var normalizedProjectName: String {
        projectName.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
