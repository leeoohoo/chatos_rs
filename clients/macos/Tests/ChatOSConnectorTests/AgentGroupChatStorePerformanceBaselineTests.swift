import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

/// An opt-in, repeatable data-volume baseline for the Agent workspace hot path.
///
/// Run explicitly with:
/// `CHATOS_RUN_AGENT_STORE_BASELINE=1 swift test --package-path clients/macos \
///   --filter AgentGroupChatStorePerformanceBaselineTests`
///
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

        for _ in 0..<repetitions {
            let (store, openDuration) = try await elapsedMilliseconds {
                try SQLiteAgentGroupChatStore(databaseURL: fixture.databaseURL)
            }
            openStoreMilliseconds.append(openDuration)

            let (_, snapshotDuration) = try await elapsedMilliseconds {
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
                async let runs = store.listRoomRuns(
                    ownerUserID: Self.ownerUserID,
                    roomID: fixture.primaryRoomID,
                    limit: 500
                )
                async let todos = store.listTeamTodos(
                    ownerUserID: Self.ownerUserID,
                    teamRoomID: fixture.primaryRoomID,
                    includeTerminal: true
                )
                _ = try await (room, members, messages, runs, todos)
            }
            workspaceSnapshotMilliseconds.append(snapshotDuration)

            let (_, recentDuration) = try await elapsedMilliseconds {
                try await store.listMessages(
                    ownerUserID: Self.ownerUserID,
                    roomID: fixture.primaryRoomID,
                    afterUnixMs: nil,
                    limit: 20
                )
            }
            recentMessagesMilliseconds.append(recentDuration)

            let (_, attachmentDuration) = try await elapsedMilliseconds {
                let payload = try await store.messageAttachment(
                    ownerUserID: Self.ownerUserID,
                    roomID: fixture.primaryRoomID,
                    messageID: fixture.imageMessageID,
                    attachmentID: fixture.imageAttachmentID
                )
                _ = try Data(contentsOf: XCTUnwrap(payload).localFileURL)
            }
            imageAttachmentReadMilliseconds.append(attachmentDuration)
        }

        let measurements = Measurements(
            fixtureAgents: 20,
            fixtureRooms: 10,
            fixtureMessages: 500,
            fixtureRuns: 500,
            fixtureTodos: 100,
            repetitions: repetitions,
            openStoreMilliseconds: openStoreMilliseconds,
            workspaceSnapshotMilliseconds: workspaceSnapshotMilliseconds,
            recentMessagesMilliseconds: recentMessagesMilliseconds,
            imageAttachmentReadMilliseconds: imageAttachmentReadMilliseconds
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        print("CHATOS_AGENT_GROUP_CHAT_BASELINE \(String(decoding: try encoder.encode(measurements), as: UTF8.self))")
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
