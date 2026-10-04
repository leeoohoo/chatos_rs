import ChatOSAPI
import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import AppKit
import Combine
import Foundation
import SwiftUI

@MainActor
final class AppModel: ObservableObject, LocalConnectorCompanionRuntimeProviding {
    @Published var selection: SidebarSelection?
    @Published var projectTab: ProjectWorkspaceTab = .messages
    @Published var isNotepadPresented = false
    @Published var navigationSplitVisibility: NavigationSplitViewVisibility = .all
    @Published var interfaceLanguage = ChatOSLanguage(normalizing: UserDefaults.standard.string(
        forKey: "ChatOS.interfaceLanguage"
    )) {
        didSet { languagePreferenceDidChange() }
    }
    @Published var contextLanguage = ChatOSLanguage(normalizing: UserDefaults.standard.string(
        forKey: "ChatOS.internalContextLanguage"
    )) {
        didSet { languagePreferenceDidChange() }
    }
    @Published var isLanguagePreferencesLoading = false
    @Published var isLanguagePreferencesSaving = false
    @Published var languagePreferencesError: String?
    @Published var requestedConnectorSettingsTab: LocalConnectorControlTab?
    @Published var preventsIdleSystemSleep = UserDefaults.standard.bool(
        forKey: "ChatOS.preventsIdleSystemSleep"
    ) {
        didSet {
            guard preventsIdleSystemSleep != oldValue else { return }
            UserDefaults.standard.set(
                preventsIdleSystemSleep,
                forKey: "ChatOS.preventsIdleSystemSleep"
            )
            idleSleepController.setEnabled(preventsIdleSystemSleep)
        }
    }
    @Published var interfaceFontSize: Double = UserDefaults.standard.object(
        forKey: "ChatOS.interfaceFontSize"
    ) as? Double ?? 14 {
        didSet {
            let normalized = min(18, max(12, interfaceFontSize))
            if normalized != interfaceFontSize {
                interfaceFontSize = normalized
            } else {
                UserDefaults.standard.set(normalized, forKey: "ChatOS.interfaceFontSize")
            }
        }
    }
    @Published var projectConversation: ConversationSessionViewModel?
    @Published var contactConversation: ConversationSessionViewModel?
    @Published var contacts: [ResourceItem] = []
    @Published var projects: [ResourceItem] = []
    @Published var workspaceProjects: [WorkspaceProject] = []
    @Published var workspaceContacts: [WorkspaceContact] = []
    var workspaceConversations: [WorkspaceConversation] = []
    @Published var remoteConnections: [RemoteConnection] = []
    @Published var pluginApplications: [LocalConnectorPluginApplication] = []
    @Published var isPluginApplicationsLoading = false
    @Published var pluginApplicationsError: String?
    @Published var isRemoteConnectionsLoading = false
    @Published var remoteConnectionsError: String?
    @Published var isWorkspaceLoading = false
    @Published var workspaceError: String?
    @Published var preparingProjectConversationIDs: Set<String> = []
    @Published var projectConversationPreparationErrors: [String: String] = [:]
    @Published var localAgentHostError: String?

    let historyStore: ConversationHistoryStore
    let apiClient: ChatOSAPIClient
    let authentication: AuthenticationViewModel
    let localConnectorControl: LocalConnectorControlCenterViewModel
    let mediaStudio: MediaStudioViewModel
    let visualSessionStore = VisualSessionPresentationStore()
    let petPreferences = PetPreferencesStore()
    let petDefaultFileHandlerPrompt = PetDefaultFileHandlerPromptController()
    let petOverlayStore = PetOverlayStore()
    let globalUtilityPreferences = GlobalUtilityPreferencesStore()
    let terminalWorkspace = TerminalWorkspaceViewModel()
    private(set) lazy var globalUtilityCoordinator = GlobalUtilityCoordinator(
        model: self,
        preferences: globalUtilityPreferences
    )

    var terminals: [ResourceItem] {
        [
            ResourceItem(
                id: "terminal-local",
                title: localized("本机终端", english: "Local Terminal"),
                subtitle: localized("可用", english: "Available"),
                conversationID: nil,
                contactName: nil
            ),
        ]
    }

    let conversationService: NativeLocalAgentConversationService?
    let commandService: NativeLocalAgentConversationService?
    let petActivityService: NativeLocalAgentPetActivityService?
    let turnProcessService: NativeLocalAgentTurnProcessService?
    let messageTaskGraphService: NativeLocalAgentMessageTaskGraphService?
    let runtimeSettingsService: NativeLocalAgentConversationRuntimeSettingsService?
    let askUserPromptService: NativeLocalAgentAskUserPromptService?
    let requirementSurveyClient: NativeLocalAgentRequirementSurveyClient?
    let platformToolWorker: NativeLocalAgentPlatformToolWorker?
    let workspaceService: NativeLocalAgentWorkspaceService?
    let localConnectorService: NativeLocalConnectorService
    let localAgentHost: (any LocalAgentHostClientServicing)?
    let localAgentEventHub: NativeLocalAgentEventHub?
    let projectConversationService: NativeLocalAgentProjectConversationService?
    let localProjectsService: NativeLocalProjectsService
    let remoteConnectionMetadataService: NativeLocalAgentRemoteConnectionMetadataService
    let remoteConnectionService: NativeRemoteConnectionService
    let remoteFileService: NativeRemoteFileService
    let remoteConnectionWorkspaceStore: RemoteConnectionWorkspaceStore
    let projectFilesystemService: NativeProjectFilesystemService
    let projectCodeNavigationService: NativeProjectCodeNavigationService
    let projectGitService: NativeProjectGitService
    let projectRunService: NativeProjectRunService
    let agentGroupChatService: NativeAgentGroupChatService
    let agentServices: any AgentServiceProviding
    let agentSkillLibrary: LocalAgentSkillLibrary
    let agentGroupChatScheduler: LocalAgentGroupChatScheduler
    let agentGroupChatBuilderService: LocalAgentBuilderService
    let notepadService: NativeLocalAgentNotepadService
    let wechatCompanionService: ChatOSWeChatCompanionService
    let userLanguagePreferencesService: LocalUserLanguagePreferencesService
    var conversationCache: [String: ConversationSessionViewModel] = [:]
    var conversationCacheRecency = ConversationCacheRecency(capacity: 8)
    var projectConversationPreparationTasks: [String: Task<String, Error>] = [:]
    var projectConversationPreparationTaskIDs: [String: UUID] = [:]
    var projectConversationPreparationRequestIDs: [String: UUID] = [:]
    var workspaceLoadGeneration: Int64 = 0
    var workspaceLoadTask: Task<Void, Never>?
    var remoteConnectionsLoadGeneration: UInt64 = 0
    var remoteConnectionsLoadTask: Task<Void, Never>?
    var pluginApplicationsLoadGeneration: Int64 = 0
    var pluginApplicationsLoadTask: Task<Void, Never>?
    var visualSessionExpansion: [String: Bool] = [:]
    var visualSessionSelection: [String: String] = [:]
    var visualSessionMonitorTask: Task<Void, Never>?
    var visualSessionChangeTask: Task<Void, Never>?
    var visualSessionSleepTask: Task<Void, Never>?
    var visualSessionMonitorGeneration: UInt64 = 0
    var petOverlayCoordinator: PetOverlayCoordinator?
    let idleSleepController = AppIdleSleepController()
    var cancellables = Set<AnyCancellable>()
    var authenticatedUserID: String?
    var workspaceAccountGeneration: UInt64 = 0
    var isApplyingLanguagePreferences = false
    var languagePreferencesLoadTask: Task<Void, Never>?
    var languagePreferencesLoadGeneration: UInt64 = 0
    var languagePreferencesSaveTask: Task<Void, Never>?
    var languagePreferencesSaveGeneration: UInt64 = 0
    var agentHeartbeatTask: Task<Void, Never>?
    var agentCommunicationTask: Task<Void, Never>?
    var agentExecutorRecoveryTask: Task<Void, Never>?
    var agentRuntimeCoordinatorGeneration: UInt64 = 0
    var agentArtifactStorageTask: Task<Void, Never>?
    var agentArtifactStorageOwnerUserID: String?
    var agentArtifactStorageGeneration: UInt64 = 0
    var localConnectorRecoveryTask: Task<Void, Never>?
    var localConnectorSleepPreparationTask: Task<Void, Never>?
    var localConnectorSleepPreparationGeneration: UInt64 = 0
    var localAgentHostLifecycleTask: Task<Void, Never>?
    var localAgentHostLifecycleGeneration: UInt64 = 0
    var localAgentHostHealthCheckTask: Task<Void, Never>?
    var localAgentHostHealthCheckGeneration: UInt64 = 0
    var localAgentHostShutdownTask: Task<Void, Never>?
    var localAgentHostShutdownGeneration: UInt64 = 0
    var localAgentCrashRecoveryTask: Task<Void, Never>?
    var localAgentCrashRecoveryAttempts = 0
    var localAgentBootstrapTask: Task<Void, Never>?
    var localAgentControlPlaneOwnerUserID: String?
    var localAgentControlPlaneBootstrapOwnerUserID: String?
    var localConnectorRecoveryGeneration: UInt64 = 0
    var lastLocalConnectorRecoveryDate: Date?
    var mainWindowPresentationHandler: (() -> Void)?
    var settingsWindowPresentationHandler: (() -> Void)?

    init() {
        let credentialStore = KeychainCredentialStore()
        let apiClient = ChatOSAPIClient(
            configuration: .init(baseURL: RuntimeConfiguration.apiBaseURL),
            credentialStore: credentialStore
        )
        let localAgentHost = RuntimeConfiguration.localAgentHostConfiguration.map {
            NativeLocalAgentHostLifecycle(configuration: $0)
        }
        let authenticationService = ChatOSAuthenticationService(
            client: apiClient,
            credentialStore: credentialStore
        )
        let historyStore = ConversationHistoryStore()
        let connectorTicketProvider = ChatOSLocalConnectorPairingTicketProvider(client: apiClient)
        let remoteConnectionMetadataService = NativeLocalAgentRemoteConnectionMetadataService(
            host: localAgentHost
        )
        let remoteConnectionService = NativeRemoteConnectionService(
            upstream: remoteConnectionMetadataService,
            connectorStateURL: RuntimeConfiguration.nativeConnectorStateURL
        )
        let localConnectorService = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: RuntimeConfiguration.localConnectorCloudBaseURL,
                stateURL: RuntimeConfiguration.nativeConnectorStateURL,
                deploymentIdentifier: RuntimeConfiguration.deployment.identifier
            ),
            ticketProvider: connectorTicketProvider,
            approvalMemoryProviderFactory: { tenantID, workspaceID, runID, runtimeScope in
                let scope = try AgentMemoryScope(
                    tenantID: tenantID, profile: "approval", projectID: workspaceID,
                    runID: runID, runtimeScope: runtimeScope
                )
                let memory = try await ChatOSMemoryEngineService(client: apiClient, scope: scope)
                return AgentMemoryContextProvider(scope: scope, service: memory)
            }
        )
        let localProjectsService = NativeLocalProjectsService(
            connector: localConnectorService,
            databaseURL: RuntimeConfiguration.nativeConnectorStateURL.deletingLastPathComponent()
                .appendingPathComponent("Projects.sqlite3")
        )
        let agentGroupChatService = NativeAgentGroupChatService(
            databaseURL: RuntimeConfiguration.nativeConnectorStateURL.deletingLastPathComponent()
                .appendingPathComponent("AgentGroupChat.sqlite3"),
            agentArtifactStore: localAgentHost.map(NativeLocalAgentArtifactClient.init(host:))
        )

        self.historyStore = historyStore
        self.apiClient = apiClient
        self.authentication = AuthenticationViewModel(service: authenticationService)
        self.localConnectorControl = LocalConnectorControlCenterViewModel(
            service: localConnectorService
        )
        let remoteAgentServices = ChatOSStoryPlanningService(client: apiClient)
        let agentServices: any AgentServiceProviding
        do {
            agentServices = try OfflineCapableAgentServiceProvider(
                upstream: remoteAgentServices,
                databaseURL: RuntimeConfiguration.nativeConnectorStateURL.deletingLastPathComponent()
                    .appendingPathComponent("AgentMemoryCache.sqlite3")
            )
        } catch {
            // Storage initialization is validated again by the scheduler. Keep
            // app startup recoverable if the local cache file needs repair.
            agentServices = remoteAgentServices
        }
        self.agentServices = agentServices
        self.mediaStudio = MediaStudioViewModel(
            service: ChatOSMediaGenerationService(client: apiClient),
            storyPlanner: remoteAgentServices
        )
        self.localConnectorService = localConnectorService
        let localAgentRuntimeSettingsService: NativeLocalAgentConversationRuntimeSettingsService?
        let localAgentConversationService: NativeLocalAgentConversationService?
        let localAgentPlatformToolWorker: NativeLocalAgentPlatformToolWorker?
        let localAgentEventHub: NativeLocalAgentEventHub?
        if let localAgentHost {
            let attachmentRootURL = RuntimeConfiguration.nativeConnectorStateURL
                .deletingLastPathComponent()
                .appendingPathComponent("LocalAgent/Attachments", isDirectory: true)
            let settings = NativeLocalAgentConversationRuntimeSettingsService(host: localAgentHost)
            let eventHub = NativeLocalAgentEventHub(host: localAgentHost)
            let worker = NativeLocalAgentPlatformToolWorker(
                host: localAgentHost,
                attachmentRootURL: attachmentRootURL,
                projects: localProjectsService,
                connector: localConnectorService,
                eventHub: eventHub
            )
            localAgentRuntimeSettingsService = settings
            localAgentEventHub = eventHub
            localAgentPlatformToolWorker = worker
            localAgentConversationService = NativeLocalAgentConversationService(
                host: localAgentHost,
                attachmentRootURL: attachmentRootURL,
                runtimeSettings: settings,
                platformToolWorker: worker,
                eventHub: eventHub
            )
        } else {
            localAgentRuntimeSettingsService = nil
            localAgentEventHub = nil
            localAgentConversationService = nil
            localAgentPlatformToolWorker = nil
        }
        self.localAgentHost = localAgentHost
        self.localAgentEventHub = localAgentEventHub
        self.conversationService = localAgentConversationService
        self.petActivityService = localAgentHost.flatMap { host in
            localAgentEventHub.map {
                NativeLocalAgentPetActivityService(host: host, eventHub: $0)
            }
        }
        let localMessageTaskGraphService = localAgentHost.map {
            NativeLocalAgentMessageTaskGraphService(host: $0)
        }
        self.messageTaskGraphService = localMessageTaskGraphService
        self.turnProcessService = localAgentHost.map {
            NativeLocalAgentTurnProcessService(host: $0)
        }
        let localAskUserPromptService = localAgentHost.map {
            NativeLocalAgentAskUserPromptService(host: $0)
        }
        self.askUserPromptService = localAskUserPromptService
        self.requirementSurveyClient = localAgentHost.map {
            NativeLocalAgentRequirementSurveyClient(host: $0)
        }
        self.platformToolWorker = localAgentPlatformToolWorker
        let workspaceService = localAgentHost.map {
            NativeLocalAgentWorkspaceService(host: $0)
        }
        self.workspaceService = workspaceService
        self.projectConversationService = localAgentHost.flatMap { host in
            workspaceService.map {
                NativeLocalAgentProjectConversationService(host: host, workspace: $0)
            }
        }
        self.localProjectsService = localProjectsService
        let agentSkillLibrary = LocalAgentSkillLibrary(
            fileURL: RuntimeConfiguration.nativeConnectorStateURL.deletingLastPathComponent()
                .appendingPathComponent("AgentSkillOverrides.json")
        )
        self.agentGroupChatService = agentGroupChatService
        self.agentSkillLibrary = agentSkillLibrary
        let agentGroupChatScheduler = LocalAgentGroupChatScheduler(
            service: agentGroupChatService,
            services: agentServices,
            projectTypeKeyProvider: { ownerUserID, projectID in
                try await localProjectsService.registry().get(
                    ownerUserID: ownerUserID,
                    id: projectID
                )?.draft.projectTypeKey
            },
            professionProvider: { ownerUserID, key in
                agentSkillLibrary.profession(ownerUserID: ownerUserID, key: key)
            },
            projectTypeProvider: { ownerUserID, key in
                agentSkillLibrary.projectType(ownerUserID: ownerUserID, key: key)
            },
            professionCatalogProvider: { ownerUserID in
                agentSkillLibrary.professions(ownerUserID: ownerUserID)
            },
            todoPluginCatalogProvider: { ownerUserID in
                try await localConnectorService.installedAgentPlugins(
                    ownerUserID: ownerUserID
                ).map {
                    LocalAgentTodoPluginOption(
                        pluginID: $0.id,
                        displayName: $0.displayName,
                        description: $0.description
                    )
                }
            },
            contextLanguageProvider: { _ in
                ChatOSLanguage(normalizing: UserDefaults.standard.string(
                    forKey: "ChatOS.internalContextLanguage"
                ))
            },
            additionalToolProviders: { profile, member, runContext, productSkillSession in
                var providers: [any AgentToolProvider] = []
                if LocalAgentPermission.canAccessLocalProjects(profile.draft.defaultSkillIDs) {
                    let store = try await agentGroupChatService.store()
                    let registry = try await localProjectsService.registry()
                    let projects = try await registry.list(
                        ownerUserID: runContext.ownerUserID,
                        includeInactive: false
                    )
                    providers.append(LocalAgentProjectToolProvider(
                        store: store,
                        projects: projects,
                        projectTypes: agentSkillLibrary.projectTypes(
                            ownerUserID: runContext.ownerUserID
                        ),
                        projectsService: localProjectsService,
                        context: runContext,
                        productSkillSession: productSkillSession
                    ))
                }
                if runContext.lane == .executor {
                    let store = try await agentGroupChatService.store()
                    guard let todo = try await store.todoForDelivery(
                        ownerUserID: runContext.ownerUserID,
                        deliveryID: runContext.deliveryID
                    ), todo.agentID == profile.id,
                    todo.teamRoomID == member.roomID,
                    !runContext.projectID.hasPrefix("direct:") else {
                        throw AgentGroupChatError.conflict
                    }
                    let projectContext = try await localProjectsService.pluginContext(
                        ownerUserID: runContext.ownerUserID,
                        projectID: runContext.projectID
                    )
                    providers.append(try await localConnectorService.makeAgentCapabilityToolProvider(
                        ownerUserID: runContext.ownerUserID,
                        runContext: runContext,
                        projectContext: projectContext,
                        executionPlan: todo.executionPlan
                    ))
                }
                return providers
            }
        )
        self.agentGroupChatScheduler = agentGroupChatScheduler
        self.agentGroupChatBuilderService = LocalAgentBuilderService(
            groupChatService: agentGroupChatService,
            projectsService: localProjectsService,
            connectorService: localConnectorService,
            agentServices: agentServices,
            skillLibrary: agentSkillLibrary
        )
        let remoteFileService = NativeRemoteFileService(runtime: remoteConnectionService)
        self.remoteConnectionMetadataService = remoteConnectionMetadataService
        self.remoteConnectionService = remoteConnectionService
        self.remoteFileService = remoteFileService
        self.remoteConnectionWorkspaceStore = RemoteConnectionWorkspaceStore(
            terminalService: remoteConnectionService,
            fileService: remoteFileService
        )
        self.projectFilesystemService = NativeProjectFilesystemService(connector: localConnectorService)
        self.projectCodeNavigationService = NativeProjectCodeNavigationService(connector: localConnectorService)
        self.projectGitService = NativeProjectGitService(connector: localConnectorService)
        self.notepadService = NativeLocalAgentNotepadService(host: localAgentHost)
        self.wechatCompanionService = ChatOSWeChatCompanionService(client: apiClient)
        self.userLanguagePreferencesService = LocalUserLanguagePreferencesService()
        self.projectRunService = NativeProjectRunService(
            connector: localConnectorService,
            preferencesURL: RuntimeConfiguration.nativeConnectorStateURL
                .deletingLastPathComponent()
                .appendingPathComponent("ProjectRunSettings.json")
        )
        self.commandService = localAgentConversationService
        self.runtimeSettingsService = localAgentRuntimeSettingsService
        idleSleepController.setEnabled(preventsIdleSystemSleep)
        authentication.$phase
            .removeDuplicates()
            .sink { [weak self] phase in
                self?.applyAuthenticationPhase(phase)
            }
            .store(in: &cancellables)
        NotificationCenter.default.publisher(for: .chatOSAuthenticationDidExpire)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.authentication.expireSession()
            }
            .store(in: &cancellables)
        NSWorkspace.shared.notificationCenter.publisher(for: NSWorkspace.willSleepNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.prepareLocalConnectorForSystemSleep()
            }
            .store(in: &cancellables)
        NSWorkspace.shared.notificationCenter.publisher(for: NSWorkspace.didWakeNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.recoverLocalConnector(forceReconnect: true)
                self?.recoverLocalAgentHostAfterSystemWake()
                self?.restartAgentHeartbeatCoordinator()
                self?.restartAgentArtifactStorageCoordinator()
            }
            .store(in: &cancellables)
        NotificationCenter.default.publisher(for: .nativeLocalAgentHostDidExit)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.recoverLocalAgentHostAfterUnexpectedExit()
            }
            .store(in: &cancellables)
        NotificationCenter.default.publisher(for: .agentHeartbeatConfigurationDidChange)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.restartAgentHeartbeatCoordinator()
            }
            .store(in: &cancellables)
        NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.authentication.retrySessionRestoreIfNeeded()
                self?.recoverLocalConnector(forceReconnect: false)
                self?.recoverLocalAgentHostIfNeeded()
                self?.ensureAgentRuntimeCoordinators()
                self?.ensureAgentArtifactStorageCoordinator()
                self?.startVisualSessionMonitoring()
            }
            .store(in: &cancellables)
        NotificationCenter.default.publisher(for: NSApplication.didResignActiveNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.stopVisualSessionMonitoring() }
            .store(in: &cancellables)
        NSWorkspace.shared.notificationCenter.publisher(for: NSWorkspace.willSleepNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.stopVisualSessionMonitoring() }
            .store(in: &cancellables)
        NSWorkspace.shared.notificationCenter.publisher(for: NSWorkspace.didWakeNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.startVisualSessionMonitoring() }
            .store(in: &cancellables)
        localConnectorControl.$status
            .map { $0?.connectorRunning == true }
            .removeDuplicates()
            .filter { $0 }
            .sink { [weak self] _ in
                self?.refreshWorkspace()
            }
            .store(in: &cancellables)
        localConnectorControl.$status
            .map { status -> String? in
                guard status?.configured == true else { return nil }
                return status?.user?.id
            }
            .compactMap { $0 }
            .sink { [weak self] ownerUserID in
                guard self?.authenticatedUserID == ownerUserID else { return }
                self?.refreshLocalAgentControlPlane(ownerUserID: ownerUserID)
            }
            .store(in: &cancellables)
        localConnectorControl.$plugins
            .map { plugins in
                plugins.map { "\($0.pluginID):\($0.installed):\($0.enabled):\($0.latestVersion)" }
                    .sorted()
            }
            .removeDuplicates()
            .sink { [weak self] _ in
                self?.refreshPluginApplications()
            }
            .store(in: &cancellables)
        $selection
            .removeDuplicates()
            .sink { [weak self] selection in
                self?.activateConversation(for: selection)
            }
            .store(in: &cancellables)
        startVisualSessionMonitoring()
        let connectorServicePreparationTask = Task { [weak self, localConnectorService] in
            await localConnectorService.setLocalAgentCompanionServices(
                host: localAgentHost,
                conversation: localAgentConversationService,
                messageTasks: localMessageTaskGraphService,
                askUser: localAskUserPromptService
            )
            await localConnectorService.setAgentGroupChatService(agentGroupChatService)
            await localConnectorService.setAgentGroupChatScheduler(agentGroupChatScheduler)
            guard let self else { return }
            await localConnectorService.setCompanionRuntime(self)
        }
        localConnectorControl.setServicePreparationTask(connectorServicePreparationTask)
        authentication.start()
    }

}
