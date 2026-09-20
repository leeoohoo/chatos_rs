import ChatOSAgentRuntime
@testable import ChatOSApp
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

@MainActor
final class AgentGroupChatErrorPresentationTests: XCTestCase {
    func testWorkspaceRefreshDoesNotDismissRunActionError() async throws {
        let fixture = makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.rootURL) }
        let viewModel = AgentGroupChatWorkspaceViewModel(
            ownerUserID: "alice",
            service: fixture.groupChatService,
            scheduler: fixture.scheduler,
            builderService: fixture.builderService
        )
        viewModel.errorMessage = "运行历史或记忆范围不一致"

        await viewModel.load()

        XCTAssertEqual(viewModel.errorMessage, "运行历史或记忆范围不一致")
    }

    func testTeamRefreshDoesNotDismissRunActionError() async throws {
        let fixture = makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.rootURL) }
        let viewModel = AgentGroupChatViewModel(
            projectID: "project-1",
            ownerUserID: "alice",
            service: fixture.groupChatService,
            scheduler: fixture.scheduler,
            builderService: fixture.builderService,
            projectsService: fixture.projectsService
        )
        viewModel.errorMessage = "运行历史或记忆范围不一致"

        await viewModel.load()

        XCTAssertEqual(viewModel.errorMessage, "运行历史或记忆范围不一致")
    }

    private func makeFixture() -> Fixture {
        let rootURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-error-presentation-\(UUID().uuidString)")
        let groupChatService = NativeAgentGroupChatService(
            databaseURL: rootURL.appendingPathComponent("group-chat.db")
        )
        let connector = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://error-presentation.invalid")!,
                stateURL: rootURL.appendingPathComponent("connector-state.json")
            ),
            ticketProvider: ErrorPresentationRejectingTicketProvider()
        )
        let projectsService = NativeLocalProjectsService(
            connector: connector,
            databaseURL: rootURL.appendingPathComponent("projects.db")
        )
        let services = ErrorPresentationRejectingAgentServices()
        return Fixture(
            rootURL: rootURL,
            groupChatService: groupChatService,
            projectsService: projectsService,
            scheduler: LocalAgentGroupChatScheduler(
                service: groupChatService,
                services: services
            ),
            builderService: LocalAgentBuilderService(
                groupChatService: groupChatService,
                projectsService: projectsService,
                connectorService: connector,
                agentServices: services
            )
        )
    }

    private struct Fixture {
        let rootURL: URL
        let groupChatService: NativeAgentGroupChatService
        let projectsService: NativeLocalProjectsService
        let scheduler: LocalAgentGroupChatScheduler
        let builderService: LocalAgentBuilderService
    }
}

private enum ErrorPresentationDependencyError: Error {
    case unexpectedRequest
}

private struct ErrorPresentationRejectingTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String {
        throw ErrorPresentationDependencyError.unexpectedRequest
    }
}

private struct ErrorPresentationRejectingAgentServices: AgentServiceProviding {
    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        throw ErrorPresentationDependencyError.unexpectedRequest
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        throw ErrorPresentationDependencyError.unexpectedRequest
    }
}
