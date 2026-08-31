import ChatOSCore
import Foundation

@MainActor
final class CreateProjectViewModel: ObservableObject {
    @Published private(set) var entries: [ProjectFileEntry] = []
    @Published private(set) var currentPath: String = ""
    @Published private(set) var currentRelativePath: String?
    @Published private(set) var parentPath: String?
    @Published private(set) var isLoadingDirectory = false
    @Published private(set) var isSaving = false
    @Published private(set) var pendingCreatedProject: WorkspaceProject?
    @Published private(set) var showsHiddenDirectories = false
    private(set) var selectedWorkspaceID: String
    @Published var projectName = ""
    @Published var errorMessage: String?

    let deviceID: String?
    let workspaces: [LocalConnectorWorkspace]

    private let defaultContact: WorkspaceContact?
    private let filesystemService: any ProjectFilesystemServicing
    private let creationService: any WorkspaceResourceCreating
    private var userEditedProjectName = false
    private var allDirectoryEntries: [ProjectFileEntry] = []

    init(
        connectorStatus: LocalConnectorStatus?,
        defaultContact: WorkspaceContact?,
        filesystemService: any ProjectFilesystemServicing,
        creationService: any WorkspaceResourceCreating
    ) {
        deviceID = connectorStatus?.deviceID
        workspaces = connectorStatus?.workspaces ?? []
        self.defaultContact = defaultContact
        self.filesystemService = filesystemService
        self.creationService = creationService
        selectedWorkspaceID = connectorStatus?.defaultWorkspaceID
            ?? connectorStatus?.workspaces.first?.id
            ?? ""
    }

    var selectedWorkspace: LocalConnectorWorkspace? {
        workspaces.first(where: { $0.id == selectedWorkspaceID })
    }

    var hasDefaultContact: Bool { defaultContact != nil }

    var canCreate: Bool {
        !normalizedProjectName.isEmpty
            && deviceID != nil
            && selectedWorkspace != nil
            && defaultContact != nil
            && !isLoadingDirectory
            && !isSaving
    }

    var saveButtonTitle: String {
        pendingCreatedProject == nil ? "创建项目" : "重试绑定"
    }

    var displayedLocation: String {
        guard let workspace = selectedWorkspace else { return "没有可用工作区" }
        guard let currentRelativePath else { return workspace.alias }
        return workspace.alias + "/" + currentRelativePath
    }

    func loadInitialDirectory() async {
        guard currentPath.isEmpty else { return }
        await openInitialDirectory()
    }

    func openWorkspaceRoot() async {
        guard let root = workspaceRootPath else {
            resetDirectory()
            return
        }
        await loadDirectory(path: root, relativePath: nil)
    }

    func openDirectory(_ entry: ProjectFileEntry) async {
        guard entry.isDirectory else { return }
        await loadDirectory(
            path: entry.path,
            relativePath: relativePath(for: entry.path)
        )
    }

    func goToParentDirectory() async {
        guard let parentPath else { return }
        await loadDirectory(
            path: parentPath,
            relativePath: relativePath(for: parentPath)
        )
    }

    func refreshDirectory() async {
        guard !currentPath.isEmpty else { return }
        await loadDirectory(path: currentPath, relativePath: currentRelativePath, forceRefresh: true)
    }

    func toggleHiddenDirectories() {
        showsHiddenDirectories.toggle()
        applyDirectoryFilter()
    }

    func createDirectory(named rawName: String) async {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, !currentPath.isEmpty else { return }
        errorMessage = nil
        isLoadingDirectory = true
        do {
            try await filesystemService.createDirectory(parentPath: currentPath, name: name)
            try await reloadCurrentDirectory()
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoadingDirectory = false
    }

    func updateProjectName(_ value: String) {
        projectName = value
        userEditedProjectName = true
    }

    func save() async -> WorkspaceProject? {
        guard let deviceID else {
            errorMessage = "本机还没有连接到 ChatOS 网关。"
            return nil
        }
        guard let workspace = selectedWorkspace else {
            errorMessage = "没有可用于创建项目的本机工作区。"
            return nil
        }
        guard let defaultContact else {
            errorMessage = "没有找到默认联系人“叽咕狸”，请先刷新资源或检查账号初始化状态。"
            return nil
        }
        guard !normalizedProjectName.isEmpty else {
            errorMessage = "请输入项目名称。"
            return nil
        }

        isSaving = true
        errorMessage = nil
        do {
            var project: WorkspaceProject
            if let pendingCreatedProject {
                project = pendingCreatedProject
            } else {
                project = try await creationService.createLocalProject(
                    LocalProjectCreationDraft(
                        name: normalizedProjectName,
                        deviceID: deviceID,
                        workspaceID: workspace.id,
                        relativePath: currentRelativePath
                    )
                )
                pendingCreatedProject = project
            }
            try await creationService.bindContact(
                projectID: project.id,
                contactID: defaultContact.id
            )
            project.latestConversationID = try await creationService.ensureConversation(
                project: project,
                contact: defaultContact
            )
            pendingCreatedProject = nil
            isSaving = false
            return project
        } catch {
            if pendingCreatedProject != nil {
                errorMessage = "项目已经创建，但准备默认联系人“叽咕狸”的会话失败：\(error.localizedDescription)\n请点击“重试”，不会重复创建项目。"
            } else {
                errorMessage = error.localizedDescription
            }
            isSaving = false
            return nil
        }
    }

    private var normalizedProjectName: String {
        projectName.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var workspaceRootPath: String? {
        guard let deviceID, let workspace = selectedWorkspace else { return nil }
        return "local://connector/\(deviceID)/\(workspace.id)"
    }

    private func loadDirectory(
        path: String,
        relativePath: String?,
        forceRefresh: Bool = false
    ) async {
        isLoadingDirectory = true
        errorMessage = nil
        do {
            let listing = try await filesystemService.listEntries(
                path: path,
                forceRefresh: forceRefresh
            )
            currentPath = listing.path
            currentRelativePath = relativePath
            parentPath = listing.parentPath
            allDirectoryEntries = listing.entries.filter(\.isDirectory)
            applyDirectoryFilter()
            applySuggestedProjectName()
        } catch {
            errorMessage = error.localizedDescription
            allDirectoryEntries = []
            entries = []
        }
        isLoadingDirectory = false
    }

    private func reloadCurrentDirectory() async throws {
        let listing = try await filesystemService.listEntries(path: currentPath, forceRefresh: true)
        parentPath = listing.parentPath
        allDirectoryEntries = listing.entries.filter(\.isDirectory)
        applyDirectoryFilter()
    }

    private func applySuggestedProjectName() {
        guard !userEditedProjectName else { return }
        if let currentRelativePath,
           let name = currentRelativePath.split(separator: "/").last {
            projectName = String(name)
        } else {
            projectName = selectedWorkspace?.alias ?? ""
        }
    }

    private func relativePath(for logicalPath: String) -> String? {
        guard let root = workspaceRootPath else { return nil }
        if logicalPath == root { return nil }
        let prefix = root + "/"
        guard logicalPath.hasPrefix(prefix) else { return currentRelativePath }
        let value = String(logicalPath.dropFirst(prefix.count))
        return value.isEmpty || value == "." ? nil : value
    }

    private func resetDirectory() {
        currentPath = ""
        currentRelativePath = nil
        parentPath = nil
        allDirectoryEntries = []
        entries = []
    }

    private func openInitialDirectory() async {
        guard let root = workspaceRootPath else {
            resetDirectory()
            return
        }
        guard let relativeHomePath else {
            await loadDirectory(path: root, relativePath: nil)
            return
        }
        await loadDirectory(path: root + "/" + relativeHomePath, relativePath: relativeHomePath)
    }

    private var relativeHomePath: String? {
        guard let workspace = selectedWorkspace else { return nil }
        let root = URL(fileURLWithPath: workspace.absoluteRoot).standardizedFileURL.path
        let home = FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
        let prefix = root == "/" ? "/" : root + "/"
        guard home != root, home.hasPrefix(prefix) else { return nil }
        return String(home.dropFirst(prefix.count)).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }

    private func applyDirectoryFilter() {
        guard !showsHiddenDirectories else {
            entries = allDirectoryEntries
            return
        }
        entries = allDirectoryEntries.filter { entry in
            guard !entry.name.hasPrefix(".") else { return false }
            if isSystemRootDirectory, Self.systemRootDirectoryNames.contains(entry.name) {
                return false
            }
            if isUserHomeDirectory, Self.userSystemDirectoryNames.contains(entry.name) {
                return false
            }
            return true
        }
    }

    private var isSystemRootDirectory: Bool {
        selectedWorkspace?.absoluteRoot == "/" && currentRelativePath == nil
    }

    private var isUserHomeDirectory: Bool {
        currentRelativePath == relativeHomePath
    }

    private static let systemRootDirectoryNames: Set<String> = [
        "Applications", "Library", "System", "bin", "cores", "dev", "etc",
        "home", "net", "opt", "private", "sbin", "tmp", "usr", "var",
    ]

    private static let userSystemDirectoryNames: Set<String> = [
        "Applications", "Applications (Parallels)", "Library", "bin", "opt",
    ]
}
