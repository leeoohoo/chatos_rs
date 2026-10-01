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
            startLocalAgentHost(ownerUserID: session.user.id)
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
            agentHeartbeatTask?.cancel()
            agentHeartbeatTask = nil
            agentCommunicationTask?.cancel()
            agentCommunicationTask = nil
            agentArtifactStorageTask?.cancel()
            agentArtifactStorageTask = nil
            agentArtifactStorageOwnerUserID = nil
            authenticatedUserID = nil
            localAgentBootstrapTask?.cancel()
            localAgentBootstrapTask = nil
            stopLocalAgentHost()
            languagePreferencesSaveTask?.cancel()
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
        localAgentHostLifecycleTask = Task { [weak self] in
            defer {
                if self?.localAgentHostLifecycleGeneration == generation {
                    self?.localAgentHostLifecycleTask = nil
                }
            }
            do {
                try await localAgentHost.start(ownerUserID: ownerUserID)
                guard !Task.isCancelled, self?.authenticatedUserID == ownerUserID else {
                    await localAgentHost.stop()
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
                await localAgentHost.stop()
            } catch {
                guard self?.authenticatedUserID == ownerUserID else { return }
                self?.localAgentHostError = error.localizedDescription
                if refreshControlPlaneAfterStart {
                    self?.recoverLocalAgentHostAfterUnexpectedExit()
                }
            }
        }
    }

    func recoverLocalAgentHostAfterSystemWake() {
        guard let ownerUserID = authenticatedUserID else { return }
        // A child process or its stdio pipes may not survive system sleep even
        // though the SwiftUI application does. Re-run the complete bootstrap so
        // credentials are sourced again instead of being retained in memory.
        startLocalAgentHost(
            ownerUserID: ownerUserID,
            refreshControlPlaneAfterStart: true
        )
    }

    func recoverLocalAgentHostIfNeeded() {
        guard localAgentHostLifecycleTask == nil,
              let ownerUserID = authenticatedUserID,
              let host = localAgentHost as? NativeLocalAgentHostLifecycle else { return }
        Task { [weak self] in
            guard await !host.isRunning,
                  let self,
                  authenticatedUserID == ownerUserID,
                  localAgentHostLifecycleTask == nil else { return }
            startLocalAgentHost(
                ownerUserID: ownerUserID,
                refreshControlPlaneAfterStart: true
            )
        }
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
        Task {
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
                    try await commandService?.configure(
                        ownerUserID: ownerUserID,
                        bootstrap: bootstrap
                    )
                    await petActivityService?.configure(ownerUserID: ownerUserID)
                    await messageTaskGraphService?.configure(ownerUserID: ownerUserID)
                    await askUserPromptService?.configure(ownerUserID: ownerUserID)
                    await turnProcessService?.configure(ownerUserID: ownerUserID)
                    await platformToolWorker?.configure(ownerUserID: ownerUserID)
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
        isLanguagePreferencesLoading = true
        languagePreferencesError = nil
        let service = userLanguagePreferencesService
        Task { [weak self] in
            do {
                let preferences = try await service.fetch(userID: expectedUserID)
                guard let self, authenticatedUserID == expectedUserID else { return }
                applyLanguagePreferences(preferences)
            } catch {
                self?.languagePreferencesError = error.localizedDescription
            }
            self?.isLanguagePreferencesLoading = false
        }
    }

    func languagePreferenceDidChange() {
        persistLanguagePreferencesLocally()
        guard !isApplyingLanguagePreferences,
              let authenticatedUserID else { return }

        languagePreferencesSaveTask?.cancel()
        let preferences = UserLanguagePreferences(
            interfaceLanguage: interfaceLanguage,
            internalContextLanguage: contextLanguage
        )
        let service = userLanguagePreferencesService
        languagePreferencesSaveTask = Task { [weak self] in
            do {
                try await Task.sleep(for: .milliseconds(250))
                guard !Task.isCancelled else { return }
                self?.isLanguagePreferencesSaving = true
                self?.languagePreferencesError = nil
                let saved = try await service.update(
                    userID: authenticatedUserID,
                    preferences: preferences
                )
                guard !Task.isCancelled else {
                    self?.isLanguagePreferencesSaving = false
                    return
                }
                guard let self else { return }
                applyLanguagePreferences(saved)
            } catch is CancellationError {
                self?.isLanguagePreferencesSaving = false
                return
            } catch {
                self?.languagePreferencesError = error.localizedDescription
            }
            self?.isLanguagePreferencesSaving = false
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
