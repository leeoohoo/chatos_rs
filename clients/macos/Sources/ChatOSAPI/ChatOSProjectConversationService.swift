import ChatOSCore
import Foundation

public struct ChatOSProjectConversationService: ProjectConversationPreparing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func ensureConversation(
        project: WorkspaceProject,
        contact: WorkspaceContact
    ) async throws -> String {
        guard let projectContext = project.projectContext,
              projectContext.projectId == project.id else {
            throw ProjectRegistryError.invalidField("projectContext")
        }
        let conversations: [ProjectConversationDTO] = try await client.request(
            "/conversations?project_id=\(project.id.queryEncoded)&limit=500&offset=0"
        )
        if let existing = conversations
            .filter({ $0.matches(projectID: project.id, contact: contact) })
            .sorted(by: ProjectConversationDTO.isPreferred)
            .first {
            if existing.projectContext != projectContext {
                let metadata = ProjectConversationMetadata(
                    projectID: project.id,
                    projectRoot: project.rootPath,
                    projectContext: projectContext,
                    contactID: contact.id,
                    contactAgentID: contact.agentID
                )
                let _: ProjectConversationDTO = try await client.request(
                    "/conversations/\(existing.id)",
                    method: "PUT",
                    body: try JSONEncoder().encode(UpdateProjectConversationRequest(
                        metadata: try existing.merging(metadata)
                    ))
                )
            }
            return existing.id
        }

        let request = CreateProjectConversationRequest(
            title: contact.name,
            projectID: project.id,
            metadata: .init(
                projectID: project.id,
                projectRoot: project.rootPath,
                projectContext: projectContext,
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
    var queryEncoded: String {
        addingPercentEncoding(withAllowedCharacters: .urlQueryValueAllowed) ?? self
    }
}

private extension CharacterSet {
    static let urlQueryValueAllowed: CharacterSet = {
        var set = CharacterSet.urlQueryAllowed
        set.remove(charactersIn: "&=+#?")
        return set
    }()
}
