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
    @Published var commitMessage = "" {
        didSet { commitMessageRevision += 1 }
    }

    let projectRoot: String?
    private let useCase: ProjectGitUseCase
    private var requestSequence = 0
    private var commitMessageRevision = 0
    private var activeLoadRequest: Int?
    private var activeDiffRequest: DiffRequest?
    private var activeMutationRequest: MutationRequest?

    init(projectRoot: String?, service: any ProjectGitServicing) {
        self.projectRoot = projectRoot
        self.useCase = ProjectGitUseCase(service: service)
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
        guard activeMutationRequest == nil else { return }
        let request = nextRequestID()
        activeLoadRequest = request
        activeDiffRequest = nil
        isLoading = true
        errorMessage = nil
        defer { finishLoad(request) }
        do {
            let snapshot = try await useCase.snapshot(projectRoot: projectRoot)
            guard activeLoadRequest == request else { return }
            self.snapshot = snapshot
        } catch {
            guard activeLoadRequest == request else { return }
            errorMessage = error.localizedDescription
        }
    }

    func initializeRepository() async {
        await mutate(.initializeRepository)
    }

    func showDiff(for change: ProjectGitChange, staged: Bool) async {
        guard let projectRoot, activeMutationRequest == nil else { return }
        let request = DiffRequest(id: nextRequestID(), path: change.path, staged: staged)
        activeDiffRequest = request
        errorMessage = nil
        do {
            let diff = try await useCase.diff(
                projectRoot: projectRoot,
                change: change,
                staged: staged
            )
            guard activeDiffRequest == request else { return }
            guard diff.path == change.path, diff.isStaged == staged else {
                throw ProjectGitViewModelError.mismatchedDiff
            }
            selectedDiff = diff
        } catch {
            guard activeDiffRequest == request else { return }
            errorMessage = error.localizedDescription
        }
        if activeDiffRequest == request { activeDiffRequest = nil }
    }

    func clearDiff() {
        activeDiffRequest = nil
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
        let message = CommitMessageRequest(value: commitMessage, revision: commitMessageRevision)
        await mutate(.commit(message: message.value), committedMessage: message)
    }

    func switchBranch(_ branch: String) async {
        guard branch != snapshot.currentBranch else { return }
        await mutate(.switchBranch(branch: branch))
    }

    func createBranch(name: String, switchToBranch: Bool) async {
        await mutate(.createBranch(name: name, switchToBranch: switchToBranch))
    }

    func mergeBranch(_ branch: String) async {
        await mutate(.mergeBranch(branch: branch))
    }

    func saveRemote(originalName: String?, name: String, url: String) async {
        await mutate(.saveRemote(originalName: originalName, name: name, url: url))
    }

    func removeRemote(_ name: String) async {
        await mutate(.removeRemote(name: name))
    }

    func pull() async {
        await mutate(.pull)
    }

    func push() async {
        await mutate(.push)
    }

    func dismissError() { errorMessage = nil }

    private func stage(paths: [String]) async {
        guard !paths.isEmpty else { return }
        await mutate(.stage(paths: paths))
    }

    private func unstage(paths: [String]) async {
        guard !paths.isEmpty else { return }
        await mutate(.unstage(paths: paths))
    }

    private func mutate(
        _ mutation: ProjectGitMutation,
        committedMessage: CommitMessageRequest? = nil
    ) async {
        guard activeMutationRequest == nil else { return }
        guard let projectRoot else {
            errorMessage = ProjectGitViewModelError.missingProjectRoot.localizedDescription
            return
        }
        let request = MutationRequest(id: nextRequestID(), mutation: mutation)
        activeMutationRequest = request
        activeLoadRequest = nil
        activeDiffRequest = nil
        isLoading = false
        isPerformingOperation = true
        errorMessage = nil
        defer { finishMutation(request) }
        do {
            let result = try await useCase.mutate(projectRoot: projectRoot, mutation: mutation)
            guard activeMutationRequest == request else { return }
            if let committedMessage, commitMessageRevision == committedMessage.revision {
                commitMessage = ""
            }
            if mutation.updatesWorkingTree { worktreeRevision += 1 }
            selectedDiff = nil
            if let refreshedSnapshot = result.refreshedSnapshot {
                snapshot = refreshedSnapshot
            } else {
                errorMessage = "Git 操作已完成，但最新状态暂时无法刷新。"
            }
        } catch {
            guard activeMutationRequest == request else { return }
            let operationError = error.localizedDescription
            let recoveredSnapshot = try? await useCase.snapshot(projectRoot: projectRoot)
            guard activeMutationRequest == request else { return }
            if let recoveredSnapshot { snapshot = recoveredSnapshot }
            errorMessage = operationError
        }
    }

    private func nextRequestID() -> Int {
        requestSequence += 1
        return requestSequence
    }

    private func finishLoad(_ request: Int) {
        guard activeLoadRequest == request else { return }
        activeLoadRequest = nil
        isLoading = false
    }

    private func finishMutation(_ request: MutationRequest) {
        guard activeMutationRequest == request else { return }
        activeMutationRequest = nil
        isPerformingOperation = false
    }

    private struct DiffRequest: Equatable {
        let id: Int
        let path: String
        let staged: Bool
    }

    private struct MutationRequest: Equatable {
        let id: Int
        let mutation: ProjectGitMutation
    }

    private struct CommitMessageRequest {
        let value: String
        let revision: Int
    }
}

private enum ProjectGitViewModelError: LocalizedError {
    case missingProjectRoot
    case mismatchedDiff

    var errorDescription: String? {
        switch self {
        case .missingProjectRoot: "这个项目还没有连接本机目录。"
        case .mismatchedDiff: "Git 差异返回了不匹配的文件。"
        }
    }
}
