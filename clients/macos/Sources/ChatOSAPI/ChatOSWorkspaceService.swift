import ChatOSCore
import Foundation

public struct ChatOSWorkspaceService: WorkspaceRemoteServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetchWorkspace() async throws -> WorkspaceSnapshot {
        async let projects: [ProjectDTO] = client.request("/projects")
        async let contacts: [ContactDTO] = client.request("/contacts?limit=500&offset=0")
        async let conversations: [ConversationDTO] = client.request(
            "/conversations?limit=500&offset=0"
        )

        return try await WorkspaceSnapshot(
            projects: projects.map(\.domainModel),
            contacts: contacts.map(\.domainModel),
            conversations: conversations.map(\.domainModel)
        )
    }

    public func deleteProject(id: String) async throws {
        let _: DeleteProjectResponse = try await client.request(
            "/projects/\(id.pathEncoded)",
            method: "DELETE"
        )
    }
}

private struct DeleteProjectResponse: Decodable, Sendable {
    var success: Bool?
    var message: String?
}

private struct ProjectDTO: Decodable, Sendable {
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
            rootPath: rootPath?.nonEmpty ?? displayRootPath?.nonEmpty,
            displayRootPath: displayRootPath?.nonEmpty ?? rootPath?.nonEmpty,
            latestConversationID: latestConversationID?.nonEmpty
        )
    }
}

private struct ContactDTO: Decodable, Sendable {
    var id: String
    var agentID: String
    var name: String?
    var status: String?

    enum CodingKeys: String, CodingKey {
        case id, status
        case agentID = "agent_id"
        case name = "agent_name_snapshot"
    }

    var domainModel: WorkspaceContact {
        WorkspaceContact(
            id: id,
            agentID: agentID,
            name: name?.nonEmpty ?? agentID,
            status: status?.nonEmpty
        )
    }
}

private struct ConversationDTO: Decodable, Sendable {
    var id: String
    var title: String
    var projectID: String?
    var createdAt: String?
    var updatedAt: String?
    var messageCount: Int?
    var archived: Bool?
    var status: String?
    var metadata: JSONValue?

    enum CodingKeys: String, CodingKey {
        case id, title, archived, status, metadata
        case projectID = "project_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case messageCount = "message_count"
    }

    var domainModel: WorkspaceConversation {
        let metadata = metadataObject
        let source = metadata.object(at: "source_metadata") ?? metadata
        let runtime = source.object(at: "chat_runtime") ?? [:]
        let contact = source.object(at: "contact") ?? [:]
        let uiContact = source.object(at: "ui_contact") ?? [:]
        let resolvedProjectID = projectID?.projectScope
            ?? runtime.firstString("project_id", "projectId")?.projectScope
        let resolvedContactID = contact.firstString("contact_id", "contactId")
            ?? uiContact.firstString("contact_id", "contactId")
        let resolvedAgentID = contact.firstString("agent_id", "agentId")
            ?? runtime.firstString("contact_agent_id", "contactAgentId")
            ?? uiContact.firstString("agent_id", "agentId")
        let normalizedStatus = status?.lowercased() ?? ""

        return WorkspaceConversation(
            id: id,
            title: title,
            projectID: resolvedProjectID,
            contactID: resolvedContactID,
            contactAgentID: resolvedAgentID,
            messageCount: max(0, messageCount ?? 0),
            updatedAt: Self.parseDate(updatedAt ?? createdAt) ?? .distantPast,
            isArchived: archived == true || ["archived", "archiving"].contains(normalizedStatus)
        )
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

    private static func parseDate(_ value: String?) -> Date? {
        guard let value else { return nil }
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: value) ?? ISO8601DateFormatter().date(from: value)
    }
}

private extension Dictionary where Key == String, Value == JSONValue {
    func object(at key: String) -> [String: JSONValue]? {
        guard case let .object(value) = self[key] else { return nil }
        return value
    }

    func firstString(_ keys: String...) -> String? {
        for key in keys {
            if let value = self[key]?.stringValue { return value }
        }
        return nil
    }
}

private extension String {
    var pathEncoded: String {
        addingPercentEncoding(withAllowedCharacters: .projectPathComponentAllowed) ?? self
    }

    var nonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }

    var projectScope: String? {
        guard let value = nonEmpty, value != "-1", value != "0" else { return nil }
        return value
    }
}

private extension CharacterSet {
    static let projectPathComponentAllowed: CharacterSet = {
        var set = CharacterSet.urlPathAllowed
        set.remove(charactersIn: "/?#")
        return set
    }()
}
