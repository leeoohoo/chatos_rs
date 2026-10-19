import ChatOSCore
import Foundation
import Testing

@MainActor
struct PrepareProjectConversationUseCaseTests {
    @Test
    func concurrentRequestsShareContextAndConversationPreparation() async throws {
        let context = try projectContext(projectID: "project-1")
        let contextLoader = ProjectContextLoaderStub(context: context)
        let preparer = ProjectConversationPreparerStub(conversationID: "conversation-1")
        let useCase = PrepareProjectConversationUseCase(
            contextLoader: contextLoader,
            projectConversationPort: preparer
        )
        let project = WorkspaceProject(
            id: "project-1",
            name: "Project",
            rootPath: "file:///project",
            latestConversationID: nil
        )
        let contact = WorkspaceContact(
            id: "contact-1",
            agentID: "agent-1",
            name: "Jiguli",
            status: "online"
        )

        async let first = useCase.execute(
            ownerUserID: "owner-1",
            project: project,
            contact: contact
        )
        async let second = useCase.execute(
            ownerUserID: "owner-1",
            project: project,
            contact: contact
        )

        #expect(try await [first, second] == ["conversation-1", "conversation-1"])
        #expect(await contextLoader.requestCount == 1)
        #expect(await preparer.requestCount == 1)
        #expect(await preparer.lastProject?.projectContext == context)
    }

    @Test
    func differentOwnersNeverShareAnInFlightPreparation() async throws {
        let contextLoader = ProjectContextLoaderStub(
            context: try projectContext(projectID: "project-1")
        )
        let preparer = ProjectConversationPreparerStub(conversationID: "conversation-1")
        let useCase = PrepareProjectConversationUseCase(
            contextLoader: contextLoader,
            projectConversationPort: preparer
        )
        let project = WorkspaceProject(
            id: "project-1",
            name: "Project",
            rootPath: nil,
            latestConversationID: nil
        )
        let contact = WorkspaceContact(
            id: "contact-1",
            agentID: "agent-1",
            name: "Jiguli",
            status: nil
        )

        async let first = useCase.execute(
            ownerUserID: "owner-1",
            project: project,
            contact: contact
        )
        async let second = useCase.execute(
            ownerUserID: "owner-2",
            project: project,
            contact: contact
        )
        _ = try await [first, second]

        #expect(await contextLoader.requestCount == 2)
        #expect(await preparer.requestCount == 2)
    }

    private func projectContext(projectID: String) throws -> ProjectContextSnapshot {
        try ProjectContextSnapshot(
            record: LocalProjectRecord(
                id: projectID,
                ownerUserID: "owner-1",
                draft: LocalProjectDraft(
                    name: "Project",
                    workspaceID: "workspace-1"
                ),
                createdAtUnixMs: 1,
                updatedAtUnixMs: 1
            ),
            deviceID: "device-1"
        )
    }
}

private actor ProjectContextLoaderStub: ProjectContextLoading {
    let context: ProjectContextSnapshot
    private(set) var requestCount = 0

    init(context: ProjectContextSnapshot) {
        self.context = context
    }

    func projectContext(
        ownerUserID: String,
        projectID: String
    ) async throws -> ProjectContextSnapshot {
        requestCount += 1
        try await Task.sleep(for: .milliseconds(20))
        return context
    }
}

private actor ProjectConversationPreparerStub: ProjectConversationPort {
    let conversationID: String
    private(set) var requestCount = 0
    private(set) var lastProject: WorkspaceProject?

    init(conversationID: String) {
        self.conversationID = conversationID
    }

    func ensureConversation(
        project: WorkspaceProject,
        contact: WorkspaceContact
    ) async throws -> String {
        requestCount += 1
        lastProject = project
        return conversationID
    }
}
