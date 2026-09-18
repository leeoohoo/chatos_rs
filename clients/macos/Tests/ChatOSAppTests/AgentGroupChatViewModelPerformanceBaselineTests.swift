import ChatOSAgentRuntime
@testable import ChatOSApp
@testable import ChatOSConnector
import ChatOSCore
import Combine
import Foundation
import XCTest

/// Opt-in UI refresh baseline for the fixed-size Agent workspace fixture.
///
/// Run explicitly with:
/// `CHATOS_RUN_AGENT_UI_BASELINE=1 swift test --package-path clients/macos \
///   --filter AgentGroupChatViewModelPerformanceBaselineTests`
@MainActor
final class AgentGroupChatViewModelPerformanceBaselineTests: XCTestCase {
    private static let ownerUserID = "ui-performance-owner"

    private struct Fixture {
        let rootURL: URL
        let service: NativeAgentGroupChatService
        let projectID: String
    }

    private struct Measurements: Codable {
        let fixtureAgents: Int
        let fixtureRooms: Int
        let fixtureMessages: Int
        let fixtureTodos: Int
        let initialLoadReturnMilliseconds: Double
        let initialLoadSettledMilliseconds: Double
        let initialLoadPreparedStatements: Int
        let initialLoadPublications: Int
        let mainActorMaximumGapMilliseconds: Double
        let publish500ChangesMilliseconds: Double
        let burstRefreshSettledMilliseconds: Double
        let burstRefreshPreparedStatements: Int
        let burstRefreshPublications: Int
    }

    func testRepeatableWorkspaceRefreshBaseline() async throws {
        guard ProcessInfo.processInfo.environment["CHATOS_RUN_AGENT_UI_BASELINE"] == "1" else {
            throw XCTSkip("Set CHATOS_RUN_AGENT_UI_BASELINE=1 to run the UI refresh baseline.")
        }

        let fixture = try await makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.rootURL) }
        let store = try await fixture.service.store()
        let viewModel = makeViewModel(fixture: fixture)
        var publicationCount = 0
        let cancellable = viewModel.objectWillChange.sink {
            publicationCount += 1
        }
        let probe = MainActorGapProbe()
        let probeTask = Task { await probe.run() }

        let initialStatementCount = await store.preparedStatementCountForTesting()
        let initialStart = DispatchTime.now().uptimeNanoseconds
        await viewModel.activate()
        let initialReturn = DispatchTime.now().uptimeNanoseconds
        try await waitUntilSettled(viewModel: viewModel, store: store)
        let initialSettled = DispatchTime.now().uptimeNanoseconds
        probeTask.cancel()
        await probeTask.value

        XCTAssertEqual(viewModel.agents.count, 20)
        XCTAssertEqual(viewModel.teams.count, 10)
        XCTAssertEqual(viewModel.members.count, 20)
        XCTAssertEqual(viewModel.messages.count, 50)
        XCTAssertEqual(viewModel.teamTodos.count, 10)

        let initialLoadPreparedStatements = await store.preparedStatementCountForTesting()
            - initialStatementCount
        let initialLoadPublications = publicationCount

        // Let the observation task subscribe before publishing a burst with no durable changes.
        try await Task.sleep(for: .milliseconds(50))
        let burstStatementCount = await store.preparedStatementCountForTesting()
        let burstPublicationCount = publicationCount
        let burstStart = DispatchTime.now().uptimeNanoseconds
        for index in 0..<500 {
            await fixture.service.publishChange(.init(
                ownerUserID: Self.ownerUserID,
                roomID: viewModel.room?.id ?? "",
                runID: UUID(),
                kind: index.isMultiple(of: 2) ? .runUpdated : .roomUpdated
            ))
        }
        let burstPublished = DispatchTime.now().uptimeNanoseconds
        try await Task.sleep(for: .milliseconds(400))
        try await waitUntilSettled(viewModel: viewModel, store: store)
        let burstSettled = DispatchTime.now().uptimeNanoseconds

        let burstRefreshPreparedStatements = await store.preparedStatementCountForTesting()
            - burstStatementCount
        let burstRefreshPublications = publicationCount - burstPublicationCount

        // Freeze the current refresh fan-out. Any lower result should be paired with an explicit
        // coalescing change and before/after measurements rather than silently weakening coverage.
        XCTAssertEqual(initialLoadPreparedStatements, 349)
        XCTAssertEqual(initialLoadPublications, 45)
        XCTAssertEqual(burstRefreshPreparedStatements, 246)
        XCTAssertEqual(burstRefreshPublications, 42)
        XCTAssertGreaterThan(probe.sampleCount, 0)

        let measurements = Measurements(
            fixtureAgents: 20,
            fixtureRooms: 10,
            fixtureMessages: 500,
            fixtureTodos: 100,
            initialLoadReturnMilliseconds: milliseconds(from: initialStart, to: initialReturn),
            initialLoadSettledMilliseconds: milliseconds(from: initialStart, to: initialSettled),
            initialLoadPreparedStatements: initialLoadPreparedStatements,
            initialLoadPublications: initialLoadPublications,
            mainActorMaximumGapMilliseconds: probe.maximumGapMilliseconds,
            publish500ChangesMilliseconds: milliseconds(from: burstStart, to: burstPublished),
            burstRefreshSettledMilliseconds: milliseconds(from: burstStart, to: burstSettled),
            burstRefreshPreparedStatements: burstRefreshPreparedStatements,
            burstRefreshPublications: burstRefreshPublications
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        print("CHATOS_AGENT_GROUP_CHAT_UI_BASELINE \(String(decoding: try encoder.encode(measurements), as: UTF8.self))")
        withExtendedLifetime(cancellable) {}
    }

    private func makeFixture() async throws -> Fixture {
        let rootURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-ui-baseline-\(UUID().uuidString)", isDirectory: true)
        let service = NativeAgentGroupChatService(
            databaseURL: rootURL.appendingPathComponent("group-chat.db")
        )
        let store = try await service.store()
        var agents: [LocalAgentProfile] = []
        for index in 0..<20 {
            agents.append(try await store.createAgent(
                ownerUserID: Self.ownerUserID,
                draft: .init(
                    name: "UI Baseline Agent \(index)",
                    rolePrompt: "Preserve behavior while measuring UI refreshes.",
                    modelConfigID: "baseline-model"
                )
            ))
        }

        var rooms: [ProjectAgentRoom] = []
        for roomIndex in 0..<10 {
            let room = try await store.createRoom(
                ownerUserID: Self.ownerUserID,
                projectID: "ui-baseline-project-\(roomIndex)",
                draft: .init(name: "UI Baseline Team \(roomIndex)", goal: "Repeatable UI baseline")
            )
            rooms.append(room)
            let roomAgents = roomIndex == 0
                ? agents
                : Array(agents[(roomIndex * 2)..<(roomIndex * 2 + 2)])
            for agent in roomAgents {
                _ = try await store.addMember(
                    ownerUserID: Self.ownerUserID,
                    roomID: room.id,
                    agentID: agent.id,
                    draft: .init(role: "Member")
                )
            }
        }

        for index in 0..<100 {
            let room = rooms[index % rooms.count]
            let agent = agents[(index % rooms.count) * 2 + (index / rooms.count) % 2]
            _ = try await store.createAgentTodo(
                ownerUserID: Self.ownerUserID,
                agentID: agent.id,
                requestKey: "ui-baseline-todo-\(index)",
                draft: .init(title: "UI Baseline Todo \(index)", teamRoomID: room.id),
                nowUnixMs: Int64(index + 1)
            )
        }

        for index in 0..<500 {
            _ = try await store.postMessage(
                ownerUserID: Self.ownerUserID,
                roomID: rooms[0].id,
                draft: .init(
                    senderKind: .human,
                    senderID: Self.ownerUserID,
                    content: "UI baseline message \(index)"
                ),
                limits: .init()
            )
        }
        return Fixture(rootURL: rootURL, service: service, projectID: rooms[0].projectID)
    }

    private func makeViewModel(fixture: Fixture) -> AgentGroupChatViewModel {
        let connector = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://baseline.invalid")!,
                stateURL: fixture.rootURL.appendingPathComponent("connector-state.json")
            ),
            ticketProvider: RejectingTicketProvider()
        )
        let projectsService = NativeLocalProjectsService(
            connector: connector,
            databaseURL: fixture.rootURL.appendingPathComponent("projects.db")
        )
        let services = RejectingAgentServices()
        return AgentGroupChatViewModel(
            projectID: fixture.projectID,
            ownerUserID: Self.ownerUserID,
            service: fixture.service,
            scheduler: LocalAgentGroupChatScheduler(service: fixture.service, services: services),
            builderService: LocalAgentBuilderService(
                groupChatService: fixture.service,
                projectsService: projectsService,
                connectorService: connector,
                agentServices: services
            ),
            projectsService: projectsService
        )
    }

    private func waitUntilSettled(
        viewModel: AgentGroupChatViewModel,
        store: SQLiteAgentGroupChatStore
    ) async throws {
        var priorCount = await store.preparedStatementCountForTesting()
        var stableChecks = 0
        for _ in 0..<250 {
            try await Task.sleep(for: .milliseconds(20))
            let currentCount = await store.preparedStatementCountForTesting()
            if currentCount == priorCount, !viewModel.isLoading {
                stableChecks += 1
                if stableChecks == 5 { return }
            } else {
                stableChecks = 0
                priorCount = currentCount
            }
        }
        XCTFail("Timed out waiting for the Agent workspace baseline to settle")
    }

    private func milliseconds(from start: UInt64, to end: UInt64) -> Double {
        Double(end - start) / 1_000_000
    }
}

@MainActor
private final class MainActorGapProbe {
    private(set) var maximumGapMilliseconds = 0.0
    private(set) var sampleCount = 0

    func run() async {
        var previous = DispatchTime.now().uptimeNanoseconds
        while !Task.isCancelled {
            try? await Task.sleep(for: .milliseconds(1))
            let current = DispatchTime.now().uptimeNanoseconds
            maximumGapMilliseconds = max(
                maximumGapMilliseconds,
                Double(current - previous) / 1_000_000
            )
            sampleCount += 1
            previous = current
        }
    }
}

private enum BaselineDependencyError: Error {
    case unexpectedRequest
}

private struct RejectingTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String {
        throw BaselineDependencyError.unexpectedRequest
    }
}

private struct RejectingAgentServices: AgentServiceProviding {
    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        throw BaselineDependencyError.unexpectedRequest
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        throw BaselineDependencyError.unexpectedRequest
    }
}
