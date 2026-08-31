import ChatOSCore
import Foundation

@MainActor
final class ProjectGitViewModel: ObservableObject {
    @Published private(set) var snapshot: ProjectGitSnapshot = .unavailable
    @Published private(set) var selectedDiff: ProjectGitDiff?
    @Published private(set) var isLoading = false
    @Published private(set) var isPerformingOperation = false
    @Published private(set) var errorMessage: String?
    @Published private(set) var worktreeRevision = 0
    @Published var commitMessage = ""

    let projectRoot: String?
    private let service: any ProjectGitServicing

    init(projectRoot: String?, service: any ProjectGitServicing) {
        self.projectRoot = projectRoot
        self.service = service
    }

    var stagedChanges: [ProjectGitChange] {
        snapshot.changes.filter(\.hasStagedChanges)
    }

    var workingTreeChanges: [ProjectGitChange] {
        snapshot.changes.filter(\.hasWorkingTreeChanges)
    }

    var changeCount: Int { snapshot.changes.count }

    func load() async {
        guard let projectRoot else {
            snapshot = .unavailable
            return
        }
        isLoading = true
        errorMessage = nil
        do {
            snapshot = try await service.snapshot(projectRoot: projectRoot)
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    func initializeRepository() async {
        await mutate {
            try await service.initializeRepository(projectRoot: try requiredProjectRoot())
        }
    }

    func showDiff(for change: ProjectGitChange, staged: Bool) async {
        guard let projectRoot else { return }
        errorMessage = nil
        do {
            selectedDiff = try await service.diff(
                projectRoot: projectRoot,
                change: change,
                staged: staged
            )
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func clearDiff() {
        selectedDiff = nil
    }

    func stage(_ change: ProjectGitChange) async {
        await stage(paths: [change.path])
    }

    func stageAll() async {
        await stage(paths: workingTreeChanges.map(\.path))
    }

    func unstage(_ change: ProjectGitChange) async {
        await unstage(paths: [change.path])
    }

    func unstageAll() async {
        await unstage(paths: stagedChanges.map(\.path))
    }

    func commit() async {
        let message = commitMessage
        await mutate {
            try await service.commit(
                projectRoot: try requiredProjectRoot(),
                message: message
            )
        } onSuccess: {
            commitMessage = ""
        }
    }

    func switchBranch(_ branch: String) async {
        guard branch != snapshot.currentBranch else { return }
        await mutate(updatesWorkingTree: true) {
            try await service.switchBranch(
                projectRoot: try requiredProjectRoot(),
                branch: branch
            )
        }
    }

    func createBranch(name: String, switchToBranch: Bool) async {
        await mutate(updatesWorkingTree: switchToBranch) {
            try await service.createBranch(
                projectRoot: try requiredProjectRoot(),
                name: name,
                switchToBranch: switchToBranch
            )
        }
    }

    func mergeBranch(_ branch: String) async {
        await mutate(updatesWorkingTree: true) {
            try await service.mergeBranch(
                projectRoot: try requiredProjectRoot(),
                branch: branch
            )
        }
    }

    func saveRemote(originalName: String?, name: String, url: String) async {
        await mutate {
            try await service.saveRemote(
                projectRoot: try requiredProjectRoot(),
                originalName: originalName,
                name: name,
                url: url
            )
        }
    }

    func removeRemote(_ name: String) async {
        await mutate {
            try await service.removeRemote(
                projectRoot: try requiredProjectRoot(),
                name: name
            )
        }
    }

    func pull() async {
        await mutate(updatesWorkingTree: true) {
            try await service.pull(projectRoot: try requiredProjectRoot())
        }
    }

    func push() async {
        await mutate {
            try await service.push(projectRoot: try requiredProjectRoot())
        }
    }

    func dismissError() { errorMessage = nil }

    private func stage(paths: [String]) async {
        guard !paths.isEmpty else { return }
        await mutate {
            try await service.stage(
                projectRoot: try requiredProjectRoot(),
                paths: paths
            )
        }
    }

    private func unstage(paths: [String]) async {
        guard !paths.isEmpty else { return }
        await mutate {
            try await service.unstage(
                projectRoot: try requiredProjectRoot(),
                paths: paths
            )
        }
    }

    private func mutate(
        updatesWorkingTree: Bool = false,
        _ operation: () async throws -> Void,
        onSuccess: () -> Void = {}
    ) async {
        guard !isPerformingOperation else { return }
        isPerformingOperation = true
        errorMessage = nil
        do {
            try await operation()
            onSuccess()
            if updatesWorkingTree { worktreeRevision += 1 }
            selectedDiff = nil
            if let projectRoot {
                snapshot = try await service.snapshot(projectRoot: projectRoot)
            }
        } catch {
            errorMessage = error.localizedDescription
            if let projectRoot {
                snapshot = (try? await service.snapshot(projectRoot: projectRoot)) ?? snapshot
            }
        }
        isPerformingOperation = false
    }

    private func requiredProjectRoot() throws -> String {
        guard let projectRoot else { throw ProjectGitViewModelError.missingProjectRoot }
        return projectRoot
    }
}

private enum ProjectGitViewModelError: LocalizedError {
    case missingProjectRoot

    var errorDescription: String? { "这个项目还没有连接本机目录。" }
}
