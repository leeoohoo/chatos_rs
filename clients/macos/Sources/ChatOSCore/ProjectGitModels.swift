import Foundation

public enum ProjectGitChangeKind: String, Sendable, Equatable {
    case modified
    case added
    case deleted
    case renamed
    case copied
    case untracked
    case conflicted
    case typeChanged
}

public struct ProjectGitChange: Sendable, Equatable, Hashable, Identifiable {
    public var id: String { "\(path):\(indexStatus):\(workTreeStatus)" }
    public var path: String
    public var originalPath: String?
    public var absolutePath: String
    public var indexStatus: String
    public var workTreeStatus: String
    public var kind: ProjectGitChangeKind

    public init(
        path: String,
        originalPath: String? = nil,
        absolutePath: String,
        indexStatus: String,
        workTreeStatus: String,
        kind: ProjectGitChangeKind
    ) {
        self.path = path
        self.originalPath = originalPath
        self.absolutePath = absolutePath
        self.indexStatus = indexStatus
        self.workTreeStatus = workTreeStatus
        self.kind = kind
    }

    public var hasStagedChanges: Bool {
        indexStatus != " " && indexStatus != "?"
    }

    public var hasWorkingTreeChanges: Bool {
        workTreeStatus != " " || (indexStatus == "?" && workTreeStatus == "?")
    }
}

public struct ProjectGitBranch: Sendable, Equatable, Hashable, Identifiable {
    public var id: String { name }
    public var name: String
    public var isCurrent: Bool
    public var upstream: String?

    public init(name: String, isCurrent: Bool, upstream: String? = nil) {
        self.name = name
        self.isCurrent = isCurrent
        self.upstream = upstream
    }
}

public struct ProjectGitCommit: Sendable, Equatable, Hashable, Identifiable {
    public var id: String
    public var shortID: String
    public var parentIDs: [String]
    public var author: String
    public var authoredAt: Date?
    public var decorations: [String]
    public var subject: String

    public init(
        id: String,
        shortID: String,
        parentIDs: [String],
        author: String,
        authoredAt: Date?,
        decorations: [String],
        subject: String
    ) {
        self.id = id
        self.shortID = shortID
        self.parentIDs = parentIDs
        self.author = author
        self.authoredAt = authoredAt
        self.decorations = decorations
        self.subject = subject
    }

    public var isMerge: Bool { parentIDs.count > 1 }
}

public struct ProjectGitRemote: Sendable, Equatable, Hashable, Identifiable {
    public var id: String { name }
    public var name: String
    public var url: String

    public init(name: String, url: String) {
        self.name = name
        self.url = url
    }
}

public struct ProjectGitSnapshot: Sendable, Equatable {
    public var isRepository: Bool
    public var repositoryRoot: String?
    public var currentBranch: String?
    public var detachedCommit: String?
    public var upstream: String?
    public var aheadCount: Int
    public var behindCount: Int
    public var hasHead: Bool
    public var changes: [ProjectGitChange]
    public var branches: [ProjectGitBranch]
    public var commits: [ProjectGitCommit]
    public var remotes: [ProjectGitRemote]

    public init(
        isRepository: Bool,
        repositoryRoot: String? = nil,
        currentBranch: String? = nil,
        detachedCommit: String? = nil,
        upstream: String? = nil,
        aheadCount: Int = 0,
        behindCount: Int = 0,
        hasHead: Bool = false,
        changes: [ProjectGitChange] = [],
        branches: [ProjectGitBranch] = [],
        commits: [ProjectGitCommit] = [],
        remotes: [ProjectGitRemote] = []
    ) {
        self.isRepository = isRepository
        self.repositoryRoot = repositoryRoot
        self.currentBranch = currentBranch
        self.detachedCommit = detachedCommit
        self.upstream = upstream
        self.aheadCount = aheadCount
        self.behindCount = behindCount
        self.hasHead = hasHead
        self.changes = changes
        self.branches = branches
        self.commits = commits
        self.remotes = remotes
    }

    public static let unavailable = ProjectGitSnapshot(isRepository: false)
}

public struct ProjectGitDiff: Sendable, Equatable, Identifiable {
    public var id: String { "\(path):\(isStaged)" }
    public var path: String
    public var isStaged: Bool
    public var content: String

    public init(path: String, isStaged: Bool, content: String) {
        self.path = path
        self.isStaged = isStaged
        self.content = content
    }
}

public protocol ProjectGitServicing: Sendable {
    func snapshot(projectRoot: String) async throws -> ProjectGitSnapshot
    func initializeRepository(projectRoot: String) async throws
    func diff(projectRoot: String, change: ProjectGitChange, staged: Bool) async throws -> ProjectGitDiff
    func stage(projectRoot: String, paths: [String]) async throws
    func unstage(projectRoot: String, paths: [String]) async throws
    func commit(projectRoot: String, message: String) async throws
    func switchBranch(projectRoot: String, branch: String) async throws
    func createBranch(projectRoot: String, name: String, switchToBranch: Bool) async throws
    func mergeBranch(projectRoot: String, branch: String) async throws
    func saveRemote(
        projectRoot: String,
        originalName: String?,
        name: String,
        url: String
    ) async throws
    func removeRemote(projectRoot: String, name: String) async throws
    func pull(projectRoot: String) async throws
    func push(projectRoot: String) async throws
}
