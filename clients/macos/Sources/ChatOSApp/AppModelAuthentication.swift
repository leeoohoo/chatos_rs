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
            restartAgentHeartbeatCoordinator()
            restartAgentArtifactSyncCoordinator()
            mediaStudio.activate(userID: session.user.id)
            loadLanguagePreferences()
            localConnectorControl.activate(
                pairIfNeeded: true,
                expectedOwnerUserID: session.user.id
            )
            refreshWorkspace()
            refreshRemoteConnections()
            refreshPluginApplications()
        case .signedOut:
            workspaceAccountGeneration += 1
            agentHeartbeatTask?.cancel()
            agentHeartbeatTask = nil
            agentArtifactSyncTask?.cancel()
            agentArtifactSyncTask = nil
            authenticatedUserID = nil
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
        agentArtifactSyncTask?.cancel()
        deactivateAllConversations()
        stopVisualSessionMonitoring()
        globalUtilityCoordinator.stop()
        localConnectorService.terminatePluginApplicationsForHostExit()
        terminalWorkspace.closeAllTerminals()
        remoteConnectionWorkspaceStore.removeAllWorkspaces()
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
                let preferences = try await service.fetch()
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
