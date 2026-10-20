import Foundation

public enum ProjectGitMutation: Sendable, Equatable {
    case initializeRepository
    case stage(paths: [String])
    case unstage(paths: [String])
    case commit(message: String)
    case switchBranch(branch: String)
    case createBranch(name: String, switchToBranch: Bool)
    case mergeBranch(branch: String)
    case saveRemote(originalName: String?, name: String, url: String)
    case removeRemote(name: String)
    case pull
    case push

    public var updatesWorkingTree: Bool {
        switch self {
        case .switchBranch, .mergeBranch, .pull:
            true
        case let .createBranch(_, switchToBranch):
            switchToBranch
        default:
            false
        }
    }
}

public struct ProjectGitMutationResult: Sendable, Equatable {
    public let refreshedSnapshot: ProjectGitSnapshot?

    public init(refreshedSnapshot: ProjectGitSnapshot?) {
        self.refreshedSnapshot = refreshedSnapshot
    }
}

public struct ProjectGitUseCase: Sendable {
    private let service: any ProjectGitServicing

    public init(service: any ProjectGitServicing) {
        self.service = service
    }

    public func snapshot(projectRoot: String) async throws -> ProjectGitSnapshot {
        try await service.snapshot(projectRoot: projectRoot)
    }

    public func diff(
        projectRoot: String,
        change: ProjectGitChange,
        staged: Bool
    ) async throws -> ProjectGitDiff {
        try await service.diff(projectRoot: projectRoot, change: change, staged: staged)
    }

    public func mutate(
        projectRoot: String,
        mutation: ProjectGitMutation
    ) async throws -> ProjectGitMutationResult {
        switch mutation {
        case .initializeRepository:
            try await service.initializeRepository(projectRoot: projectRoot)
        case let .stage(paths):
            try await service.stage(projectRoot: projectRoot, paths: paths)
        case let .unstage(paths):
            try await service.unstage(projectRoot: projectRoot, paths: paths)
        case let .commit(message):
            try await service.commit(projectRoot: projectRoot, message: message)
        case let .switchBranch(branch):
            try await service.switchBranch(projectRoot: projectRoot, branch: branch)
        case let .createBranch(name, switchToBranch):
            try await service.createBranch(
                projectRoot: projectRoot,
                name: name,
                switchToBranch: switchToBranch
            )
        case let .mergeBranch(branch):
            try await service.mergeBranch(projectRoot: projectRoot, branch: branch)
        case let .saveRemote(originalName, name, url):
            try await service.saveRemote(
                projectRoot: projectRoot,
                originalName: originalName,
                name: name,
                url: url
            )
        case let .removeRemote(name):
            try await service.removeRemote(projectRoot: projectRoot, name: name)
        case .pull:
            try await service.pull(projectRoot: projectRoot)
        case .push:
            try await service.push(projectRoot: projectRoot)
        }

        return ProjectGitMutationResult(
            refreshedSnapshot: try? await service.snapshot(projectRoot: projectRoot)
        )
    }
}
