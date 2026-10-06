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
    func requestConnectorSettings(_ tab: LocalConnectorControlTab) {
        requestedConnectorSettingsTab = tab
    }

    func consumeConnectorSettingsRequest() {
        requestedConnectorSettingsTab = nil
    }

    func applyAuthenticationPhase(_ phase: AuthenticationViewModel.Phase) {
        switch phase {
        case let .authenticated(session):
            workspaceAccountGeneration += 1
            cancelAccountScopedBackgroundTasks()
            if authenticatedUserID != session.user.id {
                workspaceConversations = []
                workspaceContacts = []
                workspaceProjects = []
                contacts = []
                projects = []
                deactivateAllConversations()
                projectConversation = nil
                contactConversation = nil
                preparingProjectConversationIDs = []
                projectConversationPreparationErrors = [:]
            }
            authenticatedUserID = session.user.id
            // Authentication and Local Connector pairing can be restored in either order.
            // Always bootstrap the local control plane after the Host starts so a connector
            // status value emitted before `authenticatedUserID` was installed cannot leave
            // conversation settings permanently unconfigured.
            startLocalAgentHost(
                ownerUserID: session.user.id,
                refreshControlPlaneAfterStart: true
            )
            restartAgentHeartbeatCoordinator()
            restartAgentArtifactStorageCoordinator()
            mediaStudio.activate(userID: session.user.id)
            loadLanguagePreferences()
            localConnectorControl.activate(
                pairIfNeeded: true,
                expectedOwnerUserID: session.user.id
            )
            refreshPluginApplications()
        case .signedOut:
            workspaceAccountGeneration += 1
            cancelAccountScopedBackgroundTasks()
            agentHeartbeatTask?.cancel()
            agentHeartbeatTask = nil
            agentCommunicationTask?.cancel()
            agentCommunicationTask = nil
            agentExecutorRecoveryTask?.cancel()
            agentExecutorRecoveryTask = nil
            agentArtifactStorageTask?.cancel()
            agentArtifactStorageTask = nil
            agentArtifactStorageOwnerUserID = nil
            authenticatedUserID = nil
            localAgentBootstrapTask?.cancel()
            localAgentBootstrapTask = nil
            stopLocalAgentHost()
            languagePreferencesSaveTask?.cancel()
            languagePreferencesSaveTask = nil
            languagePreferencesSaveGeneration &+= 1
            languagePreferencesLoadTask?.cancel()
            languagePreferencesLoadTask = nil
            languagePreferencesLoadGeneration &+= 1
            isLanguagePreferencesLoading = false
            isLanguagePreferencesSaving = false
            languagePreferencesError = nil
            localConnectorControl.resetForSignedOut()
            mediaStudio.resetForSignedOut()
            workspaceLoadGeneration += 1
            contacts = []
            projects = []
            workspaceProjects = []
            workspaceContacts = []
            workspaceConversations = []
            remoteConnectionsLoadGeneration &+= 1
            remoteConnections = []
            isRemoteConnectionsLoading = false
            remoteConnectionsError = nil
            terminalWorkspace.closeAllTerminals()
            remoteConnectionWorkspaceStore.removeAllWorkspaces()
            pluginApplicationsLoadGeneration += 1
            pluginApplications = []
            isPluginApplicationsLoading = false
            pluginApplicationsError = nil
            deactivateAllConversations()
            projectConversation = nil
            contactConversation = nil
            preparingProjectConversationIDs = []
            projectConversationPreparationErrors = [:]
        case .restoring, .authenticating:
            break
        }
    }

    func prepareForApplicationTermination() {
        cancelAccountScopedBackgroundTasks()
        languagePreferencesLoadTask?.cancel()
        languagePreferencesLoadTask = nil
        languagePreferencesSaveTask?.cancel()
        languagePreferencesSaveTask = nil
        agentArtifactStorageTask?.cancel()
        agentArtifactStorageOwnerUserID = nil
        localConnectorRecoveryTask?.cancel()
        localConnectorRecoveryTask = nil
        localAgentBootstrapTask?.cancel()
        localAgentBootstrapTask = nil
        localAgentCrashRecoveryTask?.cancel()
        localAgentCrashRecoveryTask = nil
        localAgentCrashRecoveryAttempts = 0
        stopLocalAgentHost()
        deactivateAllConversations()
        stopVisualSessionMonitoring()
        globalUtilityCoordinator.stop()
        localConnectorService.terminatePluginApplicationsForHostExit()
        terminalWorkspace.closeAllTerminals()
        remoteConnectionWorkspaceStore.removeAllWorkspaces()
    }

    private func startLocalAgentHost(
        ownerUserID: String,
        refreshControlPlaneAfterStart: Bool = false
    ) {
        localAgentHostLifecycleTask?.cancel()
        localAgentHostLifecycleGeneration &+= 1
        let generation = localAgentHostLifecycleGeneration
        localAgentControlPlaneOwnerUserID = nil
        localAgentControlPlaneBootstrapOwnerUserID = nil
        localAgentHostError = nil
        guard let localAgentHost else { return }
        let localAgentEventHub = localAgentEventHub
        let pendingShutdown = localAgentHostShutdownTask
        localAgentHostLifecycleTask = Task { [weak self] in
            defer {
                if self?.localAgentHostLifecycleGeneration == generation {
                    self?.localAgentHostLifecycleTask = nil
                }
            }
            do {
                await pendingShutdown?.value
                try Task.checkCancellation()
                guard self?.authenticatedUserID == ownerUserID,
                      self?.localAgentHostLifecycleGeneration == generation else { return }
                await localAgentEventHub?.reset()
                try await localAgentHost.start(ownerUserID: ownerUserID)
                guard !Task.isCancelled,
                      self?.authenticatedUserID == ownerUserID,
                      self?.localAgentHostLifecycleGeneration == generation else {
                    if self?.localAgentHostLifecycleGeneration == generation {
                        await localAgentHost.stop()
                    }
                    return
                }
                await self?.workspaceService?.configure(ownerUserID: ownerUserID)
                await self?.projectConversationService?.configure(ownerUserID: ownerUserID)
                await self?.notepadService.configure(ownerUserID: ownerUserID)
                await self?.remoteConnectionMetadataService.configure(ownerUserID: ownerUserID)
                self?.refreshWorkspace()
                self?.refreshRemoteConnections()
                if refreshControlPlaneAfterStart {
                    self?.refreshLocalAgentControlPlane(ownerUserID: ownerUserID)
                }
            } catch is CancellationError {
                guard self?.localAgentHostLifecycleGeneration == generation else { return }
                await localAgentHost.stop()
            } catch {
                guard self?.authenticatedUserID == ownerUserID,
                      self?.localAgentHostLifecycleGeneration == generation else { return }
                self?.localAgentHostError = error.localizedDescription
                if refreshControlPlaneAfterStart {
                    self?.recoverLocalAgentHostAfterUnexpectedExit()
                }
            }
        }
    }

    func recoverLocalAgentHostAfterSystemWake() {
        guard let ownerUserID = authenticatedUserID else { return }
        // A delayed crash retry scheduled before sleep must not restart the freshly recovered
        // process a second time after wake.
        localAgentCrashRecoveryTask?.cancel()
        localAgentCrashRecoveryTask = nil
        localAgentCrashRecoveryAttempts = 0
        // A child process or its stdio pipes may not survive system sleep even
        // though the SwiftUI application does. Re-run the complete bootstrap so
        // credentials are sourced again instead of being retained in memory.
        startLocalAgentHost(
            ownerUserID: ownerUserID,
            refreshControlPlaneAfterStart: true
        )
    }

    func recoverLocalAgentHostIfNeeded() {
        guard localAgentHostHealthCheckTask == nil,
              localAgentHostLifecycleTask == nil,
              let ownerUserID = authenticatedUserID,
              let host = localAgentHost as? NativeLocalAgentHostLifecycle else { return }
        localAgentHostHealthCheckGeneration &+= 1
        let generation = localAgentHostHealthCheckGeneration
        let accountGeneration = workspaceAccountGeneration
        localAgentHostHealthCheckTask = Task { [weak self] in
            defer {
                if self?.localAgentHostHealthCheckGeneration == generation {
                    self?.localAgentHostHealthCheckTask = nil
                }
            }
            let isRunning = await host.isRunning
            guard !Task.isCancelled,
                  let self,
                  authenticatedUserID == ownerUserID,
                  workspaceAccountGeneration == accountGeneration,
                  localAgentHostHealthCheckGeneration == generation,
                  localAgentHostLifecycleTask == nil else { return }
            if !isRunning {
                startLocalAgentHost(
                    ownerUserID: ownerUserID,
                    refreshControlPlaneAfterStart: true
                )
            }
        }
    }

    private func cancelAccountScopedBackgroundTasks() {
        workspaceLoadGeneration += 1
        workspaceLoadTask?.cancel()
        workspaceLoadTask = nil
        isWorkspaceLoading = false
        remoteConnectionsLoadGeneration &+= 1
        remoteConnectionsLoadTask?.cancel()
        remoteConnectionsLoadTask = nil
        isRemoteConnectionsLoading = false
        pluginApplicationsLoadGeneration += 1
        pluginApplicationsLoadTask?.cancel()
        pluginApplicationsLoadTask = nil
        isPluginApplicationsLoading = false
        localConnectorSleepPreparationGeneration &+= 1
        localConnectorSleepPreparationTask?.cancel()
        localConnectorSleepPreparationTask = nil
        projectConversationPreparationTasks.values.forEach { $0.cancel() }
        projectConversationPreparationTasks = [:]
        projectConversationPreparationTaskIDs = [:]
        projectConversationPreparationRequestIDs = [:]
        preparingProjectConversationIDs = []
        localAgentHostHealthCheckGeneration &+= 1
        localAgentHostHealthCheckTask?.cancel()
        localAgentHostHealthCheckTask = nil
        agentRuntimeCoordinatorGeneration &+= 1
        agentHeartbeatTask?.cancel()
        agentHeartbeatTask = nil
        agentCommunicationTask?.cancel()
        agentCommunicationTask = nil
        agentExecutorRecoveryTask?.cancel()
        agentExecutorRecoveryTask = nil
    }

    func recoverLocalAgentHostAfterUnexpectedExit() {
        guard localAgentCrashRecoveryTask == nil,
              localAgentCrashRecoveryAttempts < 5,
              let ownerUserID = authenticatedUserID else {
            if localAgentCrashRecoveryAttempts >= 5 {
                localAgentHostError = "Local Agent Host repeatedly exited and could not recover."
            }
            return
        }
        let delays: [Duration] = [
            .milliseconds(250),
            .seconds(1),
            .seconds(2),
            .seconds(5),
            .seconds(10),
        ]
        let delay = delays[localAgentCrashRecoveryAttempts]
        localAgentCrashRecoveryAttempts += 1
        localAgentCrashRecoveryTask = Task { [weak self] in
            do {
                try await Task.sleep(for: delay)
            } catch {
                return
            }
            guard let self, authenticatedUserID == ownerUserID else { return }
            localAgentCrashRecoveryTask = nil
            startLocalAgentHost(
                ownerUserID: ownerUserID,
                refreshControlPlaneAfterStart: true
            )
        }
    }

    private func stopLocalAgentHost() {
        localAgentCrashRecoveryTask?.cancel()
        localAgentCrashRecoveryTask = nil
        localAgentCrashRecoveryAttempts = 0
        localAgentHostLifecycleGeneration &+= 1
        localAgentHostLifecycleTask?.cancel()
        localAgentHostLifecycleTask = nil
        localAgentHostError = nil
        localAgentControlPlaneOwnerUserID = nil
        localAgentControlPlaneBootstrapOwnerUserID = nil
        guard let localAgentHost else { return }
        let commandService = commandService
        let petActivityService = petActivityService
        let messageTaskGraphService = messageTaskGraphService
        let askUserPromptService = askUserPromptService
        let turnProcessService = turnProcessService
        let runtimeSettingsService = runtimeSettingsService
        let platformToolWorker = platformToolWorker
        let workspaceService = workspaceService
        let projectConversationService = projectConversationService
        let notepadService = notepadService
        let remoteConnectionMetadataService = remoteConnectionMetadataService
        let previousShutdown = localAgentHostShutdownTask
        localAgentHostShutdownGeneration &+= 1
        let shutdownGeneration = localAgentHostShutdownGeneration
        localAgentHostShutdownTask = Task { [weak self] in
            await previousShutdown?.value
            await commandService?.reset()
            await petActivityService?.reset()
            await messageTaskGraphService?.reset()
            await askUserPromptService?.reset()
            await turnProcessService?.reset()
            await runtimeSettingsService?.reset()
            await platformToolWorker?.reset()
            await workspaceService?.reset()
            await projectConversationService?.reset()
            await notepadService.reset()
            await remoteConnectionMetadataService.reset()
            await localAgentHost.stop()
            guard let self,
                  localAgentHostShutdownGeneration == shutdownGeneration else { return }
            localAgentHostShutdownTask = nil
        }
    }

    func refreshLocalAgentControlPlane(ownerUserID: String) {
        guard localAgentControlPlaneOwnerUserID != ownerUserID,
              localAgentControlPlaneBootstrapOwnerUserID != ownerUserID else { return }
        localAgentBootstrapTask?.cancel()
        guard let host = localAgentHost as? NativeLocalAgentHostLifecycle else { return }
        localAgentControlPlaneBootstrapOwnerUserID = ownerUserID
        localAgentBootstrapTask = Task { [weak self] in
            guard let self else { return }
            let retryDelays: [Duration?] = [
                nil,
                .seconds(1),
                .seconds(2),
                .seconds(5),
                .seconds(10),
            ]
            for (attempt, retryDelay) in retryDelays.enumerated() {
                do {
                    if let retryDelay {
                        try await Task.sleep(for: retryDelay)
                    }
                    let memoryAccessToken = await apiClient.currentAccessToken()
                    let bootstrap = try await localConnectorService.bootstrapLocalAgentHost(
                        host,
                        ownerUserID: ownerUserID,
                        memoryAccessToken: memoryAccessToken
                    )
                    guard !Task.isCancelled, authenticatedUserID == ownerUserID else {
                        await host.stop()
                        return
                    }
                    try await runtimeSettingsService?.configure(
                        ownerUserID: ownerUserID,
                        bootstrap: bootstrap
                    )
                    await petActivityService?.configure(ownerUserID: ownerUserID)
                    try await commandService?.configure(
                        ownerUserID: ownerUserID,
                        bootstrap: bootstrap
                    )
                    await messageTaskGraphService?.configure(ownerUserID: ownerUserID)
                    await askUserPromptService?.configure(ownerUserID: ownerUserID)
                    await turnProcessService?.configure(ownerUserID: ownerUserID)
                    try await platformToolWorker?.configure(
                        ownerUserID: ownerUserID,
                        externalMCPConfigs: bootstrap.externalMCPConfigs
                    )
                    conversationCache.values.forEach {
                        $0.localAgentRuntimeDidBecomeReady()
                    }
                    localAgentControlPlaneOwnerUserID = ownerUserID
                    localAgentControlPlaneBootstrapOwnerUserID = nil
                    localAgentCrashRecoveryAttempts = 0
                    localAgentHostError = nil
                    return
                } catch is CancellationError {
                    guard localAgentControlPlaneBootstrapOwnerUserID == ownerUserID else { return }
                    localAgentControlPlaneBootstrapOwnerUserID = nil
                    return
                } catch {
                    guard authenticatedUserID == ownerUserID else { return }
                    guard attempt == retryDelays.indices.last else { continue }
                    localAgentControlPlaneBootstrapOwnerUserID = nil
                    localAgentHostError = error.localizedDescription
                }
            }
        }
    }

    private func deactivateAllConversations() {
        conversationCache.values.forEach { $0.deactivate() }
        conversationCache.removeAll()
        conversationCacheRecency.removeAll()
    }

    func loadLanguagePreferences() {
        guard let expectedUserID = authenticatedUserID else { return }
        languagePreferencesLoadTask?.cancel()
        languagePreferencesLoadGeneration &+= 1
        let generation = languagePreferencesLoadGeneration
        let accountGeneration = workspaceAccountGeneration
        isLanguagePreferencesLoading = true
        languagePreferencesError = nil
        let service = userLanguagePreferencesService
        languagePreferencesLoadTask = Task { [weak self] in
            do {
                let preferences = try await service.fetch(userID: expectedUserID)
                try Task.checkCancellation()
                guard let self,
                      authenticatedUserID == expectedUserID,
                      workspaceAccountGeneration == accountGeneration,
                      languagePreferencesLoadGeneration == generation else { return }
                applyLanguagePreferences(preferences)
            } catch is CancellationError {
                return
            } catch {
                guard let self,
                      authenticatedUserID == expectedUserID,
                      workspaceAccountGeneration == accountGeneration,
                      languagePreferencesLoadGeneration == generation else { return }
                languagePreferencesError = error.localizedDescription
            }
            guard let self,
                  authenticatedUserID == expectedUserID,
                  workspaceAccountGeneration == accountGeneration,
                  languagePreferencesLoadGeneration == generation else { return }
            isLanguagePreferencesLoading = false
            languagePreferencesLoadTask = nil
        }
    }

    func languagePreferenceDidChange() {
        persistLanguagePreferencesLocally()
        guard !isApplyingLanguagePreferences,
              let authenticatedUserID else { return }

        languagePreferencesSaveTask?.cancel()
        languagePreferencesSaveGeneration &+= 1
        let generation = languagePreferencesSaveGeneration
        let accountGeneration = workspaceAccountGeneration
        let preferences = UserLanguagePreferences(
            interfaceLanguage: interfaceLanguage,
            internalContextLanguage: contextLanguage
        )
        let service = userLanguagePreferencesService
        languagePreferencesSaveTask = Task { [weak self] in
            do {
                try await Task.sleep(for: .milliseconds(250))
                guard !Task.isCancelled,
                      let self,
                      self.authenticatedUserID == authenticatedUserID,
                      workspaceAccountGeneration == accountGeneration,
                      languagePreferencesSaveGeneration == generation else { return }
                isLanguagePreferencesSaving = true
                languagePreferencesError = nil
                let saved = try await service.update(
                    userID: authenticatedUserID,
                    preferences: preferences
                )
                try Task.checkCancellation()
                guard self.authenticatedUserID == authenticatedUserID,
                      workspaceAccountGeneration == accountGeneration,
                      languagePreferencesSaveGeneration == generation else { return }
                applyLanguagePreferences(saved)
            } catch is CancellationError {
                return
            } catch {
                guard let self,
                      self.authenticatedUserID == authenticatedUserID,
                      workspaceAccountGeneration == accountGeneration,
                      languagePreferencesSaveGeneration == generation else { return }
                languagePreferencesError = error.localizedDescription
            }
            guard let self,
                  self.authenticatedUserID == authenticatedUserID,
                  workspaceAccountGeneration == accountGeneration,
                  languagePreferencesSaveGeneration == generation else { return }
            isLanguagePreferencesSaving = false
            languagePreferencesSaveTask = nil
        }
    }

    func applyLanguagePreferences(_ preferences: UserLanguagePreferences) {
        isApplyingLanguagePreferences = true
        interfaceLanguage = preferences.interfaceLanguage
        contextLanguage = preferences.internalContextLanguage
        isApplyingLanguagePreferences = false
        persistLanguagePreferencesLocally()
    }

    func persistLanguagePreferencesLocally() {
        UserDefaults.standard.set(
            interfaceLanguage.rawValue,
            forKey: "ChatOS.interfaceLanguage"
        )
        UserDefaults.standard.set(
            contextLanguage.rawValue,
            forKey: "ChatOS.internalContextLanguage"
        )
    }

}
