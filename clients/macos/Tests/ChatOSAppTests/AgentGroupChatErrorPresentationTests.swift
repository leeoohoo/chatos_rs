import ChatOSAgentRuntime
@testable import ChatOSApp
@testable import ChatOSConnector
import ChatOSCore
import Combine
import Foundation
import XCTest

@MainActor
final class AgentGroupChatErrorPresentationTests: XCTestCase {
    func testComposerDraftDoesNotPublishTeamWorkspaceUpdates() {
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
        var workspaceUpdateCount = 0
        var composerUpdateCount = 0
        let workspaceCancellable = viewModel.objectWillChange.sink {
            workspaceUpdateCount += 1
        }
        let composerCancellable = viewModel.composerState.objectWillChange.sink {
            composerUpdateCount += 1
        }

        for length in 1 ... 200 {
            viewModel.draftMessage = String(repeating: "x", count: length)
        }

        XCTAssertEqual(viewModel.composerState.draftMessage.count, 200)
        XCTAssertEqual(workspaceUpdateCount, 0)
        XCTAssertEqual(composerUpdateCount, 200)
        withExtendedLifetime((workspaceCancellable, composerCancellable)) {}
    }

    func testComposerDraftDoesNotPublishDirectTimelineUpdates() {
        let fixture = makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.rootURL) }
        let viewModel = AgentDirectChatViewModel(
            ownerUserID: "alice",
            conversationID: "conversation-1",
            service: fixture.groupChatService,
            scheduler: fixture.scheduler,
            builderService: fixture.builderService,
            projectsService: fixture.projectsService
        )
        var timelineUpdateCount = 0
        var composerUpdateCount = 0
        let timelineCancellable = viewModel.objectWillChange.sink {
            timelineUpdateCount += 1
        }
        let composerCancellable = viewModel.composerState.objectWillChange.sink {
            composerUpdateCount += 1
        }

        for length in 1 ... 200 {
            viewModel.draftMessage = String(repeating: "x", count: length)
        }

        XCTAssertEqual(viewModel.composerState.draftMessage.count, 200)
        XCTAssertEqual(timelineUpdateCount, 0)
        XCTAssertEqual(composerUpdateCount, 200)
        withExtendedLifetime((timelineCancellable, composerCancellable)) {}
    }

    func testSchedulerIssueIgnoresAnotherConversation() {
        let result = LocalAgentGroupChatScheduler.DeliveryAttemptReceipt(
            deliveryID: "delivery-1",
            agentID: "agent-1",
            outcome: .suspended,
            detail: "另一个会话暂停"
        )

        let issue = AgentSchedulerIssueReducer.reconcile(
            current: nil,
            receipts: [result],
            roomIDByDeliveryID: ["delivery-1": "other-room"],
            roomID: "visible-room"
        )

        XCTAssertNil(issue)
    }

    func testSchedulerIssueClearsWhenSameDeliveryCompletes() throws {
        let suspended = LocalAgentGroupChatScheduler.DeliveryAttemptReceipt(
            deliveryID: "delivery-1",
            agentID: "agent-1",
            outcome: .suspended,
            detail: "暂时暂停"
        )
        let current = try XCTUnwrap(AgentSchedulerIssueReducer.reconcile(
            current: nil,
            receipts: [suspended],
            roomIDByDeliveryID: ["delivery-1": "visible-room"],
            roomID: "visible-room"
        ))
        let completed = LocalAgentGroupChatScheduler.DeliveryAttemptReceipt(
            deliveryID: "delivery-1",
            agentID: "agent-1",
            outcome: .completed,
            detail: nil
        )

        let issue = AgentSchedulerIssueReducer.reconcile(
            current: current,
            receipts: [completed],
            roomIDByDeliveryID: ["delivery-1": "visible-room"],
            roomID: "visible-room"
        )

        XCTAssertNil(issue)
    }

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
