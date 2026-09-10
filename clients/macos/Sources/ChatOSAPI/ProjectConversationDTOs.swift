import ChatOSCore
import Foundation

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

    var projectContext: ProjectContextSnapshot? {
        guard let value = sourceMetadataObject
            .objectValue(for: "chat_runtime")?["project_context"] else { return nil }
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return try? JSONDecoder().decode(ProjectContextSnapshot.self, from: data)
    }

    func merging(_ current: ProjectConversationMetadata) throws -> JSONValue {
        let encoded = try JSONEncoder().encode(current)
        guard case let .object(currentObject) = try JSONDecoder().decode(JSONValue.self, from: encoded) else {
            throw ChatOSAPIError.invalidResponse
        }
        var root = metadataObject
        if case let .object(source)? = root["source_metadata"] {
            root["source_metadata"] = .object(source.merging(currentObject) { _, new in new })
        } else {
            root.merge(currentObject) { _, new in new }
        }
        return .object(root)
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

    private var sourceMetadataObject: [String: JSONValue] {
        metadataObject.objectValue(for: "source_metadata") ?? metadataObject
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
        projectContext: ProjectContextSnapshot,
        contactID: String,
        contactAgentID: String
    ) {
        chatRuntime = ChatRuntime(
            projectID: projectID,
            projectRoot: projectRoot,
            projectContext: projectContext,
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
        var projectContext: ProjectContextSnapshot
        var contactAgentID: String

        enum CodingKeys: String, CodingKey {
            case projectID = "project_id"
            case projectRoot = "project_root"
            case projectContext = "project_context"
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

struct UpdateProjectConversationRequest: Encodable {
    var metadata: JSONValue
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
