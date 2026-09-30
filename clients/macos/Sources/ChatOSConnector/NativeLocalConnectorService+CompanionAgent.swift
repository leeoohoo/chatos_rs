import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    func companionAgentStore() async throws -> (
        NativeAgentGroupChatService,
        SQLiteAgentGroupChatStore
    ) {
        guard let service = agentGroupChatService else {
            throw NativeCompanionRelayError.agentRuntimeUnavailable
        }
        do {
            return (service, try await service.store())
        } catch {
            throw NativeCompanionRelayError.agentRuntimeUnavailable
        }
    }

    func companionAgentWorkspace(
        ownerUserID: String,
        store: SQLiteAgentGroupChatStore
    ) async throws -> LocalConnectorCompanionAgentWorkspace {
        let profiles = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: true)
        let activeAgents = profiles.filter { $0.status == .active }.map(Self.companionAgentSummary)
        let teams = try await store.listRooms(ownerUserID: ownerUserID, includeArchived: false)
        let directs = try await store.listDirectConversations(
            ownerUserID: ownerUserID,
            includeArchived: false
        )
        var teamSummaries: [LocalConnectorCompanionAgentConversationSummary] = []
        for room in teams {
            teamSummaries.append(try await companionAgentConversationSummary(
                ownerUserID: ownerUserID,
                room: room,
                store: store
            ))
        }
        var directSummaries: [LocalConnectorCompanionAgentConversationSummary] = []
        for room in directs {
            directSummaries.append(try await companionAgentConversationSummary(
                ownerUserID: ownerUserID,
                room: room,
                store: store
            ))
        }
        return .init(
            teams: teamSummaries.sorted { $0.updatedAtUnixMs > $1.updatedAtUnixMs },
            directConversations: directSummaries.sorted {
                $0.updatedAtUnixMs > $1.updatedAtUnixMs
            },
            agents: activeAgents
        )
    }

    func companionAgentConversationSummary(
        ownerUserID: String,
        room: ProjectAgentRoom,
        store: SQLiteAgentGroupChatStore
    ) async throws -> LocalConnectorCompanionAgentConversationSummary {
        let members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
        let recent = try await store.pageRecentMessages(
            ownerUserID: ownerUserID,
            roomID: room.id,
            beforeMessageID: nil,
            limit: 1
        )
        return .init(
            id: room.id,
            kind: room.conversationKind.rawValue,
            title: room.draft.name,
            goal: room.draft.goal,
            projectID: room.projectID,
            defaultAgentID: room.defaultAgentID,
            memberCount: members.count,
            canSend: room.conversationKind != .agentAgentDirect,
            updatedAtUnixMs: max(room.updatedAtUnixMs, recent.messages.last?.createdAtUnixMs ?? 0),
            lastMessage: recent.messages.last.map(Self.companionAgentMessage)
        )
    }

    func companionAgentConversationDetail(
        ownerUserID: String,
        roomID: String,
        store: SQLiteAgentGroupChatStore
    ) async throws -> LocalConnectorCompanionAgentConversationDetail {
        let normalizedRoomID = roomID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedRoomID.isEmpty else { throw NativeCompanionRelayError.missingRoomID }
        guard let room = try await store.room(
            ownerUserID: ownerUserID,
            roomID: normalizedRoomID
        ), room.status == .active else {
            throw NativeCompanionRelayError.agentResourceNotFound
        }
        let profiles = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: true)
        let profilesByID = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0) })
        let members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
            .compactMap { member -> LocalConnectorCompanionAgentMemberSummary? in
                guard let profile = profilesByID[member.agentID] else { return nil }
                return .init(
                    agent: Self.companionAgentSummary(profile),
                    role: member.draft.role,
                    responsibility: member.draft.responsibility
                )
            }
        return .init(
            conversation: try await companionAgentConversationSummary(
                ownerUserID: ownerUserID,
                room: room,
                store: store
            ),
            members: members
        )
    }

    func companionAgentMessages(
        ownerUserID: String,
        request: CompanionAgentMessagesRequest,
        store: SQLiteAgentGroupChatStore
    ) async throws -> LocalConnectorCompanionAgentMessagePage {
        let roomID = request.roomID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !roomID.isEmpty else { throw NativeCompanionRelayError.missingRoomID }
        guard request.beforeMessageID == nil || request.afterMessageID == nil else {
            throw NativeCompanionRelayError.invalidMessageCursor
        }
        let limit = min(100, max(1, request.limit ?? 40))
        let page: ProjectAgentMessagePage
        if let afterMessageID = request.afterMessageID {
            page = try await store.pageMessages(
                ownerUserID: ownerUserID,
                roomID: roomID,
                afterMessageID: afterMessageID,
                limit: limit
            )
        } else {
            page = try await store.pageRecentMessages(
                ownerUserID: ownerUserID,
                roomID: roomID,
                beforeMessageID: request.beforeMessageID,
                limit: limit
            )
        }
        return .init(
            messages: page.messages.map(Self.companionAgentMessage),
            nextCursorMessageID: page.nextCursorMessageID,
            hasMore: page.hasMore
        )
    }

    nonisolated static func companionAgentSummary(
        _ profile: LocalAgentProfile
    ) -> LocalConnectorCompanionAgentSummary {
        .init(
            id: profile.id,
            name: profile.draft.name,
            description: profile.draft.description,
            professionKey: profile.draft.professionKey,
            status: profile.status.rawValue,
            heartbeatEnabled: profile.draft.heartbeatEnabled,
            lastHeartbeatAtUnixMs: profile.lastHeartbeatAtUnixMs,
            updatedAtUnixMs: profile.updatedAtUnixMs
        )
    }

    nonisolated static func companionAgentMessage(
        _ message: ProjectAgentMessage
    ) -> LocalConnectorCompanionAgentMessage {
        .init(
            id: message.id,
            roomID: message.roomID,
            senderKind: message.senderKind.rawValue,
            senderID: message.senderID,
            content: message.content,
            mentionedAgentIDs: message.mentionedAgentIDs,
            replyToMessageID: message.replyToMessageID,
            createdAtUnixMs: message.createdAtUnixMs,
            attachments: message.attachmentItems.map {
                .init(
                    id: $0.id,
                    name: $0.name,
                    mimeType: $0.mimeType,
                    size: $0.size,
                    kind: $0.kind.rawValue
                )
            }
        )
    }

    func startCompanionAgentScheduler(ownerUserID: String) {
        guard let scheduler = agentGroupChatScheduler else { return }
        Task {
            _ = try? await scheduler.drainAccount(ownerUserID: ownerUserID)
        }
    }
}

struct CompanionAgentConversationRequest: Decodable {
    var roomID: String

    enum CodingKeys: String, CodingKey {
        case roomID = "room_id"
    }
}

struct CompanionAgentMessagesRequest: Decodable {
    var roomID: String
    var beforeMessageID: String?
    var afterMessageID: String?
    var limit: Int?

    enum CodingKeys: String, CodingKey {
        case roomID = "room_id"
        case beforeMessageID = "before_message_id"
        case afterMessageID = "after_message_id"
        case limit
    }
}

struct CompanionAgentSendMessageRequest: Decodable {
    var roomID: String
    var content: String
    var mentionedAgentIDs: [String]
    var clientMessageID: String

    enum CodingKeys: String, CodingKey {
        case roomID = "room_id"
        case content
        case mentionedAgentIDs = "mentioned_agent_ids"
        case clientMessageID = "client_message_id"
    }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        roomID = try values.decode(String.self, forKey: .roomID)
        content = try values.decode(String.self, forKey: .content)
        mentionedAgentIDs = try values.decodeIfPresent(
            [String].self,
            forKey: .mentionedAgentIDs
        ) ?? []
        clientMessageID = try values.decode(String.self, forKey: .clientMessageID)
    }
}

struct CompanionAgentOpenDirectRequest: Decodable {
    var agentID: String

    enum CodingKeys: String, CodingKey {
        case agentID = "agent_id"
    }
}
