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
            localAgentHostError = localized("本地任务组件缺失，请重新安装应用。", english: "The local Task component is missing. Please reinstall the app.")
            localAgentRuntimeState.becomeDegraded(
                ownerUserID: ownerUserID,
                generation: generation,
                errorMessage: localAgentHostError ?? "Local Agent Host is unavailable."
            )
            return
        }
        let localAgentEventHub = localAgentEventHub
        let pendingShutdown = localAgentHostShutdownTask
        let apiClient = apiClient
        let localConnectorService = localConnectorService
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
        let serviceConfigurationUseCase = LocalAgentServiceConfigurationUseCase()
        let workspaceService = workspaceService
        let projectConversationService = projectConversationService
        let notepadService = notepadService
        let remoteConnectionMetadataService = remoteConnectionMetadataService
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
                self?.localAgentRuntimeState.beginHandshake(
                    ownerUserID: ownerUserID,
                    generation: generation
                )
                if let host = localAgentHost as? NativeLocalAgentHostLifecycle {
                    try await host.startForBootstrap(ownerUserID: ownerUserID)
                } else {
                    try await localAgentHost.start(ownerUserID: ownerUserID)
                }
                guard !Task.isCancelled,
                      self?.authenticatedUserID == ownerUserID,
                      self?.localAgentHostLifecycleGeneration == generation else {
                    if self?.localAgentHostLifecycleGeneration == generation {
                        await localAgentHost.stop()
                    }
                    return
                }
                self?.localAgentRuntimeState.beginSynchronization(
                    ownerUserID: ownerUserID,
                    generation: generation
                )
                if let host = localAgentHost as? NativeLocalAgentHostLifecycle {
                    do {
                        let memoryAccessToken = await apiClient.currentAccessToken()
                        if let restored = try await localConnectorService.restoreLocalAgentHost(
                            host,
                            ownerUserID: ownerUserID,
                            memoryAccessToken: memoryAccessToken
                        ) {
                            guard !Task.isCancelled,
                                  self?.authenticatedUserID == ownerUserID,
                                  self?.localAgentHostLifecycleGeneration == generation else {
                                if self?.localAgentHostLifecycleGeneration == generation {
                                    await host.stop()
                                }
                                return
                            }
                            try await serviceConfigurationUseCase.execute(
                                port: serviceConfigurationFactory.makePort(
                                    ownerUserID: ownerUserID,
                                    bootstrap: restored
                                ),
                                isCurrent: { [weak self] in
                                    !Task.isCancelled && self?.localAgentLifecycleAccepts(
                                        ownerUserID: ownerUserID,
                                        generation: generation
                                    ) == true
                                }
                            )
                            guard self?.applyConfiguredLocalAgentServices(
                                ownerUserID: ownerUserID,
                                lifecycleGeneration: generation
                            ) == true else { return }
                        }
                    } catch is CancellationError {
                        return
                    } catch {
                        // A missing or stale local snapshot must not prevent the authenticated
                        // gateway refresh below from repairing the control plane.
                    }
                }
                await workspaceService?.configure(ownerUserID: ownerUserID)
                guard !Task.isCancelled,
                      self?.authenticatedUserID == ownerUserID,
                      self?.localAgentHostLifecycleGeneration == generation else {
                    if self?.localAgentHostLifecycleGeneration == generation {
                        await localAgentHost.stop()
                    }
                    return
                }
                await projectConversationService?.configure(ownerUserID: ownerUserID)
                guard !Task.isCancelled,
                      self?.authenticatedUserID == ownerUserID,
                      self?.localAgentHostLifecycleGeneration == generation else {
                    if self?.localAgentHostLifecycleGeneration == generation {
                        await localAgentHost.stop()
                    }
                    return
                }
                self?.localAgentWorkspaceOwnerUserID = ownerUserID
                await notepadService.configure(ownerUserID: ownerUserID)
                await remoteConnectionMetadataService.configure(ownerUserID: ownerUserID)
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
        preparingProjectConversationIDs = []
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
                localAgentHostError = NSLocalizedString("本地任务服务多次启动失败，联系人和项目暂时无法加载。请重新打开应用；若仍失败，请查看诊断日志。", comment: "Local task service failed to recover")
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
                localAgentHostError = NSLocalizedString("本地任务服务多次启动失败，联系人和项目暂时无法加载。请重新打开应用；若仍失败，请查看诊断日志。", comment: "Local task service failed to recover")
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
        let apiClient = apiClient
        let localConnectorService = localConnectorService
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
        let serviceConfigurationUseCase = LocalAgentServiceConfigurationUseCase()
        localAgentBootstrapTask = Task { [weak self] in
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
                    guard !Task.isCancelled,
                          self?.localAgentControlPlaneBootstrapAccepts(token) == true else { return }
                    let memoryAccessToken = await apiClient.currentAccessToken()
                    guard !Task.isCancelled,
                          self?.localAgentControlPlaneBootstrapAccepts(token) == true else { return }
                    let bootstrap = try await localConnectorService.bootstrapLocalAgentHost(
                        host,
                        ownerUserID: ownerUserID,
                        memoryAccessToken: memoryAccessToken
                    )
                    guard !Task.isCancelled,
                          self?.localAgentControlPlaneBootstrapAccepts(token) == true else {
                        if self?.localAgentHostLifecycleGeneration == expectedLifecycleGeneration {
                            await host.stop()
                        }
                        return
                    }
                    try await serviceConfigurationUseCase.execute(
                        port: serviceConfigurationFactory.makePort(
                            ownerUserID: ownerUserID,
                            bootstrap: bootstrap
                        ),
                        isCurrent: { [weak self] in
                            !Task.isCancelled
                                && self?.localAgentControlPlaneBootstrapAccepts(token) == true
                        }
                    )
                    guard self?.completeLocalAgentControlPlaneBootstrap(token) == true else { return }
                    return
                } catch is CancellationError {
                    self?.finishCancelledLocalAgentControlPlaneBootstrap(token)
                    return
                } catch {
                    guard !Task.isCancelled,
                          self?.localAgentControlPlaneBootstrapAccepts(token) == true else { return }
                    if error is NativeLocalAgentCredentialRefreshDeferred {
                        self?.failLocalAgentControlPlaneBootstrap(token, error: error)
                        return
                    }
                    guard attempt == retryDelays.indices.last else { continue }
                    self?.failLocalAgentControlPlaneBootstrap(token, error: error)
                }
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
