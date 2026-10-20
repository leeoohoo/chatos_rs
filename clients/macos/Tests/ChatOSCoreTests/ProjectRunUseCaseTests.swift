import Foundation
import Testing
@testable import ChatOSCore

@Suite("ProjectRunUseCase")
struct ProjectRunUseCaseTests {
    @Test
    func loadsCatalogStateAndEnvironmentAsOneSnapshot() async throws {
        let service = ProjectRunUseCaseService()
        let snapshot = try await ProjectRunUseCase(service: service).snapshot(projectID: "project")

        #expect(snapshot.catalog.projectID == "project")
        #expect(snapshot.state.projectID == "project")
        #expect(snapshot.environment.terminalUIEnabled == false)
        let fetchCounts = await service.fetchCounts
        #expect(fetchCounts == ["catalog": 1, "state": 1, "environment": 1])
    }

    @Test
    func committedStartIsSeparatedFromFailedStateRefresh() async throws {
        let service = ProjectRunUseCaseService(failingStateFetch: true)
        let result = try await ProjectRunUseCase(service: service).mutate(
            projectID: "project",
            mutation: .start(targetID: "target")
        )

        let startedTargets = await service.startedTargets
        #expect(startedTargets == ["target"])
        #expect(result.state == nil)
        #expect(!result.refreshIsComplete)
    }

    @Test
    func committedDefaultTargetIsReturnedWhenEnvironmentRefreshFails() async throws {
        let service = ProjectRunUseCaseService(failingEnvironmentFetch: true)
        let result = try await ProjectRunUseCase(service: service).mutate(
            projectID: "project",
            mutation: .selectDefaultTarget("target-2")
        )

        let selectedTargets = await service.selectedTargets
        #expect(selectedTargets == ["target-2"])
        #expect(result.catalog?.defaultTargetID == "target-2")
        #expect(result.environment == nil)
        #expect(!result.refreshIsComplete)
    }
}

private enum ProjectRunUseCaseTestError: Error {
    case refreshFailed
}

private actor ProjectRunUseCaseService: ProjectRunServicing {
    private let failingStateFetch: Bool
    private let failingEnvironmentFetch: Bool
    private(set) var fetchCounts: [String: Int] = [:]
    private(set) var startedTargets: [String] = []
    private(set) var selectedTargets: [String] = []

    init(failingStateFetch: Bool = false, failingEnvironmentFetch: Bool = false) {
        self.failingStateFetch = failingStateFetch
        self.failingEnvironmentFetch = failingEnvironmentFetch
    }

    func fetchCatalog(projectID: String) async throws -> ProjectRunCatalog {
        fetchCounts["catalog", default: 0] += 1
        return catalog(projectID: projectID, defaultTargetID: "target")
    }

    func analyze(projectID: String) async throws -> ProjectRunCatalog {
        catalog(projectID: projectID, defaultTargetID: "target")
    }

    func fetchState(projectID: String) async throws -> ProjectRunState {
        fetchCounts["state", default: 0] += 1
        if failingStateFetch { throw ProjectRunUseCaseTestError.refreshFailed }
        return state(projectID: projectID)
    }

    func fetchEnvironment(projectID: String) async throws -> ProjectRunEnvironment {
        fetchCounts["environment", default: 0] += 1
        if failingEnvironmentFetch { throw ProjectRunUseCaseTestError.refreshFailed }
        return environment()
    }

    func updateEnvironment(
        projectID: String,
        selectedToolchains: [String: String],
        customToolchains: [String: ProjectRunCustomToolchain],
        environmentVariables: [String: String]
    ) async throws -> ProjectRunEnvironment {
        environment()
    }

    func setDefaultTarget(projectID: String, targetID: String) async throws -> ProjectRunCatalog {
        selectedTargets.append(targetID)
        return catalog(projectID: projectID, defaultTargetID: targetID)
    }

    func start(projectID: String, targetID: String) async throws {
        startedTargets.append(targetID)
    }

    func stop(instanceID: String) async throws {}
    func delete(instanceID: String) async throws {}

    private func catalog(projectID: String, defaultTargetID: String) -> ProjectRunCatalog {
        .init(
            projectID: projectID,
            status: "ready",
            defaultTargetID: defaultTargetID,
            targets: [target(id: defaultTargetID)],
            errorMessage: nil
        )
    }

    private func target(id: String) -> ProjectRunTarget {
        .init(
            id: id,
            label: id,
            kind: "script",
            language: nil,
            cwd: "/project",
            command: "true",
            source: "test",
            isDefault: true,
            entrypoint: nil,
            manifestPath: nil,
            requiredToolchains: []
        )
    }

    private func state(projectID: String) -> ProjectRunState {
        .init(projectID: projectID, status: "idle", isBusy: false, isRunning: false, instances: [])
    }

    private func environment() -> ProjectRunEnvironment {
        .init(
            validationIssues: [],
            selectedToolchains: [:],
            environmentVariables: [:],
            terminalUIEnabled: false
        )
    }
}
