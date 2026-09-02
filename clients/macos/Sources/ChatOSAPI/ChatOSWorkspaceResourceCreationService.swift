import ChatOSCore
import Foundation

public struct ChatOSWorkspaceResourceCreationService: WorkspaceResourceCreating {
    private static let harnessImportTimeout: TimeInterval = 2 * 60 * 60

    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func createLocalProject(
        _ draft: LocalProjectCreationDraft
    ) async throws -> WorkspaceProject {
        let body = try JSONEncoder().encode(CreateLocalProjectRequest(draft: draft))
        let response: CreatedProjectDTO = try await client.request(
            "/local-connectors/projects",
            method: "POST",
            body: body,
            timeoutInterval: draft.repositoryMode == .managed
                ? Self.harnessImportTimeout
                : nil
        )
        return response.domainModel
    }

    public func bindContact(projectID: String, contactID: String) async throws {
        let body = try JSONEncoder().encode(BindContactRequest(contactID: contactID))
        let _: ProjectContactLinkDTO = try await client.request(
            "/projects/\(projectID.pathEncoded)/contacts",
            method: "POST",
            body: body
        )
    }

    public func ensureConversation(
        project: WorkspaceProject,
        contact: WorkspaceContact
    ) async throws -> String {
        let links: [ProjectContactLinkDTO] = try await client.request(
            "/projects/\(project.id.pathEncoded)/contacts?limit=500&offset=0"
        )
        let matchingLink = links.first(where: { $0.contactID == contact.id })
        if let existingID = matchingLink?.latestConversationID?.nonEmpty {
            return existingID
        }
        if matchingLink == nil {
            try await bindContact(projectID: project.id, contactID: contact.id)
        }

        let conversations: [ProjectConversationDTO] = try await client.request(
            "/conversations?project_id=\(project.id.queryEncoded)&limit=500&offset=0"
        )
        if let existing = conversations
            .filter({ $0.matches(projectID: project.id, contact: contact) })
            .sorted(by: ProjectConversationDTO.isPreferred)
            .first {
            return existing.id
        }

        let request = CreateProjectConversationRequest(
            title: contact.name,
            projectID: project.id,
            metadata: .init(
                projectID: project.id,
                projectRoot: project.rootPath,
                contactID: contact.id,
                contactAgentID: contact.agentID
            )
        )
        let created: ProjectConversationDTO = try await client.request(
            "/conversations",
            method: "POST",
            body: try JSONEncoder().encode(request)
        )
        return created.id
    }
}

private extension String {
    var pathEncoded: String {
        addingPercentEncoding(withAllowedCharacters: .urlPathComponentAllowed) ?? self
    }

    var queryEncoded: String {
        addingPercentEncoding(withAllowedCharacters: .urlQueryValueAllowed) ?? self
    }

    var nonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}

private extension CharacterSet {
    static let urlPathComponentAllowed: CharacterSet = {
        var set = CharacterSet.urlPathAllowed
        set.remove(charactersIn: "/?#")
        return set
    }()

    static let urlQueryValueAllowed: CharacterSet = {
        var set = CharacterSet.urlQueryAllowed
        set.remove(charactersIn: "&=+#?")
        return set
    }()
}
