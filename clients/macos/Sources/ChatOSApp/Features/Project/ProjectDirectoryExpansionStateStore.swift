import Foundation

protocol ProjectDirectoryExpansionStateStoring {
    func loadExpandedPaths() -> Set<String>
    func saveExpandedPaths(_ paths: Set<String>)
}

struct ProjectDirectoryExpansionStateStore: ProjectDirectoryExpansionStateStoring {
    private let defaults: UserDefaults
    private let key: String
    private let rootPath: String?

    init(
        projectID: String,
        rootPath: String?,
        defaults: UserDefaults = .standard
    ) {
        self.defaults = defaults
        self.key = "ChatOS.projectDirectory.expandedPaths.\(projectID)"
        self.rootPath = rootPath
    }

    func loadExpandedPaths() -> Set<String> {
        let stored = Set(defaults.stringArray(forKey: key) ?? [])
        guard let rootPath = rootPath?.trimmingCharacters(in: .whitespacesAndNewlines),
              !rootPath.isEmpty else {
            return []
        }
        return stored.filter { path in
            path == rootPath || path.hasPrefix(rootPath + "/")
        }
    }

    func saveExpandedPaths(_ paths: Set<String>) {
        defaults.set(paths.sorted(), forKey: key)
    }
}
