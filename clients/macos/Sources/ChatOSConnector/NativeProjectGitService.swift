import ChatOSCore
import Foundation

public actor NativeProjectGitService: ProjectGitServicing {
    private let connector: NativeLocalConnectorService

    public init(connector: NativeLocalConnectorService) {
        self.connector = connector
    }

    public func snapshot(projectRoot: String) async throws -> ProjectGitSnapshot {
        let context = try await projectContext(projectRoot)
        guard let repository = try await repositoryContext(context) else {
            return .unavailable
        }

        let headVerification = try await run(
            ["rev-parse", "--verify", "HEAD"],
            in: repository.root,
            allowedExitCodes: [0, 128]
        )
        let hasHead = headVerification.exitCode == 0
        let branchOutput = try await run(
            ["symbolic-ref", "--quiet", "--short", "HEAD"],
            in: repository.root,
            allowedExitCodes: [0, 1]
        )
        let currentBranch = branchOutput.exitCode == 0
            ? branchOutput.stdoutString.trimmedNonEmpty
            : nil
        let detachedCommit: String?
        if currentBranch == nil, hasHead {
            detachedCommit = try await run(
                ["rev-parse", "--short", "HEAD"],
                in: repository.root
            ).stdoutString.trimmedNonEmpty
        } else {
            detachedCommit = nil
        }

        let statusOutput = try await run(
            ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            in: repository.root
        )
        let branchesOutput = try await run(
            [
                "for-each-ref",
                "--sort=-committerdate",
                "--format=%(refname:short)%00%(HEAD)%00%(upstream:short)",
                "refs/heads",
            ],
            in: repository.root
        )
        let remoteNames = try await run(["remote"], in: repository.root).stdoutString
            .split(whereSeparator: { $0.isNewline })
            .map(String.init)
        var remotes: [ProjectGitRemote] = []
        for name in remoteNames {
            let remoteURL = try await run(
                ["remote", "get-url", name],
                in: repository.root,
                allowedExitCodes: [0, 2, 128]
            ).stdoutString.trimmedNonEmpty ?? ""
            remotes.append(ProjectGitRemote(name: name, url: remoteURL))
        }

        var upstream: String?
        var ahead = 0
        var behind = 0
        if currentBranch != nil {
            let upstreamOutput = try await run(
                ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
                in: repository.root,
                allowedExitCodes: [0, 128]
            )
            upstream = upstreamOutput.exitCode == 0
                ? upstreamOutput.stdoutString.trimmedNonEmpty
                : nil
            if upstream != nil {
                let counts = try await run(
                    ["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
                    in: repository.root
                ).stdoutString
                    .split(whereSeparator: { $0 == "\t" || $0 == " " || $0 == "\n" })
                    .compactMap { Int($0) }
                if counts.count == 2 {
                    ahead = counts[0]
                    behind = counts[1]
                }
            }
        }

        let commits: [ProjectGitCommit]
        if hasHead {
            let logOutput = try await run(
                [
                    "log", "--all", "-n", "80", "--date=iso-strict",
                    "--pretty=format:%H%x00%h%x00%P%x00%an%x00%aI%x00%D%x00%s%x1e",
                ],
                in: repository.root
            )
            commits = NativeProjectGitParser.parseCommits(logOutput.stdout)
        } else {
            commits = []
        }

        return ProjectGitSnapshot(
            isRepository: true,
            repositoryRoot: repository.root.path,
            currentBranch: currentBranch,
            detachedCommit: detachedCommit,
            upstream: upstream,
            aheadCount: ahead,
            behindCount: behind,
            hasHead: hasHead,
            changes: NativeProjectGitParser.parseChanges(
                statusOutput.stdout,
                repositoryRoot: repository.root
            ),
            branches: NativeProjectGitParser.parseBranches(branchesOutput.stdoutString),
            commits: commits,
            remotes: remotes
        )
    }

    public func initializeRepository(projectRoot: String) async throws {
        let context = try await projectContext(projectRoot)
        _ = try await run(["init", "-b", "main"], in: context.projectRoot)
    }

    public func diff(
        projectRoot: String,
        change: ProjectGitChange,
        staged: Bool
    ) async throws -> ProjectGitDiff {
        let context = try await requiredRepository(projectRoot)
        try validate(change: change, repositoryRoot: context.root)
        let output: NativeGitProcessOutput
        if change.kind == .untracked, !staged {
            output = try await run(
                ["diff", "--no-index", "--no-color", "--", "/dev/null", change.absolutePath],
                in: context.root,
                allowedExitCodes: [0, 1]
            )
        } else {
            var arguments = ["diff", "--no-ext-diff", "--no-color"]
            if staged { arguments.append("--cached") }
            arguments += ["--", change.path]
            output = try await run(arguments, in: context.root)
        }
        return ProjectGitDiff(path: change.path, isStaged: staged, content: output.stdoutString)
    }

    public func stage(projectRoot: String, paths: [String]) async throws {
        guard !paths.isEmpty else { return }
        let repository = try await requiredRepository(projectRoot)
        try validate(paths: paths, repositoryRoot: repository.root)
        _ = try await run(["add", "--"] + paths, in: repository.root)
    }

    public func unstage(projectRoot: String, paths: [String]) async throws {
        guard !paths.isEmpty else { return }
        let repository = try await requiredRepository(projectRoot)
        try validate(paths: paths, repositoryRoot: repository.root)
        let hasHead = try await run(
            ["rev-parse", "--verify", "HEAD"],
            in: repository.root,
            allowedExitCodes: [0, 128]
        ).exitCode == 0
        if hasHead {
            _ = try await run(["restore", "--staged", "--"] + paths, in: repository.root)
        } else {
            _ = try await run(["rm", "--cached", "--"] + paths, in: repository.root)
        }
    }

    public func commit(projectRoot: String, message: String) async throws {
        let cleanMessage = message.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanMessage.isEmpty else { throw NativeGitError.emptyCommitMessage }
        let repository = try await requiredRepository(projectRoot)
        _ = try await run(["commit", "-m", cleanMessage], in: repository.root)
    }

    public func switchBranch(projectRoot: String, branch: String) async throws {
        let repository = try await requiredRepository(projectRoot)
        try await validate(branch: branch, in: repository.root)
        _ = try await run(["switch", branch], in: repository.root)
    }

    public func createBranch(
        projectRoot: String,
        name: String,
        switchToBranch: Bool
    ) async throws {
        let repository = try await requiredRepository(projectRoot)
        try await validate(branch: name, in: repository.root)
        let arguments = switchToBranch ? ["switch", "-c", name] : ["branch", name]
        _ = try await run(arguments, in: repository.root)
    }

    public func mergeBranch(projectRoot: String, branch: String) async throws {
        let repository = try await requiredRepository(projectRoot)
        try await validate(branch: branch, in: repository.root)
        _ = try await run(["merge", "--no-edit", branch], in: repository.root)
    }

    public func saveRemote(
        projectRoot: String,
        originalName: String?,
        name: String,
        url: String
    ) async throws {
        let cleanName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let cleanURL = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanName.isEmpty, !cleanURL.isEmpty else { throw NativeGitError.invalidRemote }
        let repository = try await requiredRepository(projectRoot)
        if let originalName {
            if originalName != cleanName {
                _ = try await run(
                    ["remote", "rename", originalName, cleanName],
                    in: repository.root
                )
            }
            _ = try await run(
                ["remote", "set-url", cleanName, cleanURL],
                in: repository.root
            )
        } else {
            _ = try await run(
                ["remote", "add", cleanName, cleanURL],
                in: repository.root
            )
        }
    }

    public func removeRemote(projectRoot: String, name: String) async throws {
        let repository = try await requiredRepository(projectRoot)
        _ = try await run(["remote", "remove", name], in: repository.root)
    }

    public func pull(projectRoot: String) async throws {
        let repository = try await requiredRepository(projectRoot)
        _ = try await run(["pull", "--ff-only"], in: repository.root)
    }

    public func push(projectRoot: String) async throws {
        let snapshot = try await snapshot(projectRoot: projectRoot)
        guard let repositoryPath = snapshot.repositoryRoot else { throw NativeGitError.notRepository }
        let repository = URL(fileURLWithPath: repositoryPath)
        if snapshot.upstream != nil {
            _ = try await run(["push"], in: repository)
            return
        }
        guard let branch = snapshot.currentBranch else { throw NativeGitError.noCurrentBranch }
        guard let remote = snapshot.remotes.first else { throw NativeGitError.noRemote }
        _ = try await run(["push", "--set-upstream", remote.name, branch], in: repository)
    }

    private func projectContext(_ projectRoot: String) async throws -> ProjectContext {
        let resolved = try await connector.resolveProjectPath(projectRoot)
        return ProjectContext(
            projectRoot: resolved.absoluteURL,
            workspaceRoot: URL(fileURLWithPath: resolved.workspace.absoluteRoot)
                .standardizedFileURL
                .resolvingSymlinksInPath()
        )
    }

    private func requiredRepository(_ projectRoot: String) async throws -> RepositoryContext {
        let project = try await projectContext(projectRoot)
        guard let repository = try await repositoryContext(project) else {
            throw NativeGitError.notRepository
        }
        return repository
    }

    private func repositoryContext(_ context: ProjectContext) async throws -> RepositoryContext? {
        let output = try await run(
            ["rev-parse", "--show-toplevel"],
            in: context.projectRoot,
            allowedExitCodes: [0, 128]
        )
        guard output.exitCode == 0,
              let path = output.stdoutString.trimmedNonEmpty else { return nil }
        let root = URL(fileURLWithPath: path).standardizedFileURL.resolvingSymlinksInPath()
        guard Self.isInside(root, parent: context.workspaceRoot) else {
            throw NativeGitError.repositoryOutsideWorkspace
        }
        return RepositoryContext(root: root)
    }

    private func run(
        _ arguments: [String],
        in directory: URL,
        allowedExitCodes: Set<Int32> = [0]
    ) async throws -> NativeGitProcessOutput {
        try await Task.detached {
            try NativeGitProcess.run(
                arguments: arguments,
                directory: directory,
                allowedExitCodes: allowedExitCodes
            )
        }.value
    }

    private func validate(branch: String, in repository: URL) async throws {
        let value = branch.trimmingCharacters(in: .whitespacesAndNewlines)
        guard value == branch, !value.isEmpty else { throw NativeGitError.invalidBranchName }
        let output = try await run(
            ["check-ref-format", "--branch", branch],
            in: repository,
            allowedExitCodes: [0, 1, 128]
        )
        guard output.exitCode == 0 else { throw NativeGitError.invalidBranchName }
    }

    private func validate(paths: [String], repositoryRoot: URL) throws {
        for path in paths {
            let absolute = repositoryRoot.appendingPathComponent(path).standardizedFileURL
            guard Self.isInside(absolute, parent: repositoryRoot) else {
                throw NativeGitError.repositoryOutsideWorkspace
            }
        }
    }

    private func validate(change: ProjectGitChange, repositoryRoot: URL) throws {
        try validate(paths: [change.path], repositoryRoot: repositoryRoot)
        let expected = repositoryRoot.appendingPathComponent(change.path).standardizedFileURL.path
        guard expected == URL(fileURLWithPath: change.absolutePath).standardizedFileURL.path else {
            throw NativeGitError.repositoryOutsideWorkspace
        }
    }

    private static func isInside(_ child: URL, parent: URL) -> Bool {
        let childPath = child.standardizedFileURL.path
        let parentPath = parent.standardizedFileURL.path
        let prefix = parentPath.hasSuffix("/") ? parentPath : parentPath + "/"
        return childPath == parentPath || childPath.hasPrefix(prefix)
    }

}

private struct ProjectContext: Sendable {
    var projectRoot: URL
    var workspaceRoot: URL
}

private struct RepositoryContext: Sendable {
    var root: URL
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
