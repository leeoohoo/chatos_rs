import ChatOSConnector
import ChatOSCore

@MainActor
func resolveTeamProposalProject(
    ownerUserID: String,
    draft: LocalAgentTeamCreationProposalDraft,
    projectsService: NativeLocalProjectsService
) async throws -> (createdProject: WorkspaceProject?, projectID: String) {
    if let existingProjectID = draft.existingProjectID {
        let registry = try await projectsService.registry()
        guard let project = try await registry.get(ownerUserID: ownerUserID, id: existingProjectID),
              project.status == .active else {
            throw ProjectRegistryError.notFound
        }
        return (nil, project.id)
    }
    if let importedDraft = draft.importedProjectDraft,
       let absolutePath = draft.importedProjectAbsolutePath {
        let project = try await projectsService.createFromExistingDirectory(
            ownerUserID: ownerUserID,
            draft: importedDraft,
            absolutePath: absolutePath
        )
        return (project, project.id)
    }
    if let newProjectName = draft.newProjectName {
        let project = try await projectsService.createInDefaultWorkspace(
            ownerUserID: ownerUserID,
            name: newProjectName,
            description: draft.newProjectDescription,
            projectTypeKey: draft.newProjectTypeKey ?? LocalAgentSkillCatalog.legacyProjectTypeKey
        )
        return (project, project.id)
    }
    throw AgentGroupChatError.conflict
}
