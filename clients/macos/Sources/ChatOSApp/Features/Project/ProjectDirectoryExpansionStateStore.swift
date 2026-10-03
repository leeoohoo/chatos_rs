import Foundation

enum ProjectDirectoryExpansionStatePolicy {
    static let maximumPersistedPaths = 512

    static func normalizedPaths(_ paths: Set<String>, rootPath: String?) -> Set<String> {
        guard let rawRootPath = rootPath?.trimmingCharacters(in: .whitespacesAndNewlines),
              !rawRootPath.isEmpty else { return [] }
        let rootPath = canonicalPath(rawRootPath)
        let descendants = Set(paths.lazy.map(canonicalPath)).filter {
            $0.hasPrefix(rootPath + "/")
        }
        let ordered = descendants.sorted { lhs, rhs in
            let lhsDepth = relativeDepth(of: lhs, rootPath: rootPath)
            let rhsDepth = relativeDepth(of: rhs, rootPath: rootPath)
            if lhsDepth != rhsDepth { return lhsDepth < rhsDepth }
            return lhs.localizedStandardCompare(rhs) == .orderedAscending
        }
        return Set(ordered.prefix(maximumPersistedPaths))
    }

    static func canonicalPath(_ path: String) -> String {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let components = URLComponents(string: trimmed),
              components.scheme?.lowercased() == "local",
              components.host?.lowercased() == "connector" else { return trimmed }
        return "local://connector" + components.path
    }

    private static func relativeDepth(of path: String, rootPath: String) -> Int {
        path.dropFirst(rootPath.count + 1).split(separator: "/").count
    }
}

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
        let normalized = ProjectDirectoryExpansionStatePolicy.normalizedPaths(
            stored,
            rootPath: rootPath
        )
        if normalized != stored {
            defaults.set(normalized.sorted(), forKey: key)
        }
        return normalized
    }

    func saveExpandedPaths(_ paths: Set<String>) {
        let normalized = ProjectDirectoryExpansionStatePolicy.normalizedPaths(
            paths,
            rootPath: rootPath
        )
        defaults.set(normalized.sorted(), forKey: key)
    }
}
