import ChatOSCore
import Foundation

struct ResolvedWorkspaceResources {
    var contacts: [ResourceItem]
    var projects: [ResourceItem]
}

enum WorkspaceResourceResolver {
    static func resolve(_ snapshot: WorkspaceSnapshot) -> ResolvedWorkspaceResources {
        let conversations = snapshot.conversations.filter { !$0.isArchived }
        let contactByID = Dictionary(uniqueKeysWithValues: snapshot.contacts.map { ($0.id, $0) })

        let contacts = snapshot.contacts.map { contact in
            let conversation = bestConversation(
                conversations.filter {
                    $0.projectID == nil && matches($0, contact: contact)
                }
            )
            return ResourceItem(
                id: contact.id,
                title: contact.name,
                subtitle: conversation?.title ?? statusTitle(contact.status),
                conversationID: conversation?.id,
                contactName: contact.name
            )
        }

        let projects = snapshot.projects.map { project in
            let candidates = conversations.filter { $0.projectID == project.id }
            let latest = project.latestConversationID.flatMap { latestID in
                candidates.first(where: { $0.id == latestID })
            } ?? bestConversation(candidates)
            let contactName = latest.flatMap { conversation in
                resolveContactName(conversation, contacts: snapshot.contacts, byID: contactByID)
            }
            return ResourceItem(
                id: project.id,
                title: project.name,
                subtitle: project.displayRootPath ?? project.rootPath ?? latest?.title,
                conversationID: latest?.id,
                contactName: contactName
            )
        }

        return ResolvedWorkspaceResources(
            contacts: contacts.sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending },
            projects: projects.sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
        )
    }

    private static func matches(
        _ conversation: WorkspaceConversation,
        contact: WorkspaceContact
    ) -> Bool {
        if let contactID = conversation.contactID { return contactID == contact.id }
        return conversation.contactAgentID == contact.agentID
    }

    private static func bestConversation(
        _ conversations: [WorkspaceConversation]
    ) -> WorkspaceConversation? {
        conversations.sorted {
            let leftHasMessages = $0.messageCount > 0
            let rightHasMessages = $1.messageCount > 0
            if leftHasMessages != rightHasMessages { return leftHasMessages }
            return $0.updatedAt > $1.updatedAt
        }.first
    }

    private static func resolveContactName(
        _ conversation: WorkspaceConversation,
        contacts: [WorkspaceContact],
        byID: [String: WorkspaceContact]
    ) -> String? {
        if let contactID = conversation.contactID { return byID[contactID]?.name }
        guard let agentID = conversation.contactAgentID else { return nil }
        let matches = contacts.filter { $0.agentID == agentID }
        return matches.count == 1 ? matches[0].name : nil
    }

    private static func statusTitle(_ status: String?) -> String? {
        switch status?.lowercased() {
        case "active": "已连接"
        case "disabled": "已停用"
        default: nil
        }
    }
}
