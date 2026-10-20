@testable import ChatOSApp
import ChatOSCore
import Foundation
import XCTest

@MainActor
final class ProjectRunSettingsViewModelTests: XCTestCase {
    func testLateLoadCannotReplaceNewerCatalog() async {
        let service = ProjectRunViewModelService(deferFirstCatalog: true)
        let model = ProjectRunSettingsViewModel(projectID: "project", service: service)

        let stale = Task { await model.load() }
        await service.waitForFirstCatalog()
        await model.load()
        await service.resumeFirstCatalog()
        await stale.value

        XCTAssertEqual(model.catalog?.defaultTargetID, "new")
        XCTAssertEqual(model.selectedTargetID, "new")
        XCTAssertFalse(model.isLoading)
        XCTAssertNil(model.errorMessage)
    }

    func testCommittedStartWithRefreshFailureIsNotReportedAsOperationFailure() async {
        let service = ProjectRunViewModelService(failStateAfterFirstFetch: true)
        let model = ProjectRunSettingsViewModel(projectID: "project", service: service)
        await model.load()

        await model.start()

        let startedTargets = await service.startedTargets
        XCTAssertEqual(startedTargets, ["new"])
        XCTAssertEqual(model.notice, .instanceStarted)
        XCTAssertNotNil(model.refreshWarning)
        XCTAssertNil(model.errorMessage)
        XCTAssertFalse(model.isMutating)
    }

    func testEnvironmentSaveDoesNotOverwriteDraftEditedDuringRequest() async throws {
        let service = ProjectRunViewModelService(suspendEnvironmentUpdate: true)
        let model = ProjectRunSettingsViewModel(projectID: "project", service: service)
        await model.load()
        model.environmentVariables = [.init(key: "PORT", value: "4000")]

        let save = Task { await model.saveEnvironment() }
        await service.waitForEnvironmentUpdate()
        model.environmentVariables[0].value = "5000"
        await service.resumeEnvironmentUpdate()
        await save.value

        XCTAssertEqual(model.environmentVariables.first?.value, "5000")
        XCTAssertEqual(model.environment?.environmentVariables["PORT"], "4000")
        XCTAssertEqual(model.notice, .environmentSaved)
        XCTAssertFalse(model.isMutating)
    }

    func testDeactivateRejectsLateLoad() async {
        let service = ProjectRunViewModelService(deferFirstCatalog: true)
        let model = ProjectRunSettingsViewModel(projectID: "project", service: service)

        let stale = Task { await model.load() }
        await service.waitForFirstCatalog()
        model.deactivate()
        await service.resumeFirstCatalog()
        await stale.value

        XCTAssertNil(model.catalog)
        XCTAssertFalse(model.isLoading)
        XCTAssertFalse(model.isMutating)
    }
}

private enum ProjectRunViewModelTestError: LocalizedError, Sendable {
    case refreshFailed

    var errorDescription: String? { "refresh failed" }
}

private actor ProjectRunViewModelService: ProjectRunServicing {
    private let deferFirstCatalog: Bool
    private let failStateAfterFirstFetch: Bool
    private let suspendEnvironmentUpdate: Bool
    private var catalogCallCount = 0
    private var stateCallCount = 0
    private var firstCatalogStarted = false
    private var firstCatalogContinuation: CheckedContinuation<Void, Never>?
    private var environmentUpdateStarted = false
    private var environmentUpdateContinuation: CheckedContinuation<Void, Never>?
    private(set) var startedTargets: [String] = []

    init(
        deferFirstCatalog: Bool = false,
        failStateAfterFirstFetch: Bool = false,
        suspendEnvironmentUpdate: Bool = false
    ) {
        self.deferFirstCatalog = deferFirstCatalog
        self.failStateAfterFirstFetch = failStateAfterFirstFetch
        self.suspendEnvironmentUpdate = suspendEnvironmentUpdate
    }

    func fetchCatalog(projectID: String) async throws -> ProjectRunCatalog {
        let call = catalogCallCount
        catalogCallCount += 1
        if call == 0, deferFirstCatalog {
            firstCatalogStarted = true
            await withCheckedContinuation { firstCatalogContinuation = $0 }
            return catalog(projectID: projectID, targetID: "old")
        }
        return catalog(projectID: projectID, targetID: "new")
    }

    func analyze(projectID: String) async throws -> ProjectRunCatalog {
        catalog(projectID: projectID, targetID: "new")
    }

    func fetchState(projectID: String) async throws -> ProjectRunState {
        stateCallCount += 1
        if failStateAfterFirstFetch, stateCallCount > 1 {
            throw ProjectRunViewModelTestError.refreshFailed
        }
        return .init(
            projectID: projectID,
            status: "idle",
            isBusy: false,
            isRunning: false,
            instances: []
        )
    }

    func fetchEnvironment(projectID: String) async throws -> ProjectRunEnvironment {
        environment(variables: ["PORT": "3000"])
    }

    func updateEnvironment(
        projectID: String,
        selectedToolchains: [String: String],
        customToolchains: [String: ProjectRunCustomToolchain],
        environmentVariables: [String: String]
    ) async throws -> ProjectRunEnvironment {
        if suspendEnvironmentUpdate {
            environmentUpdateStarted = true
            await withCheckedContinuation { environmentUpdateContinuation = $0 }
        }
        return environment(variables: environmentVariables)
    }

    func setDefaultTarget(projectID: String, targetID: String) async throws -> ProjectRunCatalog {
        catalog(projectID: projectID, targetID: targetID)
    }

    func start(projectID: String, targetID: String) async throws {
        startedTargets.append(targetID)
    }

    func stop(instanceID: String) async throws {}
    func delete(instanceID: String) async throws {}

    func waitForFirstCatalog() async {
        while !firstCatalogStarted { await Task.yield() }
    }

    func resumeFirstCatalog() {
        firstCatalogContinuation?.resume()
        firstCatalogContinuation = nil
    }

    func waitForEnvironmentUpdate() async {
        while !environmentUpdateStarted { await Task.yield() }
    }

    func resumeEnvironmentUpdate() {
        environmentUpdateContinuation?.resume()
        environmentUpdateContinuation = nil
    }

    private func catalog(projectID: String, targetID: String) -> ProjectRunCatalog {
        .init(
            projectID: projectID,
            status: "ready",
            defaultTargetID: targetID,
            targets: [.init(
                id: targetID,
                label: targetID,
                kind: "script",
                language: nil,
                cwd: "/project",
                command: "true",
                source: "test",
                isDefault: true,
                entrypoint: nil,
                manifestPath: nil,
                requiredToolchains: []
            )],
            errorMessage: nil
        )
    }

    private func environment(variables: [String: String]) -> ProjectRunEnvironment {
        .init(
            validationIssues: [],
            selectedToolchains: [:],
            environmentVariables: variables,
            terminalUIEnabled: false
        )
    }
}
