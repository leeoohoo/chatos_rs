import ChatOSAPI
import ChatOSConnector
import ChatOSCore
import AppKit
import Combine
import Foundation
import SwiftUI

@MainActor
final class AppModel: ObservableObject {
    @Published var selection: SidebarSelection?
    @Published var projectTab: ProjectWorkspaceTab = .messages
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
    @Published private(set) var isLanguagePreferencesLoading = false
    @Published private(set) var isLanguagePreferencesSaving = false
    @Published private(set) var languagePreferencesError: String?
    @Published private(set) var requestedConnectorSettingsTab: LocalConnectorControlTab?
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
    @Published private(set) var projectConversation: ConversationSessionViewModel?
    @Published private(set) var contactConversation: ConversationSessionViewModel?
    @Published private(set) var contacts: [ResourceItem] = []
    @Published private(set) var projects: [ResourceItem] = []
    @Published private(set) var workspaceProjects: [WorkspaceProject] = []
    @Published private(set) var workspaceContacts: [WorkspaceContact] = []
    @Published private(set) var remoteConnections: [RemoteConnection] = []
    @Published private(set) var isRemoteConnectionsLoading = false
    @Published private(set) var remoteConnectionsError: String?
    @Published private(set) var isWorkspaceLoading = false
    @Published private(set) var workspaceError: String?
    @Published private(set) var preparingProjectConversationIDs: Set<String> = []
    @Published private(set) var projectConversationPreparationErrors: [String: String] = [:]

    let historyStore: ConversationHistoryStore
    let authentication: AuthenticationViewModel
    let localConnectorControl: LocalConnectorControlCenterViewModel
    let visualSessionStore = VisualSessionPresentationStore()
    let petPreferences = PetPreferencesStore()
    let petOverlayStore = PetOverlayStore()
    let globalUtilityPreferences = GlobalUtilityPreferencesStore()
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

    private let conversationService: ChatOSConversationService
    let realtimeService: ChatOSRealtimeClient
    private let commandService: ChatOSConversationCommandService
    private let turnProcessService: ChatOSTurnProcessService
    let messageTaskGraphService: ChatOSMessageTaskGraphService
    let projectExecutionService: ChatOSProjectExecutionService
    private let runtimeSettingsService: ChatOSConversationRuntimeSettingsService
    private let askUserPromptService: ChatOSAskUserPromptService
    private let petActivityInboxService: ChatOSPetActivityInboxService
    private let workspaceService: ChatOSWorkspaceService
    private let localConnectorService: NativeLocalConnectorService
    let workspaceResourceCreationService: ChatOSWorkspaceResourceCreationService
    let remoteConnectionService: NativeRemoteConnectionService
    let remoteFileService: NativeRemoteFileService
    let projectFilesystemService: NativeProjectFilesystemService
    let projectCodeNavigationService: NativeProjectCodeNavigationService
    let projectGitService: NativeProjectGitService
    let projectPlanService: ChatOSProjectPlanService
    let projectRunService: NativeProjectRunService
    let notepadService: ChatOSNotepadService
    private let userLanguagePreferencesService: ChatOSUserLanguagePreferencesService
    private var conversationCache: [String: ConversationSessionViewModel] = [:]
    private var workspaceLoadGeneration: Int64 = 0
    private var visualSessionExpansion: [String: Bool] = [:]
    private var visualSessionSelection: [String: String] = [:]
    private var visualSessionMonitorTask: Task<Void, Never>?
    private var petOverlayCoordinator: PetOverlayCoordinator?
    private let idleSleepController = AppIdleSleepController()
    private var cancellables = Set<AnyCancellable>()
    private var authenticatedUserID: String?
    private var isApplyingLanguagePreferences = false
    private var languagePreferencesSaveTask: Task<Void, Never>?

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
            upstream: ChatOSRemoteConnectionService(client: apiClient)
        )
        let localConnectorService = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: RuntimeConfiguration.localConnectorCloudBaseURL,
                stateURL: RuntimeConfiguration.nativeConnectorStateURL
            ),
            ticketProvider: connectorTicketProvider,
            remoteConnectionRuntime: remoteConnectionService
        )

        self.historyStore = historyStore
        self.authentication = AuthenticationViewModel(service: authenticationService)
        self.localConnectorControl = LocalConnectorControlCenterViewModel(
            service: localConnectorService
        )
        self.localConnectorService = localConnectorService
        self.conversationService = conversationService
        self.workspaceService = ChatOSWorkspaceService(client: apiClient)
        self.workspaceResourceCreationService = ChatOSWorkspaceResourceCreationService(client: apiClient)
        self.remoteConnectionService = remoteConnectionService
        self.remoteFileService = NativeRemoteFileService(runtime: remoteConnectionService)
        self.projectFilesystemService = NativeProjectFilesystemService(connector: localConnectorService)
        self.projectCodeNavigationService = NativeProjectCodeNavigationService(connector: localConnectorService)
        self.projectGitService = NativeProjectGitService(connector: localConnectorService)
        self.projectPlanService = ChatOSProjectPlanService(client: apiClient)
        self.notepadService = ChatOSNotepadService(client: apiClient)
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
        self.projectExecutionService = ChatOSProjectExecutionService(client: apiClient)
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
            }
            .store(in: &cancellables)
        NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.recoverLocalConnector(forceReconnect: false)
            }
            .store(in: &cancellables)
        localConnectorControl.$status
            .map { $0?.connectorRunning == true }
            .removeDuplicates()
            .filter { $0 }
            .sink { [weak self] _ in
                self?.refreshWorkspace()
            }
            .store(in: &cancellables)
        $selection
            .removeDuplicates()
            .sink { [weak self] selection in
                self?.activateConversation(for: selection)
            }
            .store(in: &cancellables)
        visualSessionMonitorTask = Task { [weak self, localConnectorService] in
            while !Task.isCancelled {
                let selectedAdapterSessionID = self?.visualSessionStore.selectedAdapterSessionID
                let preferredAdapterSessionIDs = selectedAdapterSessionID.map { Set([$0]) } ?? []
                let sessions = await localConnectorService.fetchPluginVisualSessions(
                    loadFrameDataForAdapterSessionIDs: preferredAdapterSessionIDs
                )
                self?.applyPluginVisualSessions(sessions)
                try? await Task.sleep(for: .milliseconds(450))
            }
        }
        authentication.start()
    }

    var currentConversationID: String? {
        switch selection {
        case .contact:
            return contactConversation?.sessionID
        case .project:
            return projectConversation?.sessionID
        default:
            return nil
        }
    }

    var interfaceLocale: Locale {
        interfaceLanguage.locale
    }

    func localized(_ chinese: String, english: String) -> String {
        interfaceLanguage == .english ? english : chinese
    }

    func startPetOverlayIfNeeded() {
        guard petOverlayCoordinator == nil else { return }
        petOverlayCoordinator = PetOverlayCoordinator(
            model: self,
            store: petOverlayStore,
            preferences: petPreferences
        )
    }

    func startGlobalUtilitiesIfNeeded() {
        globalUtilityCoordinator.start()
    }

    func openPetActivity(_ activity: PetActivity?) {
        if activity?.source == .localApproval {
            requestConnectorSettings(.approvals)
            NSApp.activate(ignoringOtherApps: true)
            if !NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil) {
                _ = NSApp.sendAction(Selector(("showPreferencesWindow:")), to: nil, from: nil)
            }
            return
        }

        var targetConversation: ConversationSessionViewModel?
        if let projectID = activity?.route.projectID,
           let project = projects.first(where: { $0.id == projectID }) {
            selection = .project(projectID)
            projectTab = .messages
            if let conversationID = activity?.route.conversationID ?? project.conversationID {
                targetConversation = conversation(for: conversationID, allowsPlanMode: true)
                projectConversation = targetConversation
            }
        } else if let conversationID = activity?.route.conversationID {
            if let project = projects.first(where: { $0.conversationID == conversationID }) {
                selection = .project(project.id)
                projectTab = .messages
                targetConversation = conversation(for: conversationID, allowsPlanMode: true)
                projectConversation = targetConversation
            } else if let contact = contacts.first(where: { $0.conversationID == conversationID }) {
                selection = .contact(contact.id)
                targetConversation = conversation(for: conversationID, allowsPlanMode: false)
                contactConversation = targetConversation
            }
        }

        if let route = activity?.route {
            targetConversation?.focus(
                turnID: route.turnID,
                promptID: route.promptID,
                taskID: route.taskID,
                runID: route.runID
            )
        }

        NSApp.activate(ignoringOtherApps: true)
        let mainWindow = NSApp.windows.first {
            !($0 is NSPanel) && $0.title == "ChatOS"
        }
        mainWindow?.makeKeyAndOrderFront(nil)
    }

    func retryPetActivity(_ activity: PetActivity, instruction: String) async throws {
        guard let messageID = activity.route.messageID?.trimmingCharacters(in: .whitespacesAndNewlines),
              !messageID.isEmpty,
              let runID = activity.route.runID?.trimmingCharacters(in: .whitespacesAndNewlines),
              !runID.isEmpty else {
            throw PetActivityActionError.retryUnavailable
        }
        _ = try await messageTaskGraphService.retryRun(
            messageID: messageID,
            runID: runID,
            lookup: MessageTaskLookup(
                sessionID: activity.route.conversationID,
                turnID: activity.route.turnID
            ),
            instruction: instruction
        )
    }

    func cancelPetActivity(_ activity: PetActivity) async throws {
        if let messageID = activity.route.messageID?.trimmingCharacters(in: .whitespacesAndNewlines),
           !messageID.isEmpty,
           let taskID = activity.route.taskID?.trimmingCharacters(in: .whitespacesAndNewlines),
           !taskID.isEmpty {
            try await messageTaskGraphService.cancelTask(
                messageID: messageID,
                taskID: taskID,
                lookup: MessageTaskLookup(
                    sessionID: activity.route.conversationID,
                    turnID: activity.route.turnID
                ),
                reason: "用户从全局宠物面板取消任务"
            )
            return
        }

        if activity.source == .chat || activity.source == .projectExecution,
           let conversationID = activity.route.conversationID?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !conversationID.isEmpty,
           let turnID = (activity.source == .projectExecution
               ? activity.route.runID ?? activity.route.turnID
               : activity.route.turnID)?.trimmingCharacters(in: .whitespacesAndNewlines),
           !turnID.isEmpty {
            try await commandService.stopTurn(conversationID: conversationID, turnID: turnID)
            return
        }

        throw PetActivityActionError.cancelUnavailable
    }

    func loadPetTask(_ activity: PetActivity) async throws -> MessageTask {
        guard let messageID = activity.route.messageID?
            .trimmingCharacters(in: .whitespacesAndNewlines),
              !messageID.isEmpty,
              let taskID = activity.route.taskID?
                .trimmingCharacters(in: .whitespacesAndNewlines),
              !taskID.isEmpty else {
            throw PetActivityActionError.taskDetailUnavailable
        }
        let lookup = MessageTaskLookup(
            sessionID: activity.route.conversationID,
            turnID: activity.route.turnID
        )
        let task = try await messageTaskGraphService.fetchTask(
            messageID: messageID,
            taskID: taskID,
            lookup: lookup
        )
        guard let runID = (activity.route.runID ?? task.lastRunID)?
            .trimmingCharacters(in: .whitespacesAndNewlines),
              !runID.isEmpty else {
            return task
        }
        let runDetail = try? await messageTaskGraphService.fetchRun(
            messageID: messageID,
            runID: runID,
            lookup: lookup,
            includeEvents: true,
            eventLimit: 40,
            eventOffset: 0
        )
        guard let runDetail else { return task }
        var mergedTask = runDetail.task.merging(run: runDetail.run)
        let existingProcess = mergedTask.processLog?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if existingProcess.isEmpty, !runDetail.events.isEmpty {
            let formatter = ISO8601DateFormatter()
            mergedTask.processLog = runDetail.events.map { event in
                let timestamp = event.createdAt.map(formatter.string(from:)) ?? "事件"
                let title = event.eventType.trimmingCharacters(in: .whitespacesAndNewlines)
                let detail = event.message?
                    .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                return "[\(timestamp)] \(title.isEmpty ? "过程更新" : title)\n"
                    + (detail.isEmpty ? "已记录该执行事件" : detail)
            }
            .joined(separator: "\n")
        }
        return mergedTask
    }

    func loadPetAskUserPrompt(_ activity: PetActivity) async throws -> AskUserPrompt {
        guard let sessionID = activity.route.conversationID?
            .trimmingCharacters(in: .whitespacesAndNewlines),
              !sessionID.isEmpty,
              let promptID = activity.route.promptID?
                .trimmingCharacters(in: .whitespacesAndNewlines),
              !promptID.isEmpty else {
            throw PetActivityActionError.promptUnavailable
        }
        let prompts = try await askUserPromptService.fetchPrompts(sessionID: sessionID, limit: 100)
        guard let prompt = prompts.first(where: { $0.id == promptID && $0.status.isPending }) else {
            throw PetActivityActionError.promptResolved
        }
        return prompt
    }

    func submitPetAskUserPrompt(
        _ prompt: AskUserPrompt,
        submission: AskUserSubmission
    ) async throws {
        _ = try await askUserPromptService.submit(
            promptID: prompt.id,
            sessionID: prompt.sessionID,
            submission: submission
        )
    }

    func cancelPetAskUserPrompt(_ prompt: AskUserPrompt) async throws {
        _ = try await askUserPromptService.cancel(
            promptID: prompt.id,
            sessionID: prompt.sessionID
        )
    }

    func applyPetActivityDisposition(
        _ disposition: PetActivityDisposition,
        to activity: PetActivity
    ) async throws {
        try await petActivityInboxService.apply(disposition, to: activity)
    }

    func recoverPetActivities() async throws -> [PetActivity] {
        try await petActivityInboxService.fetchOpenActivities(limit: 500)
    }

    var interfaceDynamicTypeSize: DynamicTypeSize {
        switch Int(interfaceFontSize.rounded()) {
        case ...12: .small
        case 13: .medium
        case 14: .large
        case 15: .xLarge
        case 16: .xxLarge
        case 17: .xxxLarge
        default: .accessibility1
        }
    }

    func toggleVisualSession() {
        guard var visualSession = visualSessionStore.selectedPresentation else { return }
        visualSession.isExpanded.toggle()
        visualSessionExpansion[visualSession.session.adapterSessionID] = visualSession.isExpanded
        visualSessionStore.updatePresentation(visualSession)
    }

    func selectPreviousVisualSession() {
        selectVisualSession(offset: -1)
    }

    func selectNextVisualSession() {
        selectVisualSession(offset: 1)
    }

    private func applyPluginVisualSessions(_ sessions: [PluginVisualSession]) {
        guard let conversationID = currentConversationID else {
            visualSessionStore.update([], selectedAdapterSessionID: nil)
            return
        }

        let previousPresentations = Dictionary(uniqueKeysWithValues:
            visualSessionStore.presentations.map { ($0.session.adapterSessionID, $0) }
        )
        let matchingSessions = sessions
            .filter { $0.owner.conversationID == conversationID }
            .sorted { lhs, rhs in
                let lhsDate = lhs.capturedAt ?? .distantPast
                let rhsDate = rhs.capturedAt ?? .distantPast
                if lhsDate != rhsDate { return lhsDate > rhsDate }
                return lhs.adapterSessionID < rhs.adapterSessionID
            }

        let presentations = matchingSessions.map { incoming -> VisualSessionPresentation in
            var session = incoming
            if session.frameData == nil,
               let previous = previousPresentations[session.adapterSessionID],
               previous.session.frameSequence == session.frameSequence {
                session.frameData = previous.session.frameData
            }
            let key = session.adapterSessionID
            let isExpanded = visualSessionExpansion[key]
                ?? previousPresentations[key]?.isExpanded
                ?? true
            visualSessionExpansion[key] = isExpanded
            return .init(session: session, isExpanded: isExpanded)
        }

        let activeAdapterSessionIDs = Set(presentations.map(\.session.adapterSessionID))
        let rememberedSelection = visualSessionSelection[conversationID]
            ?? visualSessionStore.selectedAdapterSessionID
        let selectedAdapterSessionID = rememberedSelection.flatMap { candidate in
            activeAdapterSessionIDs.contains(candidate) ? candidate : nil
        } ?? presentations.first?.session.adapterSessionID
        visualSessionSelection[conversationID] = selectedAdapterSessionID
        visualSessionStore.update(
            presentations,
            selectedAdapterSessionID: selectedAdapterSessionID
        )

        let activeKeys = Set(sessions.map(\.adapterSessionID))
        visualSessionExpansion = visualSessionExpansion.filter { activeKeys.contains($0.key) }
    }

    private func selectVisualSession(offset: Int) {
        let presentations = visualSessionStore.presentations
        guard presentations.count > 1 else { return }
        let currentIndex = visualSessionStore.selectedIndex ?? 0
        let nextIndex = (currentIndex + offset + presentations.count) % presentations.count
        let nextAdapterSessionID = presentations[nextIndex].session.adapterSessionID
        visualSessionStore.select(adapterSessionID: nextAdapterSessionID)
        if let conversationID = currentConversationID {
            visualSessionSelection[conversationID] = nextAdapterSessionID
        }
    }

    func refreshWorkspace() {
        workspaceLoadGeneration += 1
        let generation = workspaceLoadGeneration
        isWorkspaceLoading = true
        workspaceError = nil

        Task {
            do {
                let snapshot = try await workspaceService.fetchWorkspace()
                guard generation == workspaceLoadGeneration else { return }
                await projectRunService.updateProjects(snapshot.projects)
                guard generation == workspaceLoadGeneration else { return }
                workspaceProjects = snapshot.projects
                workspaceContacts = snapshot.contacts
                let resources = WorkspaceResourceResolver.resolve(snapshot)
                contacts = resources.contacts
                projects = resources.projects
                reconcileSelection()
            } catch {
                guard generation == workspaceLoadGeneration else { return }
                workspaceError = error.localizedDescription
            }
            if generation == workspaceLoadGeneration {
                isWorkspaceLoading = false
            }
        }
    }

    func refreshAllResources() {
        refreshWorkspace()
        refreshRemoteConnections()
        localConnectorControl.refreshStatus()
    }

    private func recoverLocalConnector(forceReconnect: Bool) {
        let service = localConnectorService
        Task { [weak self] in
            await service.recoverGatewayConnection(forceReconnect: forceReconnect)
            try? await Task.sleep(for: .milliseconds(500))
            guard !Task.isCancelled else { return }
            await MainActor.run {
                self?.localConnectorControl.refreshStatus()
            }
        }
    }

    func requestConnectorSettings(_ tab: LocalConnectorControlTab) {
        requestedConnectorSettingsTab = tab
    }

    func consumeConnectorSettingsRequest() {
        requestedConnectorSettingsTab = nil
    }

    private func applyAuthenticationPhase(_ phase: AuthenticationViewModel.Phase) {
        switch phase {
        case let .authenticated(session):
            authenticatedUserID = session.user.id
            loadLanguagePreferences()
            localConnectorControl.activate(pairIfNeeded: true)
            refreshWorkspace()
            refreshRemoteConnections()
        case .signedOut:
            authenticatedUserID = nil
            languagePreferencesSaveTask?.cancel()
            isLanguagePreferencesLoading = false
            isLanguagePreferencesSaving = false
            languagePreferencesError = nil
            localConnectorControl.resetForSignedOut()
            workspaceLoadGeneration += 1
            contacts = []
            projects = []
            workspaceProjects = []
            workspaceContacts = []
            remoteConnections = []
            conversationCache = [:]
            projectConversation = nil
            contactConversation = nil
            preparingProjectConversationIDs = []
            projectConversationPreparationErrors = [:]
        case .restoring, .authenticating:
            break
        }
    }

    private func loadLanguagePreferences() {
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

    private func languagePreferenceDidChange() {
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

    private func applyLanguagePreferences(_ preferences: UserLanguagePreferences) {
        isApplyingLanguagePreferences = true
        interfaceLanguage = preferences.interfaceLanguage
        contextLanguage = preferences.internalContextLanguage
        isApplyingLanguagePreferences = false
        persistLanguagePreferencesLocally()
    }

    private func persistLanguagePreferencesLocally() {
        UserDefaults.standard.set(
            interfaceLanguage.rawValue,
            forKey: "ChatOS.interfaceLanguage"
        )
        UserDefaults.standard.set(
            contextLanguage.rawValue,
            forKey: "ChatOS.internalContextLanguage"
        )
    }

    func workspaceProject(id: String) -> WorkspaceProject? {
        workspaceProjects.first(where: { $0.id == id })
    }

    var defaultProjectContact: WorkspaceContact? {
        workspaceContacts.first {
            $0.name.trimmingCharacters(in: .whitespacesAndNewlines) == "叽咕狸"
                && $0.status?.lowercased() != "disabled"
        }
    }

    var petQuickChatResources: [PetQuickChatResource] {
        var resources: [PetQuickChatResource] = []
        let preferredContactID = defaultProjectContact?.id
        if let contact = contacts.first(where: { $0.id == preferredContactID })
            ?? contacts.first(where: {
                $0.title.trimmingCharacters(in: .whitespacesAndNewlines) == "叽咕狸"
            }) {
            resources.append(PetQuickChatResource(
                id: "contact:\(contact.id)",
                sourceID: contact.id,
                kind: .contact,
                title: contact.title,
                subtitle: contact.subtitle,
                conversationID: contact.conversationID
            ))
        }

        resources.append(contentsOf: projects
            .filter { petPreferences.isFavorite(projectID: $0.id) }
            .map { project in
                PetQuickChatResource(
                    id: "project:\(project.id)",
                    sourceID: project.id,
                    kind: .project,
                    title: project.title,
                    subtitle: project.subtitle,
                    conversationID: project.conversationID
                )
            })
        return resources
    }

    func petConversation(for resource: PetQuickChatResource) -> ConversationSessionViewModel? {
        guard let conversationID = resource.conversationID else {
            if resource.kind == .project {
                prepareProjectConversationIfNeeded(projectID: resource.sourceID)
            }
            return nil
        }
        return conversation(for: conversationID, allowsPlanMode: resource.allowsPlanMode)
    }

    func registerCreatedProject(_ project: WorkspaceProject) {
        if let index = workspaceProjects.firstIndex(where: { $0.id == project.id }) {
            workspaceProjects[index] = project
        } else {
            workspaceProjects.append(project)
        }
        let resource = ResourceItem(
            id: project.id,
            title: project.name,
            subtitle: project.displayRootPath ?? project.rootPath,
            conversationID: project.latestConversationID,
            contactName: defaultProjectContact?.name
        )
        if let index = projects.firstIndex(where: { $0.id == project.id }) {
            projects[index] = resource
        } else {
            projects.append(resource)
            projects.sort { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
        }
        selection = .project(project.id)
        projectTab = .directory
        refreshWorkspace()
    }

    func deleteProject(id: String) async throws {
        try await workspaceService.deleteProject(id: id)

        // A workspace refresh may already be in flight. Invalidate it so an
        // older response cannot resurrect the project after deletion.
        workspaceLoadGeneration += 1
        isWorkspaceLoading = false

        let conversationIDs = Set(
            workspaceProjects
                .filter { $0.id == id }
                .compactMap(\.latestConversationID)
        )
        workspaceProjects.removeAll { $0.id == id }
        projects.removeAll { $0.id == id }
        preparingProjectConversationIDs.remove(id)
        projectConversationPreparationErrors.removeValue(forKey: id)
        petPreferences.setFavorite(false, projectID: id)
        for conversationID in conversationIDs {
            conversationCache.removeValue(forKey: conversationID)
        }

        if selection == .project(id) {
            projectConversation = nil
            projectTab = .messages
        }
        await projectRunService.updateProjects(workspaceProjects)
        reconcileSelection()
    }

    func isPreparingProjectConversation(projectID: String) -> Bool {
        preparingProjectConversationIDs.contains(projectID)
    }

    func projectConversationPreparationError(projectID: String) -> String? {
        projectConversationPreparationErrors[projectID]
    }

    func retryProjectConversationPreparation(projectID: String) {
        projectConversationPreparationErrors[projectID] = nil
        prepareProjectConversationIfNeeded(projectID: projectID, force: true)
    }

    func refreshRemoteConnections() {
        isRemoteConnectionsLoading = true
        remoteConnectionsError = nil
        Task {
            do {
                remoteConnections = try await remoteConnectionService.listConnections()
                    .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
            } catch {
                remoteConnectionsError = error.localizedDescription
            }
            isRemoteConnectionsLoading = false
        }
    }

    func remoteConnection(id: String) -> RemoteConnection? {
        remoteConnections.first(where: { $0.id == id })
    }

    func registerRemoteConnection(_ connection: RemoteConnection) {
        if let index = remoteConnections.firstIndex(where: { $0.id == connection.id }) {
            remoteConnections[index] = connection
        } else {
            remoteConnections.append(connection)
        }
        remoteConnections.sort { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
        selection = .remote(connection.id)
    }

    func deleteRemoteConnection(id: String) async throws {
        try await remoteConnectionService.deleteConnection(id: id)
        remoteConnections.removeAll(where: { $0.id == id })
        if selection == .remote(id) {
            selection = projects.first.map { .project($0.id) }
                ?? contacts.first.map { .contact($0.id) }
        }
    }

    private func reconcileSelection() {
        if case let .project(id) = selection, projects.contains(where: { $0.id == id }) { return }
        if case let .contact(id) = selection, contacts.contains(where: { $0.id == id }) { return }
        if case let .remote(id) = selection,
           remoteConnections.contains(where: { $0.id == id }) { return }
        if case let .terminal(id) = selection,
           terminals.contains(where: { $0.id == id }) { return }
        if selection == .localConnector { return }
        selection = projects.first.map { .project($0.id) }
            ?? contacts.first.map { .contact($0.id) }
            ?? remoteConnections.first.map { .remote($0.id) }
            ?? terminals.first.map { .terminal($0.id) }
    }

    private func activateConversation(for selection: SidebarSelection?) {
        switch selection {
        case let .project(id):
            let conversationID = projects.first(where: { $0.id == id })?.conversationID
            projectConversation = conversationID.map {
                conversation(for: $0, allowsPlanMode: true)
            }
            contactConversation = nil
            if conversationID == nil {
                prepareProjectConversationIfNeeded(projectID: id)
            }
        case let .contact(id):
            let conversationID = contacts.first(where: { $0.id == id })?.conversationID
            contactConversation = conversationID.map {
                conversation(for: $0, allowsPlanMode: false)
            }
            projectConversation = nil
        default:
            projectConversation = nil
            contactConversation = nil
        }
    }

    private func prepareProjectConversationIfNeeded(projectID: String, force: Bool = false) {
        guard !preparingProjectConversationIDs.contains(projectID) else { return }
        guard force || projectConversationPreparationErrors[projectID] == nil else { return }
        guard let project = workspaceProject(id: projectID),
              let contact = defaultProjectContact else { return }

        preparingProjectConversationIDs.insert(projectID)
        projectConversationPreparationErrors[projectID] = nil
        Task {
            do {
                let conversationID = try await workspaceResourceCreationService.ensureConversation(
                    project: project,
                    contact: contact
                )
                applyPreparedConversation(
                    conversationID,
                    projectID: projectID,
                    contactName: contact.name
                )
            } catch {
                projectConversationPreparationErrors[projectID] = error.localizedDescription
            }
            preparingProjectConversationIDs.remove(projectID)
        }
    }

    private func applyPreparedConversation(
        _ conversationID: String,
        projectID: String,
        contactName: String
    ) {
        if let index = workspaceProjects.firstIndex(where: { $0.id == projectID }) {
            workspaceProjects[index].latestConversationID = conversationID
        }
        if let index = projects.firstIndex(where: { $0.id == projectID }) {
            let existing = projects[index]
            projects[index] = ResourceItem(
                id: existing.id,
                title: existing.title,
                subtitle: existing.subtitle,
                conversationID: conversationID,
                contactName: contactName
            )
        }
        projectConversationPreparationErrors[projectID] = nil
        if selection == .project(projectID) {
            projectConversation = conversation(for: conversationID, allowsPlanMode: true)
        }
    }

    private func conversation(
        for sessionID: String,
        allowsPlanMode: Bool
    ) -> ConversationSessionViewModel {
        if let cached = conversationCache[sessionID] {
            cached.allowsPlanMode = allowsPlanMode
            return cached
        }
        let created = ConversationSessionViewModel(
            sessionID: sessionID,
            allowsPlanMode: allowsPlanMode,
            initialTurns: [],
            historyStore: historyStore,
            remoteService: conversationService,
            realtimeService: realtimeService,
            commandService: commandService,
            turnProcessService: turnProcessService,
            messageTaskGraphService: messageTaskGraphService,
            projectExecutionService: projectExecutionService,
            runtimeSettingsService: runtimeSettingsService,
            askUserPromptService: askUserPromptService
        )
        conversationCache[sessionID] = created
        return created
    }
}

private enum PetActivityActionError: LocalizedError {
    case retryUnavailable
    case cancelUnavailable
    case promptUnavailable
    case promptResolved
    case taskDetailUnavailable

    var errorDescription: String? {
        switch self {
        case .retryUnavailable:
            "当前事件缺少重试所需的任务运行信息，请打开详情处理。"
        case .cancelUnavailable:
            "当前事件缺少取消任务所需的信息，请打开详情处理。"
        case .promptUnavailable:
            "当前提问缺少直接处理所需的信息，请打开详情处理。"
        case .promptResolved:
            "这个提问已经处理或失效。"
        case .taskDetailUnavailable:
            "当前事件缺少读取任务执行过程所需的信息。"
        }
    }
}
