@testable import ChatOSApp
import ChatOSCore
import Foundation
import XCTest

@MainActor
final class ProjectGitViewModelTests: XCTestCase {
    func testLateLoadCannotReplaceNewerSnapshot() async {
        let service = ProjectGitLifecycleService(snapshotPlans: [
            .deferred(snapshot(branch: "old")),
            .immediate(snapshot(branch: "new")),
        ])
        let model = ProjectGitViewModel(projectRoot: "/project", service: service)

        let stale = Task { await model.load() }
        await service.waitForSnapshotCall(1)
        await model.load()
        await service.resumeSnapshot(0)
        await stale.value

        XCTAssertEqual(model.snapshot.currentBranch, "new")
        XCTAssertFalse(model.isLoading)
        XCTAssertNil(model.errorMessage)
    }

    func testLateDiffCannotReplaceNewerSelection() async {
        let service = ProjectGitLifecycleService(deferredDiffPath: "old.txt")
        let model = ProjectGitViewModel(projectRoot: "/project", service: service)
        let old = change(path: "old.txt")
        let new = change(path: "new.txt")

        let stale = Task { await model.showDiff(for: old, staged: false) }
        await service.waitForDeferredDiff()
        await model.showDiff(for: new, staged: true)
        await service.resumeDiff()
        await stale.value

        XCTAssertEqual(model.selectedDiff?.path, "new.txt")
        XCTAssertEqual(model.selectedDiff?.isStaged, true)
    }

    func testCommittedMutationIsNotReportedAsFailedWhenRefreshFails() async {
        let service = ProjectGitLifecycleService(snapshotPlans: [
            .immediate(snapshot(branch: "main")),
            .failure,
        ])
        let model = ProjectGitViewModel(projectRoot: "/project", service: service)
        await model.load()

        await model.switchBranch("feature")

        let switchedBranches = await service.switchedBranches
        XCTAssertEqual(switchedBranches, ["feature"])
        XCTAssertEqual(model.snapshot.currentBranch, "main")
        XCTAssertEqual(model.worktreeRevision, 1)
        XCTAssertEqual(model.errorMessage, "Git 操作已完成，但最新状态暂时无法刷新。")
        XCTAssertFalse(model.isPerformingOperation)
    }

    func testFailedMutationRecoversLatestSnapshotWithoutHidingOperationError() async {
        let service = ProjectGitLifecycleService(
            snapshotPlans: [
                .immediate(snapshot(branch: "main", changes: [change(path: "old.txt")])),
                .immediate(snapshot(branch: "main", changes: [change(path: "recovered.txt")])),
            ],
            stageError: ProjectGitLifecycleError.operationFailed
        )
        let model = ProjectGitViewModel(projectRoot: "/project", service: service)
        await model.load()

        await model.stage(change(path: "old.txt"))

        XCTAssertEqual(model.snapshot.changes.map(\.path), ["recovered.txt"])
        XCTAssertEqual(model.errorMessage, ProjectGitLifecycleError.operationFailed.localizedDescription)
        XCTAssertFalse(model.isPerformingOperation)
    }

    func testCommittedCommitDoesNotClearNewerMessageTypedDuringRequest() async {
        let service = ProjectGitLifecycleService(
            snapshotPlans: [.immediate(snapshot(branch: "main"))],
            suspendsCommit: true
        )
        let model = ProjectGitViewModel(projectRoot: "/project", service: service)
        model.commitMessage = "first message"

        let commit = Task { await model.commit() }
        await service.waitForCommit()
        model.commitMessage = "next message"
        await service.resumeCommit()
        await commit.value

        let commitMessages = await service.commitMessages
        XCTAssertEqual(commitMessages, ["first message"])
        XCTAssertEqual(model.commitMessage, "next message")
        XCTAssertFalse(model.isPerformingOperation)
    }

    private func snapshot(
        branch: String,
        changes: [ProjectGitChange] = []
    ) -> ProjectGitSnapshot {
        ProjectGitSnapshot(
            isRepository: true,
            repositoryRoot: "/project",
            currentBranch: branch,
            hasHead: true,
            changes: changes,
            branches: [.init(name: branch, isCurrent: true)]
        )
    }

    private func change(path: String) -> ProjectGitChange {
        ProjectGitChange(
            path: path,
            absolutePath: "/project/\(path)",
            indexStatus: " ",
            workTreeStatus: "M",
            kind: .modified
        )
    }
}

private enum ProjectGitSnapshotPlan: Sendable {
    case immediate(ProjectGitSnapshot)
    case deferred(ProjectGitSnapshot)
    case failure
}

private enum ProjectGitLifecycleError: LocalizedError, Sendable {
    case snapshotFailed
    case operationFailed

    var errorDescription: String? {
        switch self {
        case .snapshotFailed: "snapshot failed"
        case .operationFailed: "operation failed"
        }
    }
}

private actor ProjectGitLifecycleService: ProjectGitServicing {
    private let snapshotPlans: [ProjectGitSnapshotPlan]
    private let deferredDiffPath: String?
    private let stageError: ProjectGitLifecycleError?
    private let suspendsCommit: Bool
    private var snapshotCallCount = 0
    private var snapshotContinuations: [Int: CheckedContinuation<Void, Never>] = [:]
    private var diffStarted = false
    private var diffContinuation: CheckedContinuation<Void, Never>?
    private var commitStarted = false
    private var commitContinuation: CheckedContinuation<Void, Never>?
    private(set) var switchedBranches: [String] = []
    private(set) var commitMessages: [String] = []

    init(
        snapshotPlans: [ProjectGitSnapshotPlan] = [],
        deferredDiffPath: String? = nil,
        stageError: ProjectGitLifecycleError? = nil,
        suspendsCommit: Bool = false
    ) {
        self.snapshotPlans = snapshotPlans
        self.deferredDiffPath = deferredDiffPath
        self.stageError = stageError
        self.suspendsCommit = suspendsCommit
    }

    func snapshot(projectRoot: String) async throws -> ProjectGitSnapshot {
        let index = snapshotCallCount
        snapshotCallCount += 1
        let plan = index < snapshotPlans.count
            ? snapshotPlans[index]
            : .immediate(ProjectGitSnapshot(isRepository: true, repositoryRoot: projectRoot))
        switch plan {
        case let .immediate(snapshot):
            return snapshot
        case let .deferred(snapshot):
            await withCheckedContinuation { snapshotContinuations[index] = $0 }
            return snapshot
        case .failure:
            throw ProjectGitLifecycleError.snapshotFailed
        }
    }

    func initializeRepository(projectRoot: String) async throws {}

    func diff(
        projectRoot: String,
        change: ProjectGitChange,
        staged: Bool
    ) async throws -> ProjectGitDiff {
        if change.path == deferredDiffPath {
            diffStarted = true
            await withCheckedContinuation { diffContinuation = $0 }
        }
        return ProjectGitDiff(path: change.path, isStaged: staged, content: change.path)
    }

    func stage(projectRoot: String, paths: [String]) async throws {
        if let stageError { throw stageError }
    }

    func unstage(projectRoot: String, paths: [String]) async throws {}

    func commit(projectRoot: String, message: String) async throws {
        commitMessages.append(message)
        guard suspendsCommit else { return }
        commitStarted = true
        await withCheckedContinuation { commitContinuation = $0 }
    }

    func switchBranch(projectRoot: String, branch: String) async throws {
        switchedBranches.append(branch)
    }

    func createBranch(
        projectRoot: String,
        name: String,
        switchToBranch: Bool
    ) async throws {}

    func mergeBranch(projectRoot: String, branch: String) async throws {}

    func saveRemote(
        projectRoot: String,
        originalName: String?,
        name: String,
        url: String
    ) async throws {}

    func removeRemote(projectRoot: String, name: String) async throws {}
    func pull(projectRoot: String) async throws {}
    func push(projectRoot: String) async throws {}

    func waitForSnapshotCall(_ count: Int) async {
        while snapshotCallCount < count { await Task.yield() }
    }

    func resumeSnapshot(_ index: Int) {
        snapshotContinuations.removeValue(forKey: index)?.resume()
    }

    func waitForDeferredDiff() async {
        while !diffStarted { await Task.yield() }
    }

    func resumeDiff() {
        diffContinuation?.resume()
        diffContinuation = nil
    }

    func waitForCommit() async {
        while !commitStarted { await Task.yield() }
    }

    func resumeCommit() {
        commitContinuation?.resume()
        commitContinuation = nil
    }
}
