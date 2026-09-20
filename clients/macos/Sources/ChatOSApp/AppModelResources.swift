import ChatOSAPI
import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import AppKit
import Combine
import Foundation
import SwiftUI

@MainActor
extension AppModel {
    func workspaceProject(id: String) -> WorkspaceProject? {
        workspaceProjects.first(where: { $0.id == id })
    }

    var defaultProjectContact: WorkspaceContact? {
        workspaceContacts.first {
            $0.name.trimmingCharacters(in: .whitespacesAndNewlines) == "叽咕狸"
                && $0.status?.lowercased() != "disabled"
        }
    }

    var petQuickChatResources: [PetQuickChatResource] {
        var resources: [PetQuickChatResource] = []
        let preferredContactID = defaultProjectContact?.id
        if let contact = contacts.first(where: { $0.id == preferredContactID })
            ?? contacts.first(where: {
                $0.title.trimmingCharacters(in: .whitespacesAndNewlines) == "叽咕狸"
            }) {
            resources.append(PetQuickChatResource(
                id: "contact:\(contact.id)",
                sourceID: contact.id,
                kind: .contact,
                title: contact.title,
                subtitle: contact.subtitle,
                conversationID: contact.conversationID
            ))
        }

        resources.append(contentsOf: projects
            .filter { petPreferences.isFavorite(projectID: $0.id) }
            .map { project in
                PetQuickChatResource(
                    id: "project:\(project.id)",
                    sourceID: project.id,
                    kind: .project,
                    title: project.title,
                    subtitle: project.subtitle,
                    conversationID: project.conversationID
                )
            })
        return resources
    }

    func petConversation(for resource: PetQuickChatResource) -> ConversationSessionViewModel? {
        guard let conversationID = resource.conversationID else {
            if resource.kind == .project {
                prepareProjectConversationIfNeeded(projectID: resource.sourceID)
            }
            return nil
        }
        return conversation(for: conversationID)
    }

    func registerCreatedProject(_ project: WorkspaceProject) {
        if let index = workspaceProjects.firstIndex(where: { $0.id == project.id }) {
            workspaceProjects[index] = project
        } else {
            workspaceProjects.append(project)
        }
        let resource = ResourceItem(
            id: project.id,
            title: project.name,
            subtitle: project.displayRootPath ?? project.rootPath,
            conversationID: project.latestConversationID,
            contactName: defaultProjectContact?.name
        )
        if let index = projects.firstIndex(where: { $0.id == project.id }) {
            projects[index] = resource
        } else {
            projects.append(resource)
            projects.sort { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
        }
        projectTab = .directory
        selection = .project(project.id)
        refreshWorkspace()
    }

    func deleteProject(id: String) async throws {
        guard let owner = authenticatedUserID else { throw CancellationError() }
        let registry = try await localProjectsService.registry()
        guard let old = try await registry.get(ownerUserID: owner, id: id) else { throw ProjectRegistryError.notFound }
        guard owner == authenticatedUserID else { throw CancellationError() }
        try await localProjectsService.remove(ownerUserID: owner, id: id, expectedRevision: old.revision)
        guard owner == authenticatedUserID else { return }

        // A workspace refresh may already be in flight. Invalidate it so an
        // older response cannot resurrect the project after deletion.
        workspaceLoadGeneration += 1
        isWorkspaceLoading = false

        let conversationIDs = Set(
            workspaceProjects
                .filter { $0.id == id }
                .compactMap(\.latestConversationID)
        )
        workspaceProjects.removeAll { $0.id == id }
        projects.removeAll { $0.id == id }
        preparingProjectConversationIDs.remove(id)
        projectConversationPreparationErrors.removeValue(forKey: id)
        petPreferences.setFavorite(false, projectID: id)
        for conversationID in conversationIDs {
            conversationCache.removeValue(forKey: conversationID)
        }

        if selection == .project(id) {
            projectConversation = nil
            projectTab = .messages
        }
        await projectRunService.updateProjects(workspaceProjects)
        reconcileSelection()
    }

    func isPreparingProjectConversation(projectID: String) -> Bool {
        preparingProjectConversationIDs.contains(projectID)
    }

    func projectConversationPreparationError(projectID: String) -> String? {
        projectConversationPreparationErrors[projectID]
    }

    func retryProjectConversationPreparation(projectID: String) {
        projectConversationPreparationErrors[projectID] = nil
        prepareProjectConversationIfNeeded(projectID: projectID, force: true)
    }

    func prepareProjectChat(projectID: String) {
        guard projects.first(where: { $0.id == projectID })?.conversationID == nil else { return }
        prepareProjectConversationIfNeeded(projectID: projectID)
    }

    func refreshRemoteConnections() {
        isRemoteConnectionsLoading = true
        remoteConnectionsError = nil
        Task {
            do {
                remoteConnections = try await remoteConnectionService.listConnections()
                    .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
            } catch {
                remoteConnectionsError = error.localizedDescription
            }
            isRemoteConnectionsLoading = false
        }
    }

    func remoteConnection(id: String) -> RemoteConnection? {
        remoteConnections.first(where: { $0.id == id })
    }

    func registerRemoteConnection(_ connection: RemoteConnection) {
        remoteConnectionWorkspaceStore.removeWorkspace(for: connection.id)
        if let index = remoteConnections.firstIndex(where: { $0.id == connection.id }) {
            remoteConnections[index] = connection
        } else {
            remoteConnections.append(connection)
        }
        remoteConnections.sort { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
        selection = .remote(connection.id)
    }

    func deleteRemoteConnection(id: String) async throws {
        try await remoteConnectionService.deleteConnection(id: id)
        remoteConnectionWorkspaceStore.removeWorkspace(for: id)
        remoteConnections.removeAll(where: { $0.id == id })
        if selection == .remote(id) {
            selection = projects.first.map { .project($0.id) }
                ?? contacts.first.map { .contact($0.id) }
        }
    }

    func reconcileSelection() {
        if case let .project(id) = selection, projects.contains(where: { $0.id == id }) { return }
        if case let .contact(id) = selection, contacts.contains(where: { $0.id == id }) { return }
        if case let .remote(id) = selection,
           remoteConnections.contains(where: { $0.id == id }) { return }
        if case let .terminal(id) = selection,
           terminals.contains(where: { $0.id == id }) { return }
        if selection == .localConnector
            || selection == .applications
            || selection == .mediaStudio
            || selection == .agentGroupChat { return }
        selection = projects.first.map { .project($0.id) }
            ?? contacts.first.map { .contact($0.id) }
            ?? remoteConnections.first.map { .remote($0.id) }
            ?? terminals.first.map { .terminal($0.id) }
    }

    func activateConversation(for selection: SidebarSelection?) {
        switch selection {
        case let .project(id):
            let conversationID = projects.first(where: { $0.id == id })?.conversationID
            projectConversation = conversationID.map { conversationID in
                let conversation = conversation(for: conversationID)
                conversation.activate()
                return conversation
            }
            contactConversation = nil
            if projectTab == .messages {
                // ProjectRegistry is the project authority. Reconcile even an existing
                // conversation whenever it becomes active so a client reinstall, connector
                // re-pairing or workspace move cannot leave the server session bound to a stale
                // device/workspace execution target.
                prepareProjectConversationIfNeeded(
                    projectID: id,
                    force: conversationID != nil
                )
            }
        case let .contact(id):
            let conversationID = contacts.first(where: { $0.id == id })?.conversationID
            contactConversation = conversationID.map { conversationID in
                let conversation = conversation(for: conversationID)
                conversation.activate()
                return conversation
            }
            projectConversation = nil
        default:
            projectConversation = nil
            contactConversation = nil
        }
    }

    func prepareProjectConversationIfNeeded(projectID: String, force: Bool = false) {
        guard !preparingProjectConversationIDs.contains(projectID) else { return }
        guard force || projectConversationPreparationErrors[projectID] == nil else { return }
        guard workspaceProject(id: projectID) != nil,
              defaultProjectContact != nil else { return }

        preparingProjectConversationIDs.insert(projectID)
        projectConversationPreparationErrors[projectID] = nil
        Task {
            do {
                _ = try await ensureProjectConversation(projectID: projectID)
            } catch {
                if workspaceProject(id: projectID) != nil {
                    projectConversationPreparationErrors[projectID] = error.localizedDescription
                }
            }
            preparingProjectConversationIDs.remove(projectID)
        }
    }

    func ensureProjectConversation(projectID: String) async throws -> String {
        if let existing = projects.first(where: { $0.id == projectID })?.conversationID {
            return existing
        }
        if let task = projectConversationPreparationTasks[projectID] {
            return try await task.value
        }
        guard let owner = authenticatedUserID,
              var project = workspaceProject(id: projectID),
              let contact = defaultProjectContact else {
            throw LocalConnectorCompanionResourceError.unavailable
        }
        let accountGeneration = workspaceAccountGeneration
        let task = Task { @MainActor [localProjectsService, projectConversationService] in
            project.projectContext = try await localProjectsService.projectContext(
                ownerUserID: owner,
                projectID: projectID
            )
            return try await projectConversationService.ensureConversation(
                project: project,
                contact: contact
            )
        }
        projectConversationPreparationTasks[projectID] = task
        defer { projectConversationPreparationTasks.removeValue(forKey: projectID) }
        let conversationID = try await task.value
        guard owner == authenticatedUserID,
              accountGeneration == workspaceAccountGeneration,
              workspaceProject(id: projectID) != nil else {
            throw CancellationError()
        }
        applyPreparedConversation(
            conversationID,
            projectID: projectID,
            contactName: contact.name
        )
        return conversationID
    }

    func applyPreparedConversation(
        _ conversationID: String,
        projectID: String,
        contactName: String
    ) {
        if let index = workspaceProjects.firstIndex(where: { $0.id == projectID }) {
            workspaceProjects[index].latestConversationID = conversationID
        }
        if let index = projects.firstIndex(where: { $0.id == projectID }) {
            let existing = projects[index]
            projects[index] = ResourceItem(
                id: existing.id,
                title: existing.title,
                subtitle: existing.subtitle,
                conversationID: conversationID,
                contactName: contactName
            )
        }
        projectConversationPreparationErrors[projectID] = nil
        if selection == .project(projectID) {
            let conversation = conversation(for: conversationID)
            conversation.activate()
            projectConversation = conversation
        }
    }

    func conversation(
        for sessionID: String
    ) -> ConversationSessionViewModel {
        if let cached = conversationCache[sessionID] {
            return cached
        }
        let created = ConversationSessionViewModel(
            sessionID: sessionID,
            initialTurns: [],
            historyStore: historyStore,
            remoteService: conversationService,
            realtimeService: realtimeService,
            commandService: commandService,
            turnProcessService: turnProcessService,
            messageTaskGraphService: messageTaskGraphService,
            runtimeSettingsService: runtimeSettingsService,
            askUserPromptService: askUserPromptService
        )
        conversationCache[sessionID] = created
        return created
    }
}
