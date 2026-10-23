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
            mediaStudio.bindAccount(userID: session.user.id)
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
            contacts = []
            projects = []
            workspaceProjects = []
            workspaceContacts = []
            workspaceConversations = []
            remoteConnectionsLoadState.reset()
            remoteConnections = []
            isRemoteConnectionsLoading = false
            remoteConnectionsError = nil
            terminalWorkspace.closeAllTerminals()
            remoteConnectionWorkspaceStore.removeAllWorkspaces()
            pluginApplicationsLoadState.reset()
            pluginApplications = []
            isPluginApplicationsLoading = false
            pluginApplicationsError = nil
            deactivateAllConversations()
            projectConversation = nil
            contactConversation = nil
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
        localAgentHostRecoveryState.reset()
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
        // Authentication restoration, wake recovery and request-driven recovery can all arrive
        // while the same account is still launching. Cancelling that task used to abandon the
        // caller while the detached Host was copying a multi-gigabyte migration backup, then
        // immediately launch a competing Host. Let the existing same-owner launch finish.
        if localAgentHostLifecycleTask != nil,
           localAgentRuntimeState.ownerUserID == ownerUserID {
            return
        }
        if localAgentHostRecoveryState.activeRestart != nil {
            localAgentCrashRecoveryTask?.cancel()
            localAgentCrashRecoveryTask = nil
            localAgentHostRecoveryState.cancelRestart()
        }
        localAgentHostLifecycleTask?.cancel()
        localAgentHostLifecycleGeneration &+= 1
        let generation = localAgentHostLifecycleGeneration
        localAgentRuntimeState.beginLaunch(
            ownerUserID: ownerUserID,
            generation: generation
        )
        localAgentWorkspaceOwnerUserID = nil
        localAgentServicesOwnerUserID = nil
        localAgentControlPlaneOwnerUserID = nil
        localAgentControlPlaneBootstrapState.reset()
        localAgentBootstrapTask?.cancel()
        localAgentBootstrapTask = nil
        localAgentHostError = nil
        workspaceError = nil
        conversationCache.values.forEach {
            $0.localAgentRuntimeDidBecomeUnavailable()
        }
        guard let localAgentHost else {
            localAgentHostError = localized("任务组件缺失，请重新安装应用。", english: "The Task component is missing. Please reinstall the app.")
            localAgentRuntimeState.becomeDegraded(
                ownerUserID: ownerUserID,
                generation: generation,
                errorMessage: localAgentHostError ?? "Local Agent Host is unavailable."
            )
            return
        }
        let pendingShutdown = localAgentHostShutdownTask
        let serviceConfigurationFactory = NativeLocalAgentServiceConfigurationFactory(
            localConnectorService: localConnectorService,
            runtimeSettingsService: runtimeSettingsService,
            petActivityService: petActivityService,
            commandService: commandService,
            messageTaskGraphService: messageTaskGraphService,
            askUserPromptService: askUserPromptService,
            turnProcessService: turnProcessService,
            platformToolWorker: platformToolWorker
        )
        let launchPort = AppLocalAgentHostLaunchAdapter(
            host: localAgentHost,
            eventHub: localAgentEventHub,
            apiClient: apiClient,
            localConnectorService: localConnectorService,
            serviceConfigurationFactory: serviceConfigurationFactory,
            workspaceService: workspaceService,
            projectConversationService: projectConversationService,
            notepadService: notepadService,
            remoteConnectionMetadataService: remoteConnectionMetadataService,
            isCurrent: { [weak self] in
                !Task.isCancelled && self?.localAgentLifecycleAccepts(
                    ownerUserID: ownerUserID,
                    generation: generation
                ) == true
            }
        )
        let launchUseCase = LocalAgentHostLaunchUseCase()
        localAgentHostLifecycleTask = Task { [weak self] in
            defer {
                if self?.localAgentHostLifecycleGeneration == generation {
                    self?.localAgentHostLifecycleTask = nil
                }
            }
            do {
                await pendingShutdown?.value
                try await launchUseCase.execute(
                    ownerUserID: ownerUserID,
                    port: launchPort,
                    isCurrent: { [weak self] in
                        !Task.isCancelled && self?.localAgentLifecycleAccepts(
                            ownerUserID: ownerUserID,
                            generation: generation
                        ) == true
                    },
                    onTransition: { [weak self] transition in
                        guard let self,
                              localAgentLifecycleAccepts(
                                  ownerUserID: ownerUserID,
                                  generation: generation
                              ) else { throw CancellationError() }
                        switch transition {
                        case .handshaking:
                            localAgentRuntimeState.beginHandshake(
                                ownerUserID: ownerUserID,
                                generation: generation
                            )
                        case .synchronizing:
                            localAgentRuntimeState.beginSynchronization(
                                ownerUserID: ownerUserID,
                                generation: generation
                            )
                        case .restoredServicesReady:
                            guard applyConfiguredLocalAgentServices(
                                ownerUserID: ownerUserID,
                                lifecycleGeneration: generation
                            ) else { throw CancellationError() }
                        case .workspaceReady:
                            localAgentWorkspaceOwnerUserID = ownerUserID
                        }
                    }
                )
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
                self?.localAgentRuntimeState.becomeDegraded(
                    ownerUserID: ownerUserID,
                    generation: generation,
                    errorMessage: error.localizedDescription
                )
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
        localAgentHostRecoveryState.resetRestarts()
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
        let token = localAgentHostRecoveryState.beginHealthCheck(
            ownerUserID: ownerUserID,
            accountGeneration: workspaceAccountGeneration,
            lifecycleGeneration: localAgentHostLifecycleGeneration
        )
        localAgentHostHealthCheckTask = Task { [weak self] in
            defer {
                if self?.localAgentHostRecoveryState.finishHealthCheck(token) == true {
                    self?.localAgentHostHealthCheckTask = nil
                }
            }
            let isRunning = await host.isRunning
            guard !Task.isCancelled,
                  let self,
                  localAgentHostRecoveryState.acceptsHealthCheck(
                      token,
                      ownerUserID: authenticatedUserID,
                      accountGeneration: workspaceAccountGeneration,
                      lifecycleGeneration: localAgentHostLifecycleGeneration
                  ),
                  localAgentHostLifecycleTask == nil else { return }
            if !isRunning {
                localAgentCrashRecoveryTask?.cancel()
                localAgentCrashRecoveryTask = nil
                localAgentHostRecoveryState.resetRestarts()
                startLocalAgentHost(
                    ownerUserID: ownerUserID,
                    refreshControlPlaneAfterStart: true
                )
            }
        }
    }

    private func cancelAccountScopedBackgroundTasks() {
        workspaceLoadState.reset()
        workspaceLoadTask?.cancel()
        workspaceLoadTask = nil
        isWorkspaceLoading = false
        remoteConnectionsLoadState.reset()
        remoteConnectionsLoadTask?.cancel()
        remoteConnectionsLoadTask = nil
        isRemoteConnectionsLoading = false
        pluginApplicationsLoadState.reset()
        pluginApplicationsLoadTask?.cancel()
        pluginApplicationsLoadTask = nil
        isPluginApplicationsLoading = false
        localAgentControlPlaneBootstrapState.reset()
        localAgentBootstrapTask?.cancel()
        localAgentBootstrapTask = nil
        localConnectorSleepPreparationGeneration &+= 1
        localConnectorSleepPreparationTask?.cancel()
        localConnectorSleepPreparationTask = nil
        prepareProjectConversationUseCase?.cancelAll()
        projectConversationPreparationState.reset()
        projectConversationPreparationTasks.values.forEach { $0.cancel() }
        projectConversationPreparationTasks.removeAll()
        localAgentHostRecoveryState.cancelHealthCheck()
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
              let ownerUserID = authenticatedUserID else {
            if localAgentHostRecoveryState.hasExhaustedRestarts {
                localAgentHostError = NSLocalizedString("任务服务多次启动失败，联系人和项目暂时无法加载。请重新打开应用；若仍失败，请查看诊断日志。", comment: "Local task service failed to recover")
                localAgentRuntimeState.becomeDegraded(
                    ownerUserID: authenticatedUserID,
                    generation: localAgentHostLifecycleGeneration,
                    errorMessage: localAgentHostError ?? "Local Agent Host recovery failed."
                )
            }
            return
        }
        guard let plan = localAgentHostRecoveryState.scheduleRestart(
            ownerUserID: ownerUserID,
            accountGeneration: workspaceAccountGeneration,
            lifecycleGeneration: localAgentHostLifecycleGeneration
        ) else {
            if localAgentHostRecoveryState.hasExhaustedRestarts {
                localAgentHostError = NSLocalizedString("任务服务多次启动失败，联系人和项目暂时无法加载。请重新打开应用；若仍失败，请查看诊断日志。", comment: "Local task service failed to recover")
                localAgentRuntimeState.becomeDegraded(
                    ownerUserID: authenticatedUserID,
                    generation: localAgentHostLifecycleGeneration,
                    errorMessage: localAgentHostError ?? "Local Agent Host recovery failed."
                )
            }
            return
        }
        localAgentRuntimeState.scheduleRestart(
            ownerUserID: ownerUserID,
            generation: localAgentHostLifecycleGeneration,
            attempt: plan.token.attempt,
            errorMessage: localAgentHostError
        )
        localAgentCrashRecoveryTask = Task { [weak self] in
            do {
                try await Task.sleep(for: plan.delay)
            } catch {
                return
            }
            guard let self,
                  localAgentHostRecoveryState.acceptsRestart(
                      plan.token,
                      ownerUserID: authenticatedUserID,
                      accountGeneration: workspaceAccountGeneration,
                      lifecycleGeneration: localAgentHostLifecycleGeneration
                  ),
                  localAgentHostRecoveryState.finishRestart(plan.token) else { return }
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
        localAgentHostRecoveryState.reset()
        localAgentHostLifecycleGeneration &+= 1
        localAgentRuntimeState.stop(generation: localAgentHostLifecycleGeneration)
        localAgentHostLifecycleTask?.cancel()
        localAgentHostLifecycleTask = nil
        localAgentHostError = nil
        localAgentWorkspaceOwnerUserID = nil
        localAgentServicesOwnerUserID = nil
        localAgentControlPlaneOwnerUserID = nil
        localAgentControlPlaneBootstrapState.reset()
        localAgentBootstrapTask?.cancel()
        localAgentBootstrapTask = nil
        workspaceError = nil
        conversationCache.values.forEach {
            $0.localAgentRuntimeDidBecomeUnavailable()
        }
        guard let localAgentHost else { return }
        let shutdownPort = NativeLocalAgentServiceShutdownAdapter(
            commandService: commandService,
            petActivityService: petActivityService,
            messageTaskGraphService: messageTaskGraphService,
            askUserPromptService: askUserPromptService,
            turnProcessService: turnProcessService,
            runtimeSettingsService: runtimeSettingsService,
            platformToolWorker: platformToolWorker,
            workspaceService: workspaceService,
            projectConversationService: projectConversationService,
            notepadService: notepadService,
            remoteConnectionMetadataService: remoteConnectionMetadataService,
            host: localAgentHost
        )
        let shutdownUseCase = LocalAgentServiceShutdownUseCase()
        let previousShutdown = localAgentHostShutdownTask
        localAgentHostShutdownGeneration &+= 1
        let shutdownGeneration = localAgentHostShutdownGeneration
        localAgentHostShutdownTask = Task { [weak self] in
            await previousShutdown?.value
            await shutdownUseCase.execute(port: shutdownPort)
            guard let self,
                  localAgentHostShutdownGeneration == shutdownGeneration else { return }
            localAgentHostShutdownTask = nil
        }
    }

    func refreshLocalAgentControlPlane(ownerUserID: String, force: Bool = false) {
        if force {
            localAgentControlPlaneOwnerUserID = nil
            localAgentControlPlaneBootstrapState.reset()
            localAgentBootstrapTask?.cancel()
            localAgentBootstrapTask = nil
        }
        guard localAgentControlPlaneOwnerUserID != ownerUserID,
              localAgentControlPlaneBootstrapState.activeToken?.ownerUserID != ownerUserID else { return }
        localAgentBootstrapTask?.cancel()
        localAgentControlPlaneBootstrapState.reset()
        guard let host = localAgentHost as? NativeLocalAgentHostLifecycle else { return }
        let expectedLifecycleGeneration = localAgentHostLifecycleGeneration
        let token = localAgentControlPlaneBootstrapState.begin(
            ownerUserID: ownerUserID,
            accountGeneration: workspaceAccountGeneration,
            lifecycleGeneration: expectedLifecycleGeneration
        )
        let serviceConfigurationFactory = NativeLocalAgentServiceConfigurationFactory(
            localConnectorService: localConnectorService,
            runtimeSettingsService: runtimeSettingsService,
            petActivityService: petActivityService,
            commandService: commandService,
            messageTaskGraphService: messageTaskGraphService,
            askUserPromptService: askUserPromptService,
            turnProcessService: turnProcessService,
            platformToolWorker: platformToolWorker
        )
        let bootstrapPort = AppLocalAgentControlPlaneBootstrapAdapter(
            host: host,
            apiClient: apiClient,
            localConnectorService: localConnectorService,
            serviceConfigurationFactory: serviceConfigurationFactory,
            isCurrent: { [weak self] in
                !Task.isCancelled
                    && self?.localAgentControlPlaneBootstrapAccepts(token) == true
            },
            shouldStopInvalidatedHost: { [weak self] in
                self?.localAgentHostLifecycleGeneration == expectedLifecycleGeneration
            }
        )
        let bootstrapUseCase = LocalAgentControlPlaneBootstrapUseCase()
        localAgentBootstrapTask = Task { [weak self] in
            do {
                try await bootstrapUseCase.execute(
                    ownerUserID: ownerUserID,
                    port: bootstrapPort,
                    isCurrent: { [weak self] in
                        !Task.isCancelled
                            && self?.localAgentControlPlaneBootstrapAccepts(token) == true
                    }
                )
                guard self?.completeLocalAgentControlPlaneBootstrap(token) == true else { return }
            } catch is CancellationError {
                self?.finishCancelledLocalAgentControlPlaneBootstrap(token)
            } catch {
                guard !Task.isCancelled,
                      self?.localAgentControlPlaneBootstrapAccepts(token) == true else { return }
                self?.failLocalAgentControlPlaneBootstrap(token, error: error)
            }
        }
    }

    private func localAgentLifecycleAccepts(
        ownerUserID: String,
        generation: UInt64
    ) -> Bool {
        authenticatedUserID == ownerUserID
            && localAgentHostLifecycleGeneration == generation
    }

    private func localAgentControlPlaneBootstrapAccepts(
        _ token: LocalAgentControlPlaneBootstrapToken
    ) -> Bool {
        localAgentControlPlaneBootstrapState.accepts(
            token,
            ownerUserID: authenticatedUserID,
            accountGeneration: workspaceAccountGeneration,
            lifecycleGeneration: localAgentHostLifecycleGeneration
        )
    }

    @discardableResult
    private func applyConfiguredLocalAgentServices(
        ownerUserID: String,
        lifecycleGeneration: UInt64
    ) -> Bool {
        guard !Task.isCancelled,
              localAgentLifecycleAccepts(
                  ownerUserID: ownerUserID,
                  generation: lifecycleGeneration
              ) else { return false }
        localAgentServicesOwnerUserID = ownerUserID
        localAgentRuntimeState.becomeReady(
            ownerUserID: ownerUserID,
            generation: lifecycleGeneration
        )
        conversationCache.values.forEach {
            $0.localAgentRuntimeDidBecomeReady()
        }
        return true
    }

    private func completeLocalAgentControlPlaneBootstrap(
        _ token: LocalAgentControlPlaneBootstrapToken
    ) -> Bool {
        guard localAgentControlPlaneBootstrapAccepts(token),
              applyConfiguredLocalAgentServices(
                  ownerUserID: token.ownerUserID,
                  lifecycleGeneration: token.lifecycleGeneration
              ),
              localAgentControlPlaneBootstrapState.finish(token) else { return false }
        localAgentControlPlaneOwnerUserID = token.ownerUserID
        localAgentCrashRecoveryTask?.cancel()
        localAgentCrashRecoveryTask = nil
        localAgentHostRecoveryState.resetRestarts()
        localAgentHostError = nil
        localAgentBootstrapTask = nil
        return true
    }

    private func finishCancelledLocalAgentControlPlaneBootstrap(
        _ token: LocalAgentControlPlaneBootstrapToken
    ) {
        guard localAgentControlPlaneBootstrapAccepts(token),
              localAgentControlPlaneBootstrapState.finish(token) else { return }
        localAgentBootstrapTask = nil
    }

    private func failLocalAgentControlPlaneBootstrap(
        _ token: LocalAgentControlPlaneBootstrapToken,
        error: Error
    ) {
        guard localAgentControlPlaneBootstrapAccepts(token),
              localAgentControlPlaneBootstrapState.finish(token) else { return }
        localAgentBootstrapTask = nil
        localAgentHostError = error.localizedDescription
        if !localAgentRuntimeState.isReady(for: token.ownerUserID) {
            localAgentRuntimeState.becomeDegraded(
                ownerUserID: token.ownerUserID,
                generation: token.lifecycleGeneration,
                errorMessage: error.localizedDescription
            )
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
