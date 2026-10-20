@testable import ChatOSApp
import ChatOSCore
import Foundation
import XCTest

@MainActor
final class PluginApplicationHostViewModelTests: XCTestCase {
    func testProjectSelectionLaunchesThroughApplicationBoundary() async {
        let contextProvider = PluginApplicationViewModelContextProvider()
        let launcher = PluginApplicationViewModelLauncher()
        let model = makeModel(
            application: application(contextScope: "project"),
            contextProvider: contextProvider,
            launcher: launcher
        )

        model.activate()
        XCTAssertEqual(model.phase, .choosingContext)
        model.selectProject(id: "project-1")
        await waitUntil { model.phase == .ready }

        XCTAssertEqual(model.selectedProjectID, "project-1")
        XCTAssertEqual(model.bridgeContext, .init(
            projectID: "project-1",
            projectName: "Project"
        ))
        let projectIDs = await contextProvider.projectIDs
        let contexts = await launcher.contexts
        XCTAssertEqual(projectIDs, ["project-1"])
        XCTAssertEqual(contexts.compactMap { $0 }.first?.projectRoot, "private-root")
    }

    func testSwitchingContextRejectsLateLaunchResult() async {
        let launcher = PluginApplicationViewModelLauncher(suspendFirstLaunch: true)
        let model = makeModel(
            application: application(contextScope: "project"),
            launcher: launcher
        )

        model.activate()
        model.selectProject(id: "project-1")
        await launcher.waitForFirstLaunch()
        model.chooseAnotherContext()
        await launcher.resumeFirstLaunch()
        await Task.yield()

        XCTAssertEqual(model.phase, .choosingContext)
        XCTAssertNil(model.launch)
        XCTAssertNil(model.errorMessage)
    }

    func testViewModelCanReleaseWhileLauncherIgnoresCancellation() async {
        let launcher = PluginApplicationViewModelLauncher(suspendFirstLaunch: true)
        var model: PluginApplicationHostViewModel? = makeModel(launcher: launcher)
        weak var weakModel: PluginApplicationHostViewModel?
        weakModel = model

        model?.activate()
        await launcher.waitForFirstLaunch()
        model = nil

        XCTAssertNil(weakModel)
        await launcher.resumeFirstLaunch()
    }

    private func makeModel(
        application: LocalConnectorPluginApplication = application(),
        contextProvider: PluginApplicationViewModelContextProvider = .init(),
        launcher: PluginApplicationViewModelLauncher
    ) -> PluginApplicationHostViewModel {
        PluginApplicationHostViewModel(
            application: application,
            ownerUserID: "owner-1",
            accountGeneration: 7,
            useCase: PluginApplicationLaunchUseCase(
                contextProvider: contextProvider,
                launcher: launcher
            )
        )
    }

    private func waitUntil(
        _ predicate: @escaping @MainActor () -> Bool
    ) async {
        for _ in 0..<1_000 {
            if predicate() { return }
            await Task.yield()
        }
        XCTFail("condition was not met")
    }
}

private actor PluginApplicationViewModelContextProvider: PluginApplicationContextProviding {
    private(set) var projectIDs: [String] = []

    func pluginContext(
        ownerUserID: String,
        projectID: String
    ) async throws -> LocalConnectorPluginApplicationContext {
        projectIDs.append(projectID)
        return .init(
            projectID: projectID,
            projectName: "Project",
            projectRoot: "private-root"
        )
    }
}

private actor PluginApplicationViewModelLauncher: LocalConnectorPluginApplicationLaunching {
    private let suspendFirstLaunch: Bool
    private var firstLaunchStarted = false
    private var firstLaunchContinuation: CheckedContinuation<Void, Never>?
    private(set) var contexts: [LocalConnectorPluginApplicationContext?] = []

    init(suspendFirstLaunch: Bool = false) {
        self.suspendFirstLaunch = suspendFirstLaunch
    }

    func launchPluginApplication(
        pluginID: String,
        componentKey: String,
        context: LocalConnectorPluginApplicationContext?,
        expectedOwnerUserID: String
    ) async throws -> LocalConnectorPluginApplicationLaunch {
        contexts.append(context)
        if suspendFirstLaunch, contexts.count == 1 {
            firstLaunchStarted = true
            await withCheckedContinuation { firstLaunchContinuation = $0 }
        }
        return .init(
            application: application(),
            url: URL(string: "https://plugin.invalid")!
        )
    }

    func waitForFirstLaunch() async {
        while !firstLaunchStarted { await Task.yield() }
    }

    func resumeFirstLaunch() {
        firstLaunchContinuation?.resume()
        firstLaunchContinuation = nil
    }
}

private func application(
    contextScope: String? = nil
) -> LocalConnectorPluginApplication {
    .init(
        pluginID: "plugin",
        componentKey: "main",
        displayName: "Plugin",
        description: "",
        requiresLocalRuntime: true,
        contextScope: contextScope,
        missingContext: "device",
        bridgeCapabilities: ["host.context.read"]
    )
}
