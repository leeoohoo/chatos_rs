import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

/// An opt-in, repeatable data-volume baseline for the Agent workspace hot path.
///
/// Run explicitly with:
/// `CHATOS_RUN_AGENT_STORE_BASELINE=1 swift test --package-path clients/macos \
///   --filter AgentGroupChatStorePerformanceBaselineTests`
///
/// Add `-c release` for optimized timings. Prepared-statement counts are Debug-only:
/// Release JSON omits those unavailable fields, but still checks database writes and results.
/// The default suite skips this test so routine correctness checks do not absorb the fixture cost.
final class AgentGroupChatStorePerformanceBaselineTests: XCTestCase {
    private static let ownerUserID = "performance-owner"

    private struct Fixture {
        let databaseURL: URL
        let primaryRoomID: String
        let imageMessageID: String
        let imageAttachmentID: String
    }

    private struct Measurements: Codable {
        let buildConfiguration: String
        let preparedStatementCountingEnabled: Bool
        let fixtureAgents: Int
        let fixtureRooms: Int
        let fixtureMessages: Int
        let fixtureRuns: Int
        let fixtureTodos: Int
        let repetitions: Int
        let openStoreMilliseconds: [Double]
        let workspaceSnapshotMilliseconds: [Double]
        let recentMessagesMilliseconds: [Double]
        let imageAttachmentReadMilliseconds: [Double]
        let workspaceSnapshotPreparedStatements: [Int]?
        let recentMessagesPreparedStatements: [Int]?
        let imageAttachmentReadPreparedStatements: [Int]?
        let publish500RunChangesMilliseconds: [Double]
        let idleHeartbeatPoll500Milliseconds: [Double]
        let idleHeartbeatPollPreparedStatements: [Int]?
        let idleHeartbeatPollDatabaseChanges: [Int64]
        let idleAccountDrainMilliseconds: [Double]
        let idleAccountDrainPreparedStatements: [Int]?
        let idleAccountDrainDatabaseChanges: [Int64]
    }

    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-store-baseline-\(UUID().uuidString)")
            .appendingPathComponent("group-chat.db")
    }

    private func elapsedMilliseconds<T>(
        _ operation: () async throws -> T
    ) async rethrows -> (T, Double) {
        let start = DispatchTime.now().uptimeNanoseconds
        let value = try await operation()
        let end = DispatchTime.now().uptimeNanoseconds
        return (value, Double(end - start) / 1_000_000)
    }

    private func makeFixture() async throws -> Fixture {
        let url = databaseURL()
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agents = try await (0..<20).asyncMap { index in
            try await store.createAgent(
                ownerUserID: Self.ownerUserID,
                draft: .init(
                    name: "Baseline Agent \(index)",
                    rolePrompt: "Preserve behavior while measuring the local store.",
                    modelConfigID: "baseline-model"
                )
            )
        }

        var rooms: [ProjectAgentRoom] = []
        for roomIndex in 0..<10 {
            let room = try await store.createRoom(
                ownerUserID: Self.ownerUserID,
                projectID: "baseline-project-\(roomIndex)",
                draft: .init(name: "Baseline Team \(roomIndex)", goal: "Repeatable store baseline")
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
                requestKey: "baseline-todo-\(index)",
                draft: .init(
                    title: "Baseline Todo \(index)",
                    detail: "Stable fixture item for workspace loading.",
                    teamRoomID: room.id
                ),
                nowUnixMs: Int64(index + 1)
            )
        }

        var imageMessageID = ""
        var imageAttachmentID = ""
        let primaryRoom = rooms[0]
        for index in 0..<500 {
            let agent = agents[index % agents.count]
            let attachments: [ProjectAgentMessageAttachmentDraft]
            if index == 250 {
                attachments = [
                    .init(
                        name: "baseline.png",
                        mimeType: "image/png",
                        kind: .image,
                        origin: .pastedImage,
                        data: Data(repeating: 0x5A, count: 128 * 1_024)
                    ),
                ]
            } else {
                attachments = []
            }
            let post = try await store.postMessage(
                ownerUserID: Self.ownerUserID,
                roomID: primaryRoom.id,
                draft: .init(
                    senderKind: .human,
                    senderID: Self.ownerUserID,
                    content: "Baseline message \(index)",
                    mentionedAgentIDs: [agent.id],
                    attachments: attachments
                ),
                limits: .init()
            )
            if let attachment = post.message.attachmentItems.first {
                imageMessageID = post.message.id
                imageAttachmentID = attachment.id
            }
            let claimTime = post.message.createdAtUnixMs + 1
            let claimed = try await store.claimNextDelivery(
                ownerUserID: Self.ownerUserID,
                agentID: agent.id,
                nowUnixMs: claimTime
            )
            let delivery = try XCTUnwrap(claimed)
            let runID = UUID()
            let context = try LocalAgentChatRunContext(
                ownerUserID: Self.ownerUserID,
                projectID: primaryRoom.projectID,
                roomID: primaryRoom.id,
                agentID: agent.id,
                deliveryID: delivery.id,
                triggerMessageID: post.message.id,
                rootMessageID: post.message.rootMessageID,
                runID: runID.uuidString.lowercased(),
                hopCount: delivery.hopCount
            )
            var checkpoint = AgentRunCheckpoint(
                scope: LocalAgentGroupChatRun.runtimeScope(for: context),
                messages: [.init(role: .system, content: "baseline")]
            )
            checkpoint.id = runID
            checkpoint.status = .completed
            checkpoint.result = "baseline complete"
            let run = try LocalAgentGroupChatRun(
                id: runID,
                context: context,
                modelConfigID: agent.draft.modelConfigID,
                policy: .init(),
                checkpoint: checkpoint,
                createdAtUnixMs: claimTime,
                updatedAtUnixMs: claimTime
            )
            try await store.saveRun(run)
            _ = try await store.completeHeartbeatDelivery(
                ownerUserID: Self.ownerUserID,
                deliveryID: delivery.id,
                nowUnixMs: claimTime + 1
            )
        }

        return Fixture(
            databaseURL: url,
            primaryRoomID: primaryRoom.id,
            imageMessageID: imageMessageID,
            imageAttachmentID: imageAttachmentID
        )
    }

    func testRepeatableWorkspaceDataVolumeBaseline() async throws {
        guard ProcessInfo.processInfo.environment["CHATOS_RUN_AGENT_STORE_BASELINE"] == "1" else {
            throw XCTSkip("Set CHATOS_RUN_AGENT_STORE_BASELINE=1 to run the data-volume baseline.")
        }
        let fixture = try await makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.databaseURL.deletingLastPathComponent()) }
        let repetitions = 3
        var openStoreMilliseconds: [Double] = []
        var workspaceSnapshotMilliseconds: [Double] = []
        var recentMessagesMilliseconds: [Double] = []
        var imageAttachmentReadMilliseconds: [Double] = []
        var workspaceSnapshotPreparedStatements: [Int]?
        var recentMessagesPreparedStatements: [Int]?
        var imageAttachmentReadPreparedStatements: [Int]?
        var publish500RunChangesMilliseconds: [Double] = []
        var idleHeartbeatPoll500Milliseconds: [Double] = []
        var idleHeartbeatPollPreparedStatements: [Int]?
        var idleHeartbeatPollDatabaseChanges: [Int64] = []
        var idleAccountDrainMilliseconds: [Double] = []
        var idleAccountDrainPreparedStatements: [Int]?
        var idleAccountDrainDatabaseChanges: [Int64] = []
#if DEBUG
        let buildConfiguration = "debug"
        workspaceSnapshotPreparedStatements = []
        recentMessagesPreparedStatements = []
        imageAttachmentReadPreparedStatements = []
        idleHeartbeatPollPreparedStatements = []
        idleAccountDrainPreparedStatements = []
#else
        let buildConfiguration = "release"
#endif

        for _ in 0..<repetitions {
            let (store, openDuration) = try await elapsedMilliseconds {
                try SQLiteAgentGroupChatStore(databaseURL: fixture.databaseURL)
            }
            openStoreMilliseconds.append(openDuration)

#if DEBUG
            let snapshotCountBefore = await store.preparedStatementCountForTesting()
#endif
            let (snapshot, snapshotDuration) = try await elapsedMilliseconds {
                async let room = store.room(
                    ownerUserID: Self.ownerUserID,
                    roomID: fixture.primaryRoomID
                )
                async let members = store.listMembers(
                    ownerUserID: Self.ownerUserID,
                    roomID: fixture.primaryRoomID
                )
                async let messages = store.listMessages(
                    ownerUserID: Self.ownerUserID,
                    roomID: fixture.primaryRoomID,
                    afterUnixMs: nil,
                    limit: 500
                )
                async let runs = store.listRoomRunHistorySummaries(
                    ownerUserID: Self.ownerUserID,
                    roomID: fixture.primaryRoomID,
                    limit: 500
                )
                async let todos = store.listTeamTodos(
                    ownerUserID: Self.ownerUserID,
                    teamRoomID: fixture.primaryRoomID,
                    includeTerminal: true
                )
                return try await (room, members, messages, runs, todos)
            }
            XCTAssertEqual(snapshot.0?.id, fixture.primaryRoomID)
            XCTAssertEqual(snapshot.1.count, 20)
            XCTAssertEqual(snapshot.2.count, 500)
            XCTAssertEqual(snapshot.3.count, 500)
            XCTAssertEqual(snapshot.4.count, 10)
            workspaceSnapshotMilliseconds.append(snapshotDuration)
#if DEBUG
            workspaceSnapshotPreparedStatements?.append(
                await store.preparedStatementCountForTesting() - snapshotCountBefore
            )

            let recentCountBefore = await store.preparedStatementCountForTesting()
#endif
            let (recentMessages, recentDuration) = try await elapsedMilliseconds {
                try await store.listMessages(
                    ownerUserID: Self.ownerUserID,
                    roomID: fixture.primaryRoomID,
                    afterUnixMs: nil,
                    limit: 20
                )
            }
            XCTAssertEqual(recentMessages.count, 20)
            recentMessagesMilliseconds.append(recentDuration)
#if DEBUG
            recentMessagesPreparedStatements?.append(
                await store.preparedStatementCountForTesting() - recentCountBefore
            )

            let attachmentCountBefore = await store.preparedStatementCountForTesting()
#endif
            let (attachmentData, attachmentDuration) = try await elapsedMilliseconds {
                let payload = try await store.messageAttachment(
                    ownerUserID: Self.ownerUserID,
                    roomID: fixture.primaryRoomID,
                    messageID: fixture.imageMessageID,
                    attachmentID: fixture.imageAttachmentID
                )
                return try Data(contentsOf: XCTUnwrap(payload).localFileURL)
            }
            XCTAssertEqual(attachmentData, Data(repeating: 0x5A, count: 128 * 1_024))
            imageAttachmentReadMilliseconds.append(attachmentDuration)
#if DEBUG
            imageAttachmentReadPreparedStatements?.append(
                await store.preparedStatementCountForTesting() - attachmentCountBefore
            )
#endif

            let changeService = NativeAgentGroupChatService(databaseURL: fixture.databaseURL)
            let changes = await changeService.changes(
                ownerUserID: Self.ownerUserID,
                roomID: fixture.primaryRoomID
            )
            let firstRunID = UUID()
            let lastRunID = UUID()
            let (_, publishDuration) = await elapsedMilliseconds {
                for index in 0..<500 {
                    await changeService.publishChange(.init(
                        ownerUserID: Self.ownerUserID,
                        roomID: fixture.primaryRoomID,
                        runID: index == 499 ? lastRunID : firstRunID,
                        kind: .runUpdated
                    ))
                }
            }
            publish500RunChangesMilliseconds.append(publishDuration)
            var iterator = changes.makeAsyncIterator()
            var bufferedChanges: [NativeAgentGroupChatChange] = []
            for _ in 0..<64 {
                if let change = await iterator.next() {
                    bufferedChanges.append(change)
                }
            }
            XCTAssertEqual(bufferedChanges.count, 64)
            XCTAssertEqual(bufferedChanges.last?.runID, lastRunID)

#if DEBUG
            let heartbeatStatementCountBefore = await store.preparedStatementCountForTesting()
#endif
            let heartbeatDatabaseChangesBefore = await store.totalDatabaseChangesForTesting()
            let (_, heartbeatPollDuration) = try await elapsedMilliseconds {
                for _ in 0..<500 {
                    let nextDue = try await store.nextAgentHeartbeatDue(
                        ownerUserID: Self.ownerUserID
                    )
                    XCTAssertNil(nextDue)
                }
            }
            idleHeartbeatPoll500Milliseconds.append(heartbeatPollDuration)
#if DEBUG
            idleHeartbeatPollPreparedStatements?.append(
                await store.preparedStatementCountForTesting() - heartbeatStatementCountBefore
            )
#endif
            idleHeartbeatPollDatabaseChanges.append(
                await store.totalDatabaseChangesForTesting() - heartbeatDatabaseChangesBefore
            )

            let idleService = NativeAgentGroupChatService(databaseURL: fixture.databaseURL)
            let idleStore = try await idleService.store()
            let idleScheduler = LocalAgentGroupChatScheduler(
                service: idleService,
                services: IdleSchedulerServices()
            )
#if DEBUG
            let drainStatementCountBefore = await idleStore.preparedStatementCountForTesting()
#endif
            let drainDatabaseChangesBefore = await idleStore.totalDatabaseChangesForTesting()
            let (drainResults, drainDuration) = try await elapsedMilliseconds {
                try await idleScheduler.drainAccount(ownerUserID: Self.ownerUserID)
            }
            XCTAssertTrue(drainResults.isEmpty)
            idleAccountDrainMilliseconds.append(drainDuration)
#if DEBUG
            idleAccountDrainPreparedStatements?.append(
                await idleStore.preparedStatementCountForTesting() - drainStatementCountBefore
            )
#endif
            idleAccountDrainDatabaseChanges.append(
                await idleStore.totalDatabaseChangesForTesting() - drainDatabaseChangesBefore
            )
        }

        // These assertions intentionally freeze the current N+1 baseline. Lower counts are welcome,
        // but must be accompanied by an evidence-backed performance change and an updated snapshot.
#if DEBUG
        XCTAssertEqual(workspaceSnapshotPreparedStatements, [10, 10, 10])
        XCTAssertEqual(recentMessagesPreparedStatements, [4, 4, 4])
        XCTAssertEqual(imageAttachmentReadPreparedStatements, [1, 1, 1])
        XCTAssertEqual(idleHeartbeatPollPreparedStatements, [500, 500, 500])
        XCTAssertEqual(idleAccountDrainPreparedStatements, [1, 1, 1])
#endif
        XCTAssertEqual(idleHeartbeatPollDatabaseChanges, [0, 0, 0])
        XCTAssertEqual(idleAccountDrainDatabaseChanges, [0, 0, 0])

        let measurements = Measurements(
            buildConfiguration: buildConfiguration,
            preparedStatementCountingEnabled: workspaceSnapshotPreparedStatements != nil,
            fixtureAgents: 20,
            fixtureRooms: 10,
            fixtureMessages: 500,
            fixtureRuns: 500,
            fixtureTodos: 100,
            repetitions: repetitions,
            openStoreMilliseconds: openStoreMilliseconds,
            workspaceSnapshotMilliseconds: workspaceSnapshotMilliseconds,
            recentMessagesMilliseconds: recentMessagesMilliseconds,
            imageAttachmentReadMilliseconds: imageAttachmentReadMilliseconds,
            workspaceSnapshotPreparedStatements: workspaceSnapshotPreparedStatements,
            recentMessagesPreparedStatements: recentMessagesPreparedStatements,
            imageAttachmentReadPreparedStatements: imageAttachmentReadPreparedStatements,
            publish500RunChangesMilliseconds: publish500RunChangesMilliseconds,
            idleHeartbeatPoll500Milliseconds: idleHeartbeatPoll500Milliseconds,
            idleHeartbeatPollPreparedStatements: idleHeartbeatPollPreparedStatements,
            idleHeartbeatPollDatabaseChanges: idleHeartbeatPollDatabaseChanges,
            idleAccountDrainMilliseconds: idleAccountDrainMilliseconds,
            idleAccountDrainPreparedStatements: idleAccountDrainPreparedStatements,
            idleAccountDrainDatabaseChanges: idleAccountDrainDatabaseChanges
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        print("CHATOS_AGENT_GROUP_CHAT_BASELINE \(String(decoding: try encoder.encode(measurements), as: UTF8.self))")
    }
}

private enum IdleSchedulerBaselineError: Error {
    case unexpectedRuntimeRequest
}

private struct IdleSchedulerServices: AgentServiceProviding {
    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        throw IdleSchedulerBaselineError.unexpectedRuntimeRequest
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        throw IdleSchedulerBaselineError.unexpectedRuntimeRequest
    }
}

private extension Sequence {
    func asyncMap<T>(_ transform: (Element) async throws -> T) async rethrows -> [T] {
        var values: [T] = []
        values.reserveCapacity(underestimatedCount)
        for element in self {
            values.append(try await transform(element))
        }
        return values
    }
}
