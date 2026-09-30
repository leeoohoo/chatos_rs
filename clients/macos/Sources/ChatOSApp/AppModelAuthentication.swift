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
                expectedOwnerUserID: session.user.id,
                onReady: { [weak self] _ in
                    self?.refreshLocalAgentControlPlane(ownerUserID: session.user.id)
                }
            )
            refreshRemoteConnections()
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
            remoteConnections = []
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
        stopLocalAgentHost()
        deactivateAllConversations()
        stopVisualSessionMonitoring()
        globalUtilityCoordinator.stop()
        localConnectorService.terminatePluginApplicationsForHostExit()
        terminalWorkspace.closeAllTerminals()
        remoteConnectionWorkspaceStore.removeAllWorkspaces()
    }

    private func startLocalAgentHost(ownerUserID: String) {
        localAgentHostLifecycleTask?.cancel()
        localAgentHostError = nil
        guard let localAgentHost else { return }
        localAgentHostLifecycleTask = Task { [weak self] in
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
            } catch is CancellationError {
                await localAgentHost.stop()
            } catch {
                guard self?.authenticatedUserID == ownerUserID else { return }
                self?.localAgentHostError = error.localizedDescription
            }
        }
    }

    private func stopLocalAgentHost() {
        localAgentHostLifecycleTask?.cancel()
        localAgentHostLifecycleTask = nil
        localAgentHostError = nil
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

    private func refreshLocalAgentControlPlane(ownerUserID: String) {
        localAgentBootstrapTask?.cancel()
        guard let host = localAgentHost as? NativeLocalAgentHostLifecycle else { return }
        localAgentBootstrapTask = Task { [weak self] in
            do {
                let memoryAccessToken = await self?.apiClient.currentAccessToken()
                guard let bootstrap = try await self?.localConnectorService.bootstrapLocalAgentHost(
                    host,
                    ownerUserID: ownerUserID,
                    memoryAccessToken: memoryAccessToken
                ) else { return }
                guard !Task.isCancelled, self?.authenticatedUserID == ownerUserID else {
                    await host.stop()
                    return
                }
                try await self?.runtimeSettingsService?.configure(
                    ownerUserID: ownerUserID,
                    bootstrap: bootstrap
                )
                try await self?.commandService?.configure(
                    ownerUserID: ownerUserID,
                    bootstrap: bootstrap
                )
                await self?.petActivityService?.configure(ownerUserID: ownerUserID)
                await self?.messageTaskGraphService?.configure(ownerUserID: ownerUserID)
                await self?.askUserPromptService?.configure(ownerUserID: ownerUserID)
                await self?.turnProcessService?.configure(ownerUserID: ownerUserID)
                await self?.platformToolWorker?.configure(ownerUserID: ownerUserID)
                self?.localAgentHostError = nil
            } catch is CancellationError {
            } catch {
                guard self?.authenticatedUserID == ownerUserID else { return }
                self?.localAgentHostError = error.localizedDescription
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
