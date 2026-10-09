import AppKit
import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import SwiftUI
import XCTest
@testable import ChatOSApp

@MainActor
final class AgentModelSelectionTests: XCTestCase {
    private static let current = LocalAgentBuilderModelOption(
        id: "current-model", name: "Provider", provider: "gpt", modelName: "current",
        supportsReasoning: true, thinkingLevels: ["low", "high"]
    )
    private static let removed = LocalAgentBuilderModelOption(
        id: "removed-model", name: "Provider", provider: "gpt", modelName: "removed"
    )

    func testUnknownSelectionNeverAppearsConfiguredOrValid() {
        XCTAssertEqual(AgentModelAvailability.resolve(
            id: "removed-model", models: [Self.current], catalogStatus: .ready
        ), .unavailable)
        XCTAssertFalse(AgentModelPicker.contains("removed-model", in: [Self.current]))
        XCTAssertFalse(AgentModelPicker.contains("", in: [Self.current]))
        XCTAssertTrue(AgentModelPicker.contains(Self.current.id, in: [Self.current]))
        XCTAssertEqual(AgentModelAvailability.resolve(
            id: Self.current.id, models: [Self.current], catalogStatus: .ready
        ), .available(Self.current))
    }

    func testNativePickerDisplaysRemovedSelectionAndOffersValidReplacement() async throws {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 560, height: 180),
                              styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        let hosting = NSHostingView(rootView: AgentModelPicker(
            selection: .constant(Self.removed.id), models: [Self.current]
        ).padding())
        window.contentView = hosting
        for _ in 0..<3 {
            hosting.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(20))
        }
        let picker = try XCTUnwrap(findPopup(in: hosting))
        XCTAssertFalse(picker.title.isEmpty)
        XCTAssertEqual(picker.title, "原模型不可用")
        XCTAssertTrue(picker.itemTitles.contains("Provider · current"))
    }

    private func findPopup(in view: NSView) -> NSPopUpButton? {
        if let popup = view as? NSPopUpButton { return popup }
        for child in view.subviews {
            if let popup = findPopup(in: child) { return popup }
        }
        return nil
    }

    func testUnloadedLoadingAndFailedCatalogsAreNotReportedAsUnavailableOrConfigured() {
        for (status, expected) in [
            (AgentModelCatalogStatus.notLoaded, AgentModelAvailability.unchecked),
            (.loading, .loading), (.failed, .failed),
        ] {
            XCTAssertEqual(AgentModelAvailability.resolve(
                id: Self.current.id, models: [Self.current], catalogStatus: status
            ), expected)
        }
        XCTAssertEqual(AgentModelAvailability.resolve(
            id: Self.current.id, models: [], catalogStatus: .ready
        ), .unavailable)
    }

    func testReopeningEditorReloadsCatalogInsteadOfKeepingDeletedModelsForever() async throws {
        let probe = ModelSelectionCatalogProbe(results: [
            .success(.init(models: [Self.removed], plugins: [])),
            .success(.init(models: [Self.current], plugins: [])),
        ])
        let fixture = makeFixture(probe: probe)
        defer { try? FileManager.default.removeItem(at: fixture.rootURL) }
        let first = await fixture.viewModel.prepareAgentEditor()
        XCTAssertTrue(first)
        XCTAssertEqual(fixture.viewModel.availableModels, [Self.removed])
        let second = await fixture.viewModel.prepareAgentEditor()
        XCTAssertTrue(second)
        XCTAssertEqual(fixture.viewModel.availableModels, [Self.current])
        let count = await probe.count
        XCTAssertEqual(count, 2)
    }

    func testConcurrentEditorPreparationSharesOneCatalogRequest() async throws {
        let probe = ModelSelectionCatalogProbe(results: [
            .success(.init(models: [Self.current], plugins: [])),
        ])
        let fixture = makeFixture(probe: probe)
        defer { try? FileManager.default.removeItem(at: fixture.rootURL) }
        async let first = fixture.viewModel.prepareAgentEditor()
        async let second = fixture.viewModel.prepareAgentEditor()
        let values = await (first, second)
        XCTAssertTrue(values.0 && values.1)
        let count = await probe.count
        XCTAssertEqual(count, 1)
        XCTAssertEqual(fixture.viewModel.modelCatalogStatus, .ready)
    }

    func testFailedRefreshDoesNotClaimCachedModelsAreConfirmed() async throws {
        let probe = ModelSelectionCatalogProbe(results: [
            .success(.init(models: [Self.current], plugins: [])),
            .failure(ModelSelectionFixtureError.unexpectedRequest),
        ])
        let fixture = makeFixture(probe: probe)
        defer { try? FileManager.default.removeItem(at: fixture.rootURL) }
        _ = await fixture.viewModel.prepareAgentEditor()
        fixture.viewModel.errorMessage = "Keep an existing run error visible"
        let refreshed = await fixture.viewModel.refreshModelCatalog(reportErrors: false)
        XCTAssertFalse(refreshed)
        XCTAssertEqual(fixture.viewModel.modelCatalogStatus, .failed)
        XCTAssertEqual(fixture.viewModel.errorMessage, "Keep an existing run error visible")
        XCTAssertEqual(AgentModelAvailability.resolve(
            id: Self.current.id, models: fixture.viewModel.availableModels,
            catalogStatus: fixture.viewModel.modelCatalogStatus
        ), .failed)
    }

    func testSaveRevalidatesCatalogAndDoesNotOverwriteAgentWithRemovedModel() async throws {
        let probe = ModelSelectionCatalogProbe(results: [
            .success(.init(models: [Self.removed], plugins: [])),
            .success(.init(models: [Self.current], plugins: [])),
        ])
        let fixture = makeFixture(probe: probe)
        defer { try? FileManager.default.removeItem(at: fixture.rootURL) }
        let store = try await fixture.service.store()
        let agent = try await store.createAgent(ownerUserID: "alice", draft: .init(
            name: "Existing Agent", rolePrompt: "Preserve my configuration",
            modelConfigID: Self.removed.id, thinkingLevel: "low"
        ))
        _ = await fixture.viewModel.prepareAgentEditor()
        let saved = await save(agent: agent, modelID: Self.removed.id, in: fixture.viewModel)
        XCTAssertFalse(saved)
        XCTAssertEqual(fixture.viewModel.errorMessage, LocalAgentBuilderError.modelUnavailable.localizedDescription)
        let agents = try await store.listAgents(ownerUserID: "alice", includeArchived: false)
        XCTAssertEqual(agents.first?.draft, agent.draft)
        XCTAssertFalse(fixture.viewModel.isSavingAgent)
    }

    func testExplicitlyChoosingAvailableModelRepairsConfiguration() async throws {
        let probe = ModelSelectionCatalogProbe(results: [
            .success(.init(models: [Self.current], plugins: [])),
        ])
        let fixture = makeFixture(probe: probe)
        defer { try? FileManager.default.removeItem(at: fixture.rootURL) }
        let store = try await fixture.service.store()
        let agent = try await store.createAgent(ownerUserID: "alice", draft: .init(
            name: "Existing Agent", rolePrompt: "Preserve my configuration",
            modelConfigID: Self.removed.id, thinkingLevel: "low"
        ))
        let saved = await save(agent: agent, modelID: Self.current.id, in: fixture.viewModel)
        XCTAssertTrue(saved)
        let agents = try await store.listAgents(ownerUserID: "alice", includeArchived: false)
        XCTAssertEqual(agents.first?.draft.modelConfigID, Self.current.id)
        XCTAssertEqual(agents.first?.draft.thinkingLevel, "low")
        XCTAssertEqual(agents.first?.draft.rolePrompt, agent.draft.rolePrompt)
    }

    private func save(
        agent: LocalAgentProfile, modelID: String, in viewModel: AgentGroupChatWorkspaceViewModel
    ) async -> Bool {
        await viewModel.saveAgent(
            existing: agent, name: agent.draft.name, avatarData: nil, description: "",
            rolePrompt: agent.draft.rolePrompt, modelConfigID: modelID, thinkingLevel: "low",
            professionKey: agent.draft.professionKey, canManageStaff: false,
            canAccessLocalProjects: false, heartbeatEnabled: false,
            heartbeatIntervalSeconds: 900, heartbeatPrompt: ""
        )
    }

    private struct Fixture {
        let rootURL: URL
        let service: NativeAgentGroupChatService
        let viewModel: AgentGroupChatWorkspaceViewModel
    }

    private func makeFixture(probe: ModelSelectionCatalogProbe) -> Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-model-selection-\(UUID().uuidString)")
        let service = NativeAgentGroupChatService(databaseURL: root.appendingPathComponent("chat.db"))
        let connector = NativeLocalConnectorService(configuration: .init(
            gatewayBaseURL: URL(string: "https://model-selection.invalid")!,
            stateURL: root.appendingPathComponent("state.json")
        ), ticketProvider: ModelSelectionTicketProvider())
        let projects = NativeLocalProjectsService(connector: connector,
                                                   databaseURL: root.appendingPathComponent("projects.db"))
        let services = ModelSelectionAgentServices()
        let builder = LocalAgentBuilderService(groupChatService: service, projectsService: projects,
                                               connectorService: connector, agentServices: services)
        return .init(rootURL: root, service: service, viewModel: .init(
            ownerUserID: "alice", service: service,
            scheduler: .init(service: service, services: services), builderService: builder,
            loadModelResources: { try await probe.load() }
        ))
    }
}

private enum ModelSelectionFixtureError: Error { case unexpectedRequest }

private actor ModelSelectionCatalogProbe {
    private var results: [Result<LocalAgentBuilderResources, Error>]
    private(set) var count = 0

    init(results: [Result<LocalAgentBuilderResources, Error>]) { self.results = results }

    func load() async throws -> LocalAgentBuilderResources {
        count += 1
        guard !results.isEmpty else { throw ModelSelectionFixtureError.unexpectedRequest }
        let result = results.removeFirst()
        try await Task.sleep(for: .milliseconds(10))
        return try result.get()
    }
}

private struct ModelSelectionTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String {
        throw ModelSelectionFixtureError.unexpectedRequest
    }
}

private struct ModelSelectionAgentServices: AgentServiceProviding {
    func makeAgentModel(configID: String, policy: AgentRunPolicy) async throws -> any AgentModelClient {
        throw ModelSelectionFixtureError.unexpectedRequest
    }
    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        throw ModelSelectionFixtureError.unexpectedRequest
    }
}
