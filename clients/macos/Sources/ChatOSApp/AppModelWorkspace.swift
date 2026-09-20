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
    func refreshWorkspace() {
        guard let ownerUserID = authenticatedUserID else { return }
        workspaceLoadGeneration += 1
        let generation = workspaceLoadGeneration
        isWorkspaceLoading = true
        workspaceError = nil

        Task {
            do {
                let registry = try await localProjectsService.registry()
                let loader = try ClientOwnedWorkspaceLoader(registry: registry, remote: workspaceService, ownerUserID: ownerUserID)
                let deviceID = try? await localProjectsService.deviceID(ownerUserID: ownerUserID)
                try? await localProjectsService.repairRootWorkspaceBindings(ownerUserID: ownerUserID)
                var local = try await loader.loadLocal(deviceID: deviceID)
                guard generation == workspaceLoadGeneration, ownerUserID == authenticatedUserID else { return }
                local.contacts = workspaceContacts
                local.conversations = workspaceConversations
                await publishWorkspace(local, generation: generation, ownerUserID: ownerUserID)
                guard generation == workspaceLoadGeneration, ownerUserID == authenticatedUserID else { return }
                let result = try await loader.refresh(deviceID: deviceID)
                guard generation == workspaceLoadGeneration, ownerUserID == authenticatedUserID else { return }
                var snapshot = result.snapshot
                if result.remoteError != nil {
                    snapshot.contacts = workspaceContacts
                    snapshot.conversations = workspaceConversations
                }
                await publishWorkspace(snapshot, generation: generation, ownerUserID: ownerUserID)
                guard generation == workspaceLoadGeneration, ownerUserID == authenticatedUserID else { return }
                workspaceError = result.remoteError
            } catch {
                guard generation == workspaceLoadGeneration else { return }
                workspaceError = error.localizedDescription
            }
            if generation == workspaceLoadGeneration {
                isWorkspaceLoading = false
            }
        }
    }

    func publishWorkspace(_ snapshot: WorkspaceSnapshot, generation: Int64, ownerUserID: String) async {
        guard generation == workspaceLoadGeneration, ownerUserID == authenticatedUserID else { return }
        await projectRunService.updateProjects(snapshot.projects)
        guard generation == workspaceLoadGeneration, ownerUserID == authenticatedUserID else { return }
        workspaceProjects = snapshot.projects
        workspaceContacts = snapshot.contacts
        workspaceConversations = snapshot.conversations
        let resources = WorkspaceResourceResolver.resolve(snapshot)
        contacts = resources.contacts
        projects = resources.projects
        reconcileSelection()
    }

    var localProjectCreator: AccountLocalProjectCreator? {
        authenticatedUserID.map { AccountLocalProjectCreator(ownerUserID: $0, service: localProjectsService) }
    }

    var localProjectOwnerUserID: String? { authenticatedUserID }

    func renameLocalProject(id: String, name: String) async throws {
        guard let owner = authenticatedUserID else { throw CancellationError() }
        let registry = try await localProjectsService.registry()
        guard let old = try await registry.get(ownerUserID: owner, id: id) else { throw ProjectRegistryError.notFound }
        guard owner == authenticatedUserID else { throw CancellationError() }
        try await localProjectsService.rename(ownerUserID: owner, id: id, name: name, expectedRevision: old.revision)
        guard owner == authenticatedUserID else { return }
        refreshWorkspace()
    }

    func refreshAllResources() {
        refreshWorkspace()
        refreshRemoteConnections()
        refreshPluginApplications()
        localConnectorControl.refreshStatus()
    }

    func refreshPluginApplications() {
        pluginApplicationsLoadGeneration += 1
        let generation = pluginApplicationsLoadGeneration
        isPluginApplicationsLoading = true
        pluginApplicationsError = nil
        let service = localConnectorService
        Task { [weak self] in
            do {
                let applications = try await service.fetchPluginApplications()
                guard let self, generation == pluginApplicationsLoadGeneration else { return }
                pluginApplications = applications
                reconcilePluginApplicationSelection()
            } catch {
                guard let self, generation == pluginApplicationsLoadGeneration else { return }
                pluginApplicationsError = error.localizedDescription
            }
            guard let self, generation == pluginApplicationsLoadGeneration else { return }
            isPluginApplicationsLoading = false
        }
    }

    func pluginApplication(pluginID: String, componentKey: String) -> LocalConnectorPluginApplication? {
        pluginApplications.first {
            $0.pluginID == pluginID && $0.componentKey == componentKey
        }
    }

    func launchPluginApplication(
        _ application: LocalConnectorPluginApplication,
        context: LocalConnectorPluginApplicationContext? = nil
    ) async throws -> LocalConnectorPluginApplicationLaunch {
        guard let owner = authenticatedUserID else { throw CancellationError() }
        let accountGeneration = workspaceAccountGeneration
        let resolved: LocalConnectorPluginApplicationContext?
        if let projectID = context?.projectID {
            resolved = try await localProjectsService.pluginContext(ownerUserID: owner, projectID: projectID)
        } else {
            resolved = context
        }
        guard owner == authenticatedUserID, accountGeneration == workspaceAccountGeneration else { throw CancellationError() }
        let launch = try await localConnectorService.launchPluginApplication(
            pluginID: application.pluginID, componentKey: application.componentKey, context: resolved,
            expectedOwnerUserID: owner
        )
        guard owner == authenticatedUserID, accountGeneration == workspaceAccountGeneration else { throw CancellationError() }
        return launch
    }

    func reconcilePluginApplicationSelection() {
        guard case let .pluginApplication(pluginID, componentKey) = selection else { return }
        if pluginApplication(pluginID: pluginID, componentKey: componentKey) == nil {
            selection = .applications
        }
    }

    func recoverLocalConnector(forceReconnect: Bool) {
        let service = localConnectorService
        Task { [weak self] in
            await service.recoverGatewayConnection(forceReconnect: forceReconnect)
            try? await Task.sleep(for: .milliseconds(500))
            guard !Task.isCancelled else { return }
            await MainActor.run {
                self?.localConnectorControl.refreshStatus()
            }
        }
    }

}
