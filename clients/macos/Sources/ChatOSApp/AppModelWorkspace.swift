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
        workspaceLoadTask?.cancel()
        workspaceLoadGeneration += 1
        let generation = workspaceLoadGeneration
        isWorkspaceLoading = true
        workspaceError = nil

        workspaceLoadTask = Task { [weak self] in
            guard let self else { return }
            do {
                guard let workspaceService else {
                    throw LocalConnectorCompanionResourceError.unavailable
                }
                let registry = try await localProjectsService.registry()
                try Task.checkCancellation()
                let loader = try ClientOwnedWorkspaceLoader(registry: registry, remote: workspaceService, ownerUserID: ownerUserID)
                let deviceID = try? await localProjectsService.deviceID(ownerUserID: ownerUserID)
                try Task.checkCancellation()
                try? await localProjectsService.repairRootWorkspaceBindings(ownerUserID: ownerUserID)
                try Task.checkCancellation()
                var local = try await loader.loadLocal(deviceID: deviceID)
                try Task.checkCancellation()
                guard generation == workspaceLoadGeneration, ownerUserID == authenticatedUserID else { return }
                local.contacts = workspaceContacts
                local.conversations = workspaceConversations
                await publishWorkspace(local, generation: generation, ownerUserID: ownerUserID)
                try Task.checkCancellation()
                guard generation == workspaceLoadGeneration, ownerUserID == authenticatedUserID else { return }
                let result = try await loader.refresh(deviceID: deviceID)
                try Task.checkCancellation()
                guard generation == workspaceLoadGeneration, ownerUserID == authenticatedUserID else { return }
                var snapshot = result.snapshot
                if result.remoteError != nil {
                    snapshot.contacts = workspaceContacts
                    snapshot.conversations = workspaceConversations
                }
                await publishWorkspace(snapshot, generation: generation, ownerUserID: ownerUserID)
                try Task.checkCancellation()
                guard generation == workspaceLoadGeneration, ownerUserID == authenticatedUserID else { return }
                workspaceError = result.remoteError
            } catch is CancellationError {
                return
            } catch {
                guard generation == workspaceLoadGeneration,
                      ownerUserID == authenticatedUserID else { return }
                workspaceError = error.localizedDescription
            }
            guard generation == workspaceLoadGeneration,
                  ownerUserID == authenticatedUserID else { return }
            isWorkspaceLoading = false
            workspaceLoadTask = nil
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
        guard let expectedOwnerUserID = authenticatedUserID else {
            pluginApplicationsLoadTask?.cancel()
            pluginApplicationsLoadTask = nil
            isPluginApplicationsLoading = false
            return
        }
        pluginApplicationsLoadTask?.cancel()
        pluginApplicationsLoadGeneration += 1
        let generation = pluginApplicationsLoadGeneration
        let accountGeneration = workspaceAccountGeneration
        isPluginApplicationsLoading = true
        pluginApplicationsError = nil
        let service = localConnectorService
        pluginApplicationsLoadTask = Task { [weak self] in
            do {
                let applications = try await service.fetchPluginApplications()
                try Task.checkCancellation()
                guard let self,
                      generation == pluginApplicationsLoadGeneration,
                      accountGeneration == workspaceAccountGeneration,
                      expectedOwnerUserID == authenticatedUserID else { return }
                pluginApplications = applications
                reconcilePluginApplicationSelection()
            } catch is CancellationError {
                return
            } catch {
                guard let self,
                      generation == pluginApplicationsLoadGeneration,
                      accountGeneration == workspaceAccountGeneration,
                      expectedOwnerUserID == authenticatedUserID else { return }
                pluginApplicationsError = error.localizedDescription
            }
            guard let self,
                  generation == pluginApplicationsLoadGeneration,
                  accountGeneration == workspaceAccountGeneration,
                  expectedOwnerUserID == authenticatedUserID else { return }
            isPluginApplicationsLoading = false
            pluginApplicationsLoadTask = nil
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
        localConnectorSleepPreparationGeneration &+= 1
        localConnectorSleepPreparationTask?.cancel()
        localConnectorSleepPreparationTask = nil
        let now = Date()
        let secondsSinceLastRecovery = lastLocalConnectorRecoveryDate.map {
            now.timeIntervalSince($0)
        }
        let hasLiveTask = localConnectorRecoveryTask.map { !$0.isCancelled } ?? false
        guard LocalConnectorRecoveryPolicy.shouldStart(
            forceReconnect: forceReconnect,
            hasLiveTask: hasLiveTask,
            secondsSinceLastRecovery: secondsSinceLastRecovery
        ) else { return }

        localConnectorRecoveryTask?.cancel()
        localConnectorRecoveryGeneration += 1
        let generation = localConnectorRecoveryGeneration
        lastLocalConnectorRecoveryDate = now
        let service = localConnectorService
        localConnectorRecoveryTask = Task { [weak self] in
            await service.recoverGatewayConnection(forceReconnect: forceReconnect)
            do {
                try await Task.sleep(for: .milliseconds(500))
            } catch {
                guard self?.localConnectorRecoveryGeneration == generation else { return }
                self?.localConnectorRecoveryTask = nil
                return
            }
            guard !Task.isCancelled,
                  self?.localConnectorRecoveryGeneration == generation else { return }
            self?.localConnectorControl.refreshStatus()
            self?.localConnectorRecoveryTask = nil
        }
    }

    func prepareLocalConnectorForSystemSleep() {
        localConnectorSleepPreparationTask?.cancel()
        localConnectorSleepPreparationGeneration &+= 1
        let generation = localConnectorSleepPreparationGeneration
        let service = localConnectorService
        localConnectorSleepPreparationTask = Task { [weak self] in
            await service.prepareForSystemSleep()
            guard !Task.isCancelled,
                  let self,
                  localConnectorSleepPreparationGeneration == generation else { return }
            localConnectorSleepPreparationTask = nil
        }
    }

}

enum LocalConnectorRecoveryPolicy {
    static let minimumRecoveryInterval: TimeInterval = 60

    static func shouldStart(
        forceReconnect: Bool,
        hasLiveTask: Bool,
        secondsSinceLastRecovery: TimeInterval?
    ) -> Bool {
        if forceReconnect { return true }
        if hasLiveTask { return false }
        guard let secondsSinceLastRecovery else { return true }
        return secondsSinceLastRecovery >= minimumRecoveryInterval
    }
}
