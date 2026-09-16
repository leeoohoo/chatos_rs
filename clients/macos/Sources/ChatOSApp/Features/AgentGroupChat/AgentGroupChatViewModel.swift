import ChatOSConnector
import ChatOSCore
import Combine
import Foundation

@MainActor
final class AgentGroupChatViewModel: ObservableObject {
    struct MemberPresentation: Identifiable {
        var id: String { member.agentID }
        let member: ProjectAgentRoomMember
        let profile: LocalAgentProfile?
    }

    let projectID: String
    let ownerUserID: String

    @Published private(set) var room: ProjectAgentRoom?
    @Published private(set) var agents: [LocalAgentProfile] = []
    @Published private(set) var members: [ProjectAgentRoomMember] = []
    @Published private(set) var messages: [ProjectAgentMessage] = []
    @Published var draftMessage = ""
    @Published var selectedMentionAgentIDs: Set<String> = []
    @Published private(set) var isLoading = false
    @Published private(set) var isSending = false
    @Published var errorMessage: String?

    private let service: NativeAgentGroupChatService
    private var openedStore: SQLiteAgentGroupChatStore?

    init(projectID: String, ownerUserID: String, service: NativeAgentGroupChatService) {
        self.projectID = projectID
        self.ownerUserID = ownerUserID
        self.service = service
    }

    var profilesByID: [String: LocalAgentProfile] {
        Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0) })
    }

    var activeMembers: [MemberPresentation] {
        let profiles = profilesByID
        return members.map { MemberPresentation(member: $0, profile: profiles[$0.agentID]) }
    }

    func displayName(senderID: String, kind: ProjectAgentMessageSenderKind) -> String {
        switch kind {
        case .human: "你"
        case .system: "系统"
        case .agent: profilesByID[senderID]?.draft.name ?? senderID
        }
    }

    func load() async {
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let store = try await resolveStore()
            let agents = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: false)
            let room = try await store.activeRoom(ownerUserID: ownerUserID, projectID: projectID)
            let members: [ProjectAgentRoomMember]
            let messages: [ProjectAgentMessage]
            if let room {
                members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
                messages = try await store.listMessages(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    afterUnixMs: nil,
                    limit: 500
                )
            } else {
                members = []
                messages = []
            }
            self.agents = agents
            self.room = room
            self.members = members
            self.messages = messages
            selectedMentionAgentIDs.formIntersection(Set(members.map(\.agentID)))
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func createRoom(name: String, goal: String) async -> Bool {
        do {
            let store = try await resolveStore()
            _ = try await store.createRoom(
                ownerUserID: ownerUserID,
                projectID: projectID,
                draft: .init(
                    name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                    goal: goal.trimmingCharacters(in: .whitespacesAndNewlines)
                )
            )
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func createAgentAndJoin(
        name: String,
        role: String,
        responsibility: String,
        rolePrompt: String,
        modelConfigID: String
    ) async -> Bool {
        guard let room else {
            errorMessage = AgentGroupChatError.notFound.localizedDescription
            return false
        }
        do {
            let store = try await resolveStore()
            let agent = try await store.createAgent(
                ownerUserID: ownerUserID,
                draft: .init(
                    name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                    description: responsibility.trimmingCharacters(in: .whitespacesAndNewlines),
                    rolePrompt: rolePrompt.trimmingCharacters(in: .whitespacesAndNewlines),
                    modelConfigID: modelConfigID.trimmingCharacters(in: .whitespacesAndNewlines)
                )
            )
            _ = try await store.addMember(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: agent.id,
                draft: .init(
                    role: role.trimmingCharacters(in: .whitespacesAndNewlines),
                    responsibility: responsibility.trimmingCharacters(in: .whitespacesAndNewlines)
                )
            )
            if members.isEmpty {
                _ = try await store.setDefaultAgent(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    agentID: agent.id
                )
            }
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func sendMessage() async {
        guard let room, !isSending else { return }
        let content = draftMessage.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !content.isEmpty else { return }
        isSending = true
        defer { isSending = false }
        do {
            let store = try await resolveStore()
            _ = try await store.postMessage(
                ownerUserID: ownerUserID,
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: ownerUserID,
                    content: content,
                    mentionedAgentIDs: selectedMentionAgentIDs.sorted()
                ),
                limits: .init()
            )
            draftMessage = ""
            selectedMentionAgentIDs.removeAll()
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func toggleMention(agentID: String) {
        if selectedMentionAgentIDs.contains(agentID) {
            selectedMentionAgentIDs.remove(agentID)
        } else {
            selectedMentionAgentIDs.insert(agentID)
        }
    }

    private func resolveStore() async throws -> SQLiteAgentGroupChatStore {
        if let openedStore { return openedStore }
        let store = try await service.store()
        openedStore = store
        return store
    }
}
