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

    let historyStore: ConversationHistoryStore
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

    let conversationService: ChatOSConversationService
    let realtimeService: ChatOSRealtimeClient
    let commandService: ChatOSConversationCommandService
    let turnProcessService: ChatOSTurnProcessService
    let messageTaskGraphService: ChatOSMessageTaskGraphService
    let runtimeSettingsService: ChatOSConversationRuntimeSettingsService
    let askUserPromptService: ChatOSAskUserPromptService
    let petActivityInboxService: ChatOSPetActivityInboxService
    let workspaceService: ChatOSWorkspaceService
    let localConnectorService: NativeLocalConnectorService
    let projectConversationService: ChatOSProjectConversationService
    let localProjectsService: NativeLocalProjectsService
    let remoteConnectionService: NativeRemoteConnectionService
    let remoteFileService: NativeRemoteFileService
    let remoteConnectionWorkspaceStore: RemoteConnectionWorkspaceStore
    let projectFilesystemService: NativeProjectFilesystemService
    let projectCodeNavigationService: NativeProjectCodeNavigationService
    let projectGitService: NativeProjectGitService
    let projectRunService: NativeProjectRunService
    let agentGroupChatService: NativeAgentGroupChatService
    let agentSkillLibrary: LocalAgentSkillLibrary
    let agentGroupChatScheduler: LocalAgentGroupChatScheduler
    let agentGroupChatBuilderService: LocalAgentBuilderService
    let notepadService: ChatOSNotepadService
    let wechatCompanionService: ChatOSWeChatCompanionService
    let userLanguagePreferencesService: ChatOSUserLanguagePreferencesService
    var conversationCache: [String: ConversationSessionViewModel] = [:]
    var conversationCacheRecency = ConversationCacheRecency(capacity: 8)
    var projectConversationPreparationTasks: [String: Task<String, Error>] = [:]
    var workspaceLoadGeneration: Int64 = 0
    var pluginApplicationsLoadGeneration: Int64 = 0
    var visualSessionExpansion: [String: Bool] = [:]
    var visualSessionSelection: [String: String] = [:]
    var visualSessionMonitorTask: Task<Void, Never>?
    var petOverlayCoordinator: PetOverlayCoordinator?
    let idleSleepController = AppIdleSleepController()
    var cancellables = Set<AnyCancellable>()
    var authenticatedUserID: String?
    var workspaceAccountGeneration: UInt64 = 0
    var isApplyingLanguagePreferences = false
    var languagePreferencesSaveTask: Task<Void, Never>?
    var agentHeartbeatTask: Task<Void, Never>?
    var agentCommunicationTask: Task<Void, Never>?
    var agentArtifactSyncTask: Task<Void, Never>?
    var mainWindowPresentationHandler: (() -> Void)?
    var settingsWindowPresentationHandler: (() -> Void)?

    init() {
        let credentialStore = KeychainCredentialStore()
        let apiClient = ChatOSAPIClient(
            configuration: .init(baseURL: RuntimeConfiguration.apiBaseURL),
            credentialStore: credentialStore
        )
        let authenticationService = ChatOSAuthenticationService(
            client: apiClient,
            credentialStore: credentialStore
        )
        let conversationService = ChatOSConversationService(client: apiClient)
        let historyStore = ConversationHistoryStore()
        let connectorTicketProvider = ChatOSLocalConnectorPairingTicketProvider(client: apiClient)
        let remoteConnectionService = NativeRemoteConnectionService(
            upstream: ChatOSRemoteConnectionService(client: apiClient),
            connectorStateURL: RuntimeConfiguration.nativeConnectorStateURL
        )
        let localConnectorService = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: RuntimeConfiguration.localConnectorCloudBaseURL,
                stateURL: RuntimeConfiguration.nativeConnectorStateURL,
                deploymentIdentifier: RuntimeConfiguration.deployment.identifier
            ),
            ticketProvider: connectorTicketProvider,
            remoteConnectionRuntime: remoteConnectionService,
            approvalMemoryProviderFactory: { tenantID, workspaceID, runID, runtimeScope in
                let scope = try AgentMemoryScope(
                    tenantID: tenantID, profile: "approval", projectID: workspaceID,
                    runID: runID, runtimeScope: runtimeScope
                )
                let memory = try await ChatOSMemoryEngineService(client: apiClient, scope: scope)
                return AgentMemoryContextProvider(scope: scope, service: memory)
            }
        )

        self.historyStore = historyStore
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
        self.mediaStudio = MediaStudioViewModel(
            service: ChatOSMediaGenerationService(client: apiClient),
            storyPlanner: remoteAgentServices
        )
        self.localConnectorService = localConnectorService
        self.conversationService = conversationService
        self.workspaceService = ChatOSWorkspaceService(client: apiClient)
        self.projectConversationService = ChatOSProjectConversationService(client: apiClient)
        let localProjectsService = NativeLocalProjectsService(
            connector: localConnectorService,
            databaseURL: RuntimeConfiguration.nativeConnectorStateURL.deletingLastPathComponent()
                .appendingPathComponent("Projects.sqlite3")
        )
        self.localProjectsService = localProjectsService
        let agentGroupChatService = NativeAgentGroupChatService(
            databaseURL: RuntimeConfiguration.nativeConnectorStateURL.deletingLastPathComponent()
                .appendingPathComponent("AgentGroupChat.sqlite3"),
            agentArtifactService: ChatOSAgentArtifactService(client: apiClient)
        )
        Task { await localConnectorService.setAgentGroupChatService(agentGroupChatService) }
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
        Task { await localConnectorService.setAgentGroupChatScheduler(agentGroupChatScheduler) }
        self.agentGroupChatBuilderService = LocalAgentBuilderService(
            groupChatService: agentGroupChatService,
            projectsService: localProjectsService,
            connectorService: localConnectorService,
            agentServices: agentServices,
            skillLibrary: agentSkillLibrary
        )
        let remoteFileService = NativeRemoteFileService(runtime: remoteConnectionService)
        self.remoteConnectionService = remoteConnectionService
        self.remoteFileService = remoteFileService
        self.remoteConnectionWorkspaceStore = RemoteConnectionWorkspaceStore(
            terminalService: remoteConnectionService,
            fileService: remoteFileService
        )
        self.projectFilesystemService = NativeProjectFilesystemService(connector: localConnectorService)
        self.projectCodeNavigationService = NativeProjectCodeNavigationService(connector: localConnectorService)
        self.projectGitService = NativeProjectGitService(connector: localConnectorService)
        self.notepadService = ChatOSNotepadService(client: apiClient)
        self.wechatCompanionService = ChatOSWeChatCompanionService(client: apiClient)
        self.userLanguagePreferencesService = ChatOSUserLanguagePreferencesService(client: apiClient)
        self.projectRunService = NativeProjectRunService(
            connector: localConnectorService,
            preferencesURL: RuntimeConfiguration.nativeConnectorStateURL
                .deletingLastPathComponent()
                .appendingPathComponent("ProjectRunSettings.json")
        )
        self.commandService = ChatOSConversationCommandService(client: apiClient)
        self.turnProcessService = ChatOSTurnProcessService(client: apiClient)
        self.messageTaskGraphService = ChatOSMessageTaskGraphService(client: apiClient)
        self.runtimeSettingsService = ChatOSConversationRuntimeSettingsService(client: apiClient)
        self.askUserPromptService = ChatOSAskUserPromptService(client: apiClient)
        self.petActivityInboxService = ChatOSPetActivityInboxService(client: apiClient)
        self.realtimeService = ChatOSRealtimeClient(
            apiClient: apiClient,
            conversationService: conversationService
        )
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
                guard let service = self?.localConnectorService else { return }
                Task { await service.prepareForSystemSleep() }
            }
            .store(in: &cancellables)
        NSWorkspace.shared.notificationCenter.publisher(for: NSWorkspace.didWakeNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.recoverLocalConnector(forceReconnect: true)
                self?.restartAgentHeartbeatCoordinator()
                self?.restartAgentArtifactSyncCoordinator()
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
                self?.recoverLocalConnector(forceReconnect: false)
                self?.restartAgentArtifactSyncCoordinator()
            }
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
        Task { [weak self, localConnectorService] in
            guard let self else { return }
            await localConnectorService.setCompanionRuntime(self)
        }
        authentication.start()
    }

}
