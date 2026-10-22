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
        let accountGeneration = workspaceAccountGeneration
        let token = workspaceLoadState.begin(
            ownerUserID: ownerUserID,
            accountGeneration: accountGeneration
        )
        isWorkspaceLoading = true
        workspaceError = nil

        let localProjectsService = localProjectsService
        let workspaceService = workspaceService
        let projectRunService = projectRunService
        workspaceLoadTask = Task { [weak self] in
            do {
                guard let workspaceService else {
                    throw LocalConnectorCompanionResourceError.unavailable
                }
                let registry = try await localProjectsService.registry()
                try Task.checkCancellation()
                let loader = try ClientOwnedWorkspaceLoader(registry: registry, remote: workspaceService, ownerUserID: ownerUserID)
                // Publish the SQLite registry before asking the Connector actor for pairing
                // metadata. Its network connection or reconnect work must never block the
                // client-owned project list.
                let local = try await loader.loadLocal(deviceID: nil)
                try Task.checkCancellation()
                // This first registry-only snapshot intentionally has no device metadata yet.
                // Keep any values already resolved by project creation or a previous refresh so
                // the UI does not briefly lose its directory/conversation while pairing loads.
                guard let localSnapshot = self?.prepareLocalWorkspaceSnapshot(
                    local,
                    token: token
                ) else { return }
                await projectRunService.updateProjects(localSnapshot.projects)
                try Task.checkCancellation()
                guard self?.applyWorkspaceSnapshot(localSnapshot, token: token) == true else { return }
                guard self?.workspaceLoadCanRefreshRemote(token) == true else {
                    self?.finishWorkspaceLoad(token)
                    return
                }
                let deviceID = try? await localProjectsService.deviceID(ownerUserID: ownerUserID)
                try Task.checkCancellation()
                guard self?.workspaceLoadAccepts(token) == true else { return }
                // Never hide durable projects behind filesystem/grant repair. A stale or
                // unavailable workspace can make path reauthorization slow, while the registry
                // itself remains immediately useful for navigation and project management.
                try? await localProjectsService.repairRootWorkspaceBindings(ownerUserID: ownerUserID)
                try Task.checkCancellation()
                guard self?.workspaceLoadAccepts(token) == true else { return }
                let result = try await loader.refresh(deviceID: deviceID)
                try Task.checkCancellation()
                guard let snapshot = self?.prepareRemoteWorkspaceSnapshot(
                    result.snapshot,
                    preservingRelations: result.remoteError != nil,
                    token: token
                ) else { return }
                await projectRunService.updateProjects(snapshot.projects)
                try Task.checkCancellation()
                guard self?.applyWorkspaceSnapshot(snapshot, token: token) == true else { return }
                self?.finishWorkspaceLoad(token, error: result.remoteError)
            } catch is CancellationError {
                self?.finishWorkspaceLoad(token)
            } catch {
                self?.finishWorkspaceLoad(token, error: error.localizedDescription)
            }
        }
    }

    private func workspaceLoadAccepts(_ token: WorkspaceLoadToken) -> Bool {
        workspaceLoadState.accepts(
            token,
            ownerUserID: authenticatedUserID,
            accountGeneration: workspaceAccountGeneration
        )
    }

    private func workspaceLoadCanRefreshRemote(_ token: WorkspaceLoadToken) -> Bool {
        workspaceLoadAccepts(token)
            && localAgentWorkspaceOwnerUserID == token.ownerUserID
    }

    private func prepareLocalWorkspaceSnapshot(
        _ snapshot: WorkspaceSnapshot,
        token: WorkspaceLoadToken
    ) -> WorkspaceSnapshot? {
        guard workspaceLoadAccepts(token) else { return nil }
        var snapshot = snapshot
        snapshot.preserveUnavailableProjectMetadata(from: workspaceProjects)
        snapshot.contacts = workspaceContacts
        snapshot.conversations = workspaceConversations
        return snapshot
    }

    private func prepareRemoteWorkspaceSnapshot(
        _ snapshot: WorkspaceSnapshot,
        preservingRelations: Bool,
        token: WorkspaceLoadToken
    ) -> WorkspaceSnapshot? {
        guard workspaceLoadAccepts(token) else { return nil }
        guard preservingRelations else { return snapshot }
        var snapshot = snapshot
        snapshot.contacts = workspaceContacts
        snapshot.conversations = workspaceConversations
        return snapshot
    }

    @discardableResult
    private func applyWorkspaceSnapshot(
        _ snapshot: WorkspaceSnapshot,
        token: WorkspaceLoadToken
    ) -> Bool {
        guard workspaceLoadAccepts(token) else { return false }
        workspaceProjects = snapshot.projects
        workspaceContacts = snapshot.contacts
        workspaceConversations = snapshot.conversations
        let resources = WorkspaceResourceResolver.resolve(snapshot)
        contacts = resources.contacts
        projects = resources.projects
        reconcileSelection()
        attachSelectedConversationIfNeeded()
        return true
    }

    private func finishWorkspaceLoad(
        _ token: WorkspaceLoadToken,
        error: String? = nil
    ) {
        guard workspaceLoadAccepts(token), workspaceLoadState.finish(token) else { return }
        workspaceError = error
        isWorkspaceLoading = false
        workspaceLoadTask = nil
    }

    private func attachSelectedConversationIfNeeded() {
        // A valid selection does not emit a new Combine value. Attach a conversation that only
        // became available in the authoritative snapshot, without reactivating an unchanged
        // session during both phases of every workspace refresh.
        switch selection {
        case let .project(id):
            let conversationID = projects.first(where: { $0.id == id })?.conversationID
            if projectConversation?.sessionID != conversationID {
                activateConversation(for: selection)
            } else if conversationID == nil {
                prepareProjectConversationIfNeeded(projectID: id)
            }
        case let .contact(id):
            let conversationID = contacts.first(where: { $0.id == id })?.conversationID
            if contactConversation?.sessionID != conversationID {
                activateConversation(for: selection)
            }
        default:
            if projectConversation != nil || contactConversation != nil {
                activateConversation(for: selection)
            }
        }
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
            pluginApplicationsLoadState.reset()
            pluginApplicationsLoadTask?.cancel()
            pluginApplicationsLoadTask = nil
            isPluginApplicationsLoading = false
            return
        }
        pluginApplicationsLoadTask?.cancel()
        let accountGeneration = workspaceAccountGeneration
        let token = pluginApplicationsLoadState.begin(
            ownerUserID: expectedOwnerUserID,
            accountGeneration: accountGeneration
        )
        isPluginApplicationsLoading = true
        pluginApplicationsError = nil
        let useCase = pluginApplicationListUseCase
        pluginApplicationsLoadTask = Task { [weak self] in
            let result: Result<[LocalConnectorPluginApplication], Error>
            do {
                result = .success(try await useCase.load())
            } catch is CancellationError {
                return
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.pluginApplicationsLoadState.accepts(
                    token,
                    ownerUserID: self.authenticatedUserID,
                    accountGeneration: self.workspaceAccountGeneration
                  ) else { return }
            switch result {
            case let .success(applications):
                self.pluginApplications = applications
                self.reconcilePluginApplicationSelection()
            case let .failure(error):
                self.pluginApplicationsError = error.localizedDescription
            }
            guard self.pluginApplicationsLoadState.finish(token) else { return }
            self.isPluginApplicationsLoading = false
            self.pluginApplicationsLoadTask = nil
        }
    }

    func pluginApplication(pluginID: String, componentKey: String) -> LocalConnectorPluginApplication? {
        pluginApplications.first {
            $0.pluginID == pluginID && $0.componentKey == componentKey
        }
    }

    func makePluginApplicationHostViewModel(
        application: LocalConnectorPluginApplication
    ) -> PluginApplicationHostViewModel? {
        guard let ownerUserID = authenticatedUserID else { return nil }
        return PluginApplicationHostViewModel(
            application: application,
            ownerUserID: ownerUserID,
            accountGeneration: workspaceAccountGeneration,
            useCase: PluginApplicationLaunchUseCase(
                contextProvider: localProjectsService,
                launcher: localConnectorService
            )
        )
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

extension WorkspaceSnapshot {
    mutating func preserveUnavailableProjectMetadata(from existingProjects: [WorkspaceProject]) {
        let existingByID = Dictionary(uniqueKeysWithValues: existingProjects.map { ($0.id, $0) })
        projects = projects.map { project in
            guard let existing = existingByID[project.id] else { return project }
            var merged = project
            merged.rootPath = project.rootPath ?? existing.rootPath
            merged.displayRootPath = project.displayRootPath ?? existing.displayRootPath
            merged.latestConversationID = project.latestConversationID ?? existing.latestConversationID
            merged.projectContext = project.projectContext ?? existing.projectContext
            return merged
        }
    }
}
