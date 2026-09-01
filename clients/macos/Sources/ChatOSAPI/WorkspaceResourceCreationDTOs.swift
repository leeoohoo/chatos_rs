import ChatOSCore
import Foundation

struct CreateLocalProjectRequest: Encodable {
    var name: String
    var deviceID: String
    var workspaceID: String
    var relativePath: String?
    var repositoryMode: String
    var gitURL: String?

    init(draft: LocalProjectCreationDraft) {
        name = draft.name
        deviceID = draft.deviceID
        workspaceID = draft.workspaceID
        relativePath = draft.relativePath
        repositoryMode = draft.repositoryMode.rawValue
        gitURL = draft.gitURL
    }

    enum CodingKeys: String, CodingKey {
        case name
        case deviceID = "device_id"
        case workspaceID = "workspace_id"
        case relativePath = "relative_path"
        case repositoryMode = "repository_mode"
        case gitURL = "git_url"
    }
}

struct BindContactRequest: Encodable {
    var contactID: String

    enum CodingKeys: String, CodingKey {
        case contactID = "contact_id"
    }
}

struct CreatedProjectDTO: Decodable {
    var id: String
    var name: String
    var rootPath: String?
    var displayRootPath: String?
    var latestConversationID: String?

    enum CodingKeys: String, CodingKey {
        case id, name
        case rootPath = "root_path"
        case displayRootPath = "display_root_path"
        case latestConversationID = "latest_session_id"
    }

    var domainModel: WorkspaceProject {
        WorkspaceProject(
            id: id,
            name: name,
            rootPath: rootPath,
            displayRootPath: displayRootPath ?? rootPath,
            latestConversationID: latestConversationID
        )
    }
}

struct ProjectContactLinkDTO: Decodable {
    var contactID: String?
    var latestConversationID: String?

    enum CodingKeys: String, CodingKey {
        case contactID = "contact_id"
        case latestConversationID = "latest_session_id"
    }
}

struct ProjectConversationDTO: Decodable {
    var id: String
    var projectID: String?
    var messageCount: Int?
    var updatedAt: String?
    var metadata: JSONValue?

    enum CodingKeys: String, CodingKey {
        case id, metadata
        case projectID = "project_id"
        case messageCount = "message_count"
        case updatedAt = "updated_at"
    }

    func matches(projectID: String, contact: WorkspaceContact) -> Bool {
        guard self.projectID?.trimmedNonEmpty == projectID else { return false }
        let root = metadataObject
        let source = root.objectValue(for: "source_metadata") ?? root
        let runtime = source.objectValue(for: "chat_runtime") ?? [:]
        let metadataContact = source.objectValue(for: "contact") ?? [:]
        let uiContact = source.objectValue(for: "ui_contact") ?? [:]
        let contactID = metadataContact.stringValue(for: "contact_id", "contactId")
            ?? uiContact.stringValue(for: "contact_id", "contactId")
        if let contactID { return contactID == contact.id }
        let agentID = metadataContact.stringValue(for: "agent_id", "agentId")
            ?? runtime.stringValue(for: "contact_agent_id", "contactAgentId")
            ?? uiContact.stringValue(for: "agent_id", "agentId")
        return agentID == contact.agentID
    }

    static func isPreferred(_ lhs: Self, _ rhs: Self) -> Bool {
        let lhsHasMessages = (lhs.messageCount ?? 0) > 0
        let rhsHasMessages = (rhs.messageCount ?? 0) > 0
        if lhsHasMessages != rhsHasMessages { return lhsHasMessages }
        return (lhs.updatedAt ?? "") > (rhs.updatedAt ?? "")
    }

    private var metadataObject: [String: JSONValue] {
        switch metadata {
        case let .object(value):
            return value
        case let .string(value):
            guard let data = value.data(using: .utf8),
                  let decoded = try? JSONDecoder().decode(JSONValue.self, from: data),
                  case let .object(object) = decoded else { return [:] }
            return object
        default:
            return [:]
        }
    }
}

struct CreateProjectConversationRequest: Encodable {
    var title: String
    var projectID: String
    var metadata: ProjectConversationMetadata

    enum CodingKeys: String, CodingKey {
        case title, metadata
        case projectID = "project_id"
    }
}

struct ProjectConversationMetadata: Encodable {
    var chatRuntime: ChatRuntime
    var contact: ContactIdentity
    var uiChatSelection: UIChatSelection
    var uiContact: ContactIdentity

    init(
        projectID: String,
        projectRoot: String?,
        contactID: String,
        contactAgentID: String
    ) {
        chatRuntime = ChatRuntime(
            projectID: projectID,
            projectRoot: projectRoot,
            contactAgentID: contactAgentID
        )
        contact = ContactIdentity(contactID: contactID, agentID: contactAgentID)
        uiChatSelection = UIChatSelection(selectedAgentID: contactAgentID)
        uiContact = ContactIdentity(contactID: contactID, agentID: contactAgentID)
    }

    enum CodingKeys: String, CodingKey {
        case chatRuntime = "chat_runtime"
        case contact
        case uiChatSelection = "ui_chat_selection"
        case uiContact = "ui_contact"
    }

    struct ChatRuntime: Encodable {
        var projectID: String
        var projectRoot: String?
        var contactAgentID: String

        enum CodingKeys: String, CodingKey {
            case projectID = "project_id"
            case projectRoot = "project_root"
            case contactAgentID = "contact_agent_id"
        }
    }

    struct ContactIdentity: Encodable {
        let type = "memory_agent"
        var contactID: String
        var agentID: String

        enum CodingKeys: String, CodingKey {
            case type
            case contactID = "contact_id"
            case agentID = "agent_id"
        }
    }

    struct UIChatSelection: Encodable {
        var selectedAgentID: String

        enum CodingKeys: String, CodingKey {
            case selectedAgentID = "selected_agent_id"
        }
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}

private extension Dictionary where Key == String, Value == JSONValue {
    func objectValue(for key: String) -> [String: JSONValue]? {
        guard case let .object(value) = self[key] else { return nil }
        return value
    }

    func stringValue(for keys: String...) -> String? {
        for key in keys {
            guard case let .string(value) = self[key] else { continue }
            let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
            if !normalized.isEmpty { return normalized }
        }
        return nil
    }
}
