import ChatOSCore
import ChatOSConnector
import Foundation

@MainActor
final class ConversationComposerState: ObservableObject {
    @Published var draft = ""
    @Published var attachments: [ConversationAttachmentDraft] = []
    @Published var attachmentError: String?
    @Published var selectedCapability: ConversationExplicitCapabilitySelection?
}

@MainActor
final class ConversationTimelineObservationState: ObservableObject {
    private(set) var revision = 0

    func invalidate() {
        objectWillChange.send()
        revision &+= 1
    }
}

@MainActor
final class ConversationSessionViewModel: ObservableObject {
    let sessionID: String

    @Published private(set) var turns: [ConversationTurn]
    @Published private(set) var hasOlder = false
    @Published private(set) var unreadNewerCount = 0
    @Published private(set) var isRefreshing = false
    @Published private(set) var isLoadingOlder = false
    @Published var isSending = false
    @Published private(set) var isUpdatingRuntimeSettings = false
    @Published private(set) var availableModels: [ConversationModelOption] = []
    @Published private(set) var selectedModelID: String?
    @Published private(set) var selectedRemoteConnectionID: String?
    @Published private(set) var selectedThinkingLevel: String?
    @Published private(set) var reasoningEnabled = false
    @Published private(set) var taskGraphAvailability: [String: Bool] = [:]
    @Published var historyError: String?
    @Published var runtimeSettingsError: String?
    @Published var sendError: String?
    @Published var selectedTurnID: String?
    @Published var askUserPrompts: [AskUserPrompt] = []
    @Published var submittingAskUserPromptIDs: Set<String> = []
    @Published var askUserPromptErrors: [String: String] = [:]
    @Published private(set) var focusRequest: ConversationFocusRequest?

    let composerState = ConversationComposerState()
    let timelineObservationState = ConversationTimelineObservationState()
    private(set) var timelineItems: [ConversationTimelineItem]
    private(set) var timelineItemsBuildCount = 0

    var draft: String {
        get { composerState.draft }
        set { composerState.draft = newValue }
    }

    var attachments: [ConversationAttachmentDraft] {
        get { composerState.attachments }
        set { composerState.attachments = newValue }
    }

    var attachmentError: String? {
        get { composerState.attachmentError }
        set { composerState.attachmentError = newValue }
    }

    var selectedCapability: ConversationExplicitCapabilitySelection? {
        get { composerState.selectedCapability }
        set { composerState.selectedCapability = newValue }
    }

    let historyStore: any ConversationHistoryStoring
    let sendUseCase: ConversationSendUseCase?
    let viewportUseCase: ConversationViewportUseCase
    let turnProcessService: (any TurnProcessServicing)?
    let messageTaskGraphService: (any MessageTaskGraphServicing)?
    private let historyUseCase: ConversationHistoryUseCase?
    let realtimeService: (any ConversationRealtimeStreaming)?
    private let runtimeSettingsService: (any ConversationRuntimeSettingsServicing)?
    let askUserPromptUseCase: ConversationAskUserPromptUseCase?
    private var olderCursor: String?
    private var requestGeneration: Int64 = 0
    private var inFlightOlderCursor: String?
    private var realtimeTask: Task<Void, Never>?
    private var bootstrapTask: Task<Void, Never>?
    var bootstrapState = ConversationBootstrapState()
    private var historyRetryTask: Task<Void, Never>?
    private var latestRefreshDebounceTask: Task<Void, Never>?
    private var olderHistoryLoadTask: Task<Void, Never>?
    private var latestRefreshState = ConversationLatestRefreshState()
    private var historyRetryAttempt = 0
    private var viewportMutationTasks: [
        ConversationViewportMutationKind: Task<Void, Never>
    ] = [:]
    var viewportMutationState = ConversationViewportMutationState()
    private var taskGraphAvailabilityTasks: [String: Task<Void, Never>] = [:]
    private var taskGraphAvailabilityRevisions: [String: Int64] = [:]
    var taskGraphAvailabilityState = ConversationTaskGraphAvailabilityState()
    var attachmentFileLoadTasks: [UUID: Task<Void, Never>] = [:]
    var pastedImageNormalizationTasks: [UUID: Task<Void, Never>] = [:]
    private var runtimeSettingsLoadGeneration: UInt64 = 0
    private var runtimeSettingsMutationTask: Task<Void, Never>?
    var runtimeSettingsMutationState = ConversationRuntimeSettingsMutationState()
    private var runtimeRecoveryTask: Task<Void, Never>?
    var runtimeRecoveryState = ConversationRuntimeRecoveryState()
    var sendTask: Task<Void, Never>?
    var sendRecoveryTask: Task<Void, Never>?
    var sendLifecycleState = ConversationSendLifecycleState()
    var activeSendSubmission: ConversationDraftSubmission?
    var activeSendOptimisticTurnID: String?
    var askUserPromptLifecycleState = ConversationAskUserPromptLifecycleState()
    var askUserPromptRefreshTask: Task<[AskUserPrompt], Error>?
    var askUserPromptActionTasks: [String: Task<Void, Never>] = [:]
    var activationState = ConversationSessionActivationState()
    private var isActive = false
    private(set) var isLocalAgentRuntimeReady: Bool

    var taskGraphAvailabilityCacheEntryCount: Int {
        Set(taskGraphAvailability.keys)
            .union(taskGraphAvailabilityTasks.keys)
            .union(taskGraphAvailabilityRevisions.keys)
            .count
    }

    init(
        sessionID: String,
        initialTurns: [ConversationTurn],
        historyStore: any ConversationHistoryStoring,
        localAgentRuntimeReady: Bool = true,
        remoteService: (any ConversationRemoteServicing)? = nil,
        realtimeService: (any ConversationRealtimeStreaming)? = nil,
        commandService: (any ConversationCommandServicing)? = nil,
        turnProcessService: (any TurnProcessServicing)? = nil,
        messageTaskGraphService: (any MessageTaskGraphServicing)? = nil,
        runtimeSettingsService: (any ConversationRuntimeSettingsServicing)? = nil,
        askUserPromptService: (any AskUserPromptServicing)? = nil
    ) {
        self.sessionID = sessionID
        self.turns = initialTurns
        self.timelineItems = ConversationTimelineItem.build(
            turns: initialTurns,
            promptsByTurnID: [:],
            unattachedPrompts: []
        )
        self.timelineItemsBuildCount = 1
        self.selectedTurnID = initialTurns.last?.id
        self.historyStore = historyStore
        self.viewportUseCase = ConversationViewportUseCase(historyStore: historyStore)
        self.isLocalAgentRuntimeReady = localAgentRuntimeReady
        self.historyUseCase = remoteService.map {
            ConversationHistoryUseCase(
                remoteService: $0,
                historyStore: historyStore
            )
        }
        self.realtimeService = realtimeService
        self.sendUseCase = commandService.map {
            ConversationSendUseCase(
                commandService: $0,
                historyStore: historyStore
            )
        }
        self.turnProcessService = turnProcessService
        self.messageTaskGraphService = messageTaskGraphService
        self.runtimeSettingsService = runtimeSettingsService
        self.askUserPromptUseCase = askUserPromptService.map {
            ConversationAskUserPromptUseCase(service: $0)
        }

        let bootstrapToken = bootstrapState.begin()
        let shouldLoadRuntimeServices = localAgentRuntimeReady
        bootstrapTask = Task { [weak self] in
            await self?.bootstrap(
                initialTurns: initialTurns,
                shouldLoadRuntimeServices: shouldLoadRuntimeServices,
                token: bootstrapToken
            )
        }
    }

    deinit {
        realtimeTask?.cancel()
        bootstrapTask?.cancel()
        historyRetryTask?.cancel()
        latestRefreshDebounceTask?.cancel()
        olderHistoryLoadTask?.cancel()
        taskGraphAvailabilityTasks.values.forEach { $0.cancel() }
        askUserPromptRefreshTask?.cancel()
        askUserPromptActionTasks.values.forEach { $0.cancel() }
        runtimeSettingsMutationTask?.cancel()
        runtimeRecoveryTask?.cancel()
        viewportMutationTasks.values.forEach { $0.cancel() }
        sendTask?.cancel()
        sendRecoveryTask?.cancel()
        attachmentFileLoadTasks.values.forEach { $0.cancel() }
        pastedImageNormalizationTasks.values.forEach { $0.cancel() }
    }

    func refreshLatest() {
        historyRetryTask?.cancel()
        historyRetryTask = nil
        historyRetryAttempt = 0
        enqueueLatestRefresh(presentation: .visible, debounce: false)
    }

    func activate() {
        guard !isActive else { return }
        isActive = true
        let activation = activationState.activate()
        guard isLocalAgentRuntimeReady else { return }
        preloadTaskGraphAvailability(for: turns)
        refreshLatestSilently()
        Task { [weak self] in
            await self?.refreshAskUserPrompts()
        }
        startRealtime(activation: activation)
    }

    func deactivate() {
        guard isActive else { return }
        isActive = false
        activationState.deactivate()
        realtimeTask?.cancel()
        realtimeTask = nil
        historyRetryTask?.cancel()
        historyRetryTask = nil
        latestRefreshDebounceTask?.cancel()
        latestRefreshDebounceTask = nil
        latestRefreshState.cancelScheduledAndPending()
        isRefreshing = false
        olderHistoryLoadTask?.cancel()
        olderHistoryLoadTask = nil
        isLoadingOlder = false
        inFlightOlderCursor = nil
        cancelViewportMutations()
        cancelTaskGraphAvailabilityRequests()
    }

    func localAgentRuntimeDidBecomeReady() {
        isLocalAgentRuntimeReady = true
        bootstrapState.reset()
        realtimeTask?.cancel()
        realtimeTask = nil
        historyRetryTask?.cancel()
        historyRetryTask = nil
        historyRetryAttempt = 0
        runtimeSettingsError = nil
        runtimeRecoveryTask?.cancel()
        let token = runtimeRecoveryState.begin()
        runtimeRecoveryTask = Task { [weak self] in
            guard let self else { return }
            async let settings: Void = loadRuntimeSettings()
            async let prompts: Void = refreshAskUserPrompts()
            _ = await (settings, prompts)
            guard !Task.isCancelled,
                  self.runtimeRecoveryState.accepts(token) else { return }
            guard self.runtimeSettingsError == nil,
                  self.isLocalAgentRuntimeReady else {
                self.finishRuntimeRecovery(token)
                return
            }
            self.preloadTaskGraphAvailability(for: self.turns)
            guard self.isActive,
                  let activation = self.activationState.activeSession else {
                self.finishRuntimeRecovery(token)
                return
            }
            self.refreshLatest()
            self.startRealtime(activation: activation)
            self.finishRuntimeRecovery(token)
        }
    }

    func localAgentRuntimeDidBecomeUnavailable() {
        isLocalAgentRuntimeReady = false
        bootstrapState.reset()
        runtimeRecoveryState.reset()
        runtimeRecoveryTask?.cancel()
        runtimeRecoveryTask = nil
        cancelActiveSendForRuntimeUnavailable()
        realtimeTask?.cancel()
        realtimeTask = nil
        historyRetryTask?.cancel()
        historyRetryTask = nil
        latestRefreshDebounceTask?.cancel()
        latestRefreshDebounceTask = nil
        latestRefreshState.cancelScheduledAndPending()
        olderHistoryLoadTask?.cancel()
        olderHistoryLoadTask = nil
        runtimeSettingsLoadGeneration &+= 1
        runtimeSettingsMutationState.reset()
        runtimeSettingsMutationTask?.cancel()
        runtimeSettingsMutationTask = nil
        askUserPromptLifecycleState.reset()
        askUserPromptRefreshTask?.cancel()
        askUserPromptRefreshTask = nil
        askUserPromptActionTasks.values.forEach { $0.cancel() }
        askUserPromptActionTasks.removeAll()
        cancelTaskGraphAvailabilityRequests()
        submittingAskUserPromptIDs = []
        askUserPromptErrors = [:]
        isRefreshing = false
        isLoadingOlder = false
        isUpdatingRuntimeSettings = false
        inFlightOlderCursor = nil
        historyError = nil
        runtimeSettingsError = nil
    }

    private func finishRuntimeRecovery(_ token: ConversationRuntimeRecoveryToken) {
        guard runtimeRecoveryState.finish(token) else { return }
        runtimeRecoveryTask = nil
    }

    func refreshLatestSilently() {
        enqueueLatestRefresh(presentation: .silent, debounce: true)
    }

    private func enqueueLatestRefresh(
        presentation: ConversationHistoryRefreshPresentation,
        debounce: Bool
    ) {
        guard isActive,
              isLocalAgentRuntimeReady,
              let activation = activationState.activeSession,
              let historyUseCase else { return }

        if debounce {
            let scheduledPresentation = latestRefreshState.scheduleDebounced(presentation)
            latestRefreshDebounceTask?.cancel()
            latestRefreshDebounceTask = Task { [weak self] in
                do {
                    try await Task.sleep(for: ConversationLatestRefreshState.debounceDelay)
                } catch {
                    return
                }
                guard !Task.isCancelled,
                      let self,
                      self.activationState.accepts(activation),
                      self.isLocalAgentRuntimeReady else { return }
                self.latestRefreshDebounceTask = nil
                self.latestRefreshState.consumeDebounced()
                self.performLatestRefresh(
                    using: historyUseCase,
                    presentation: scheduledPresentation,
                    activation: activation
                )
            }
            return
        }

        latestRefreshDebounceTask?.cancel()
        latestRefreshDebounceTask = nil
        latestRefreshState.consumeDebounced()
        performLatestRefresh(
            using: historyUseCase,
            presentation: presentation,
            activation: activation
        )
    }

    private func performLatestRefresh(
        using historyUseCase: ConversationHistoryUseCase,
        presentation: ConversationHistoryRefreshPresentation,
        activation: ConversationSessionActivation
    ) {
        guard activationState.accepts(activation),
              latestRefreshState.beginOrQueue(presentation) else { return }

        requestGeneration += 1
        let generation = requestGeneration
        if presentation == .visible {
            isRefreshing = true
            historyError = nil
        }

        Task {
            do {
                let snapshot = try await historyUseCase.loadLatest(
                    sessionID: sessionID,
                    requestGeneration: generation
                )
                if activationState.accepts(activation), isLocalAgentRuntimeReady {
                    applyHistorySnapshot(snapshot)
                    if historyError != nil {
                        historyError = nil
                    }
                    historyRetryAttempt = 0
                    historyRetryTask?.cancel()
                    historyRetryTask = nil
                }
            } catch {
                if activationState.accepts(activation), isLocalAgentRuntimeReady {
                    if historyError != error.localizedDescription {
                        historyError = error.localizedDescription
                    }
                    scheduleHistoryRetry()
                }
            }
            if presentation == .visible,
               activationState.accepts(activation) {
                isRefreshing = false
            }
            if let pending = latestRefreshState.complete() {
                enqueueLatestRefresh(presentation: pending, debounce: true)
            }
        }
    }

    private func scheduleHistoryRetry() {
        guard isActive,
              isLocalAgentRuntimeReady,
              historyRetryTask == nil,
              let delay = ConversationHistoryUseCase.retryDelay(
                at: historyRetryAttempt
              ) else { return }
        historyRetryAttempt += 1
        historyRetryTask = Task { [weak self] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled, let self else { return }
            self.historyRetryTask = nil
            self.enqueueLatestRefresh(presentation: .silent, debounce: false)
        }
    }

    func loadOlder() {
        guard isActive,
              isLocalAgentRuntimeReady,
              let activation = activationState.activeSession,
              let historyUseCase,
              let cursor = olderCursor,
              hasOlder,
              !isLoadingOlder,
              inFlightOlderCursor != cursor else { return }

        requestGeneration += 1
        let generation = requestGeneration
        inFlightOlderCursor = cursor
        isLoadingOlder = true
        historyError = nil
        timelineObservationState.invalidate()

        olderHistoryLoadTask = Task { [weak self] in
            guard let self else { return }
            do {
                let snapshot = try await historyUseCase.loadOlder(
                    sessionID: self.sessionID,
                    before: cursor,
                    requestGeneration: generation
                )
                guard !Task.isCancelled,
                      self.activationState.accepts(activation),
                      self.isLocalAgentRuntimeReady,
                      self.inFlightOlderCursor == cursor else { return }
                self.applyHistorySnapshot(snapshot)
            } catch {
                guard !Task.isCancelled,
                      self.activationState.accepts(activation),
                      self.isLocalAgentRuntimeReady,
                      self.inFlightOlderCursor == cursor else { return }
                self.historyError = error.localizedDescription
            }
            guard self.activationState.accepts(activation),
                  self.inFlightOlderCursor == cursor else { return }
            self.inFlightOlderCursor = nil
            self.isLoadingOlder = false
            self.olderHistoryLoadTask = nil
            self.timelineObservationState.invalidate()
        }
    }

    func markNewerContentRead() {
        guard let activation = activationState.activeSession else { return }
        let kind = ConversationViewportMutationKind.markNewerContentRead
        viewportMutationTasks[kind]?.cancel()
        let token = viewportMutationState.begin(kind)
        viewportMutationTasks[kind] = Task { [weak self] in
            guard let self else { return }
            let snapshot = await self.viewportUseCase.markNewerContentRead(
                sessionID: self.sessionID
            )
            guard !Task.isCancelled,
                  self.activationState.accepts(activation),
                  self.viewportMutationState.accepts(token) else { return }
            self.applyHistorySnapshot(snapshot)
            self.finishViewportMutation(token)
        }
    }

    func focus(
        turnID: String?,
        promptID: String?,
        taskID: String?,
        runID: String?
    ) {
        selectedTurnID = turnID
        focusRequest = ConversationFocusRequest(
            turnID: turnID,
            promptID: promptID,
            taskID: taskID,
            runID: runID
        )
        timelineObservationState.invalidate()
    }

    func focusAfterRefreshingAskUserPrompts(
        turnID: String?,
        promptID: String?,
        taskID: String?,
        runID: String?
    ) async {
        if promptID != nil {
            await refreshAskUserPrompts()
        }
        guard !Task.isCancelled else { return }
        focus(turnID: turnID, promptID: promptID, taskID: taskID, runID: runID)
    }

    func consumeFocusRequest(id: UUID) {
        guard focusRequest?.id == id else { return }
        focusRequest = nil
        timelineObservationState.invalidate()
    }

    func setTimelinePinnedToBottom(_ isPinned: Bool) {
        guard let activation = activationState.activeSession else { return }
        let kind = ConversationViewportMutationKind.timelineAnchor
        let latestTurnID = turns.last?.id
        viewportMutationTasks[kind]?.cancel()
        let token = viewportMutationState.begin(kind)
        viewportMutationTasks[kind] = Task { [weak self] in
            guard let self else { return }
            let snapshot = await self.viewportUseCase.updateTimelineAnchor(
                sessionID: self.sessionID,
                latestTurnID: latestTurnID,
                isPinnedToBottom: isPinned
            )
            guard !Task.isCancelled,
                  self.activationState.accepts(activation),
                  self.viewportMutationState.accepts(token) else { return }
            self.applyHistorySnapshot(snapshot)
            self.finishViewportMutation(token)
        }
    }

    private func finishViewportMutation(_ token: ConversationViewportMutationToken) {
        guard viewportMutationState.finish(token) else { return }
        viewportMutationTasks[token.kind] = nil
    }

    private func cancelViewportMutations() {
        viewportMutationState.reset()
        viewportMutationTasks.values.forEach { $0.cancel() }
        viewportMutationTasks.removeAll()
    }

    func hasTaskGraph(for turn: ConversationTurn) -> Bool {
        taskGraphAvailability[turn.id] == true
    }

    func resolveTaskGraphAvailability(for turn: ConversationTurn) {
        let isCandidate = turn.isTaskGraphAvailable && turn.messageTaskLookup != nil
        guard isCandidate, let messageTaskGraphService else {
            let hadVisibleGraph = taskGraphAvailability[turn.id] != nil
            guard taskGraphAvailabilityRevisions[turn.id] != turn.revision
                    || hadVisibleGraph
                    || taskGraphAvailabilityTasks[turn.id] != nil else { return }
            taskGraphAvailability.removeValue(forKey: turn.id)
            taskGraphAvailabilityRevisions[turn.id] = turn.revision
            taskGraphAvailabilityState.invalidate(turnID: turn.id)
            taskGraphAvailabilityTasks[turn.id]?.cancel()
            taskGraphAvailabilityTasks[turn.id] = nil
            if hadVisibleGraph {
                timelineObservationState.invalidate()
            }
            return
        }
        guard isLocalAgentRuntimeReady else {
            taskGraphAvailabilityState.invalidate(turnID: turn.id)
            taskGraphAvailabilityTasks[turn.id]?.cancel()
            taskGraphAvailabilityTasks[turn.id] = nil
            taskGraphAvailabilityRevisions[turn.id] = nil
            return
        }
        guard taskGraphAvailabilityRevisions[turn.id] != turn.revision else { return }

        taskGraphAvailabilityRevisions[turn.id] = turn.revision
        let token = taskGraphAvailabilityState.begin(
            turnID: turn.id,
            revision: turn.revision
        )
        let hadVisibleGraph = taskGraphAvailability.removeValue(forKey: turn.id) != nil
        taskGraphAvailabilityTasks[turn.id]?.cancel()
        if hadVisibleGraph {
            timelineObservationState.invalidate()
        }
        taskGraphAvailabilityTasks[turn.id] = Task { [weak self] in
            do {
                let graph = try await messageTaskGraphService.fetchGraph(
                    messageID: turn.userMessage.id,
                    lookup: turn.resolvedMessageTaskLookup
                )
                guard !Task.isCancelled,
                      let self,
                      self.isLocalAgentRuntimeReady,
                      self.taskGraphAvailabilityState.accepts(token) else {
                    return
                }
                if graph.nodes.isEmpty {
                    if self.taskGraphAvailability.removeValue(forKey: turn.id) != nil {
                        self.timelineObservationState.invalidate()
                    }
                } else {
                    if self.taskGraphAvailability[turn.id] != true {
                        self.taskGraphAvailability[turn.id] = true
                        self.timelineObservationState.invalidate()
                    }
                }
            } catch {
                guard !Task.isCancelled,
                      let self,
                      self.isLocalAgentRuntimeReady,
                      self.taskGraphAvailabilityState.accepts(token) else {
                    return
                }
                if self.taskGraphAvailability.removeValue(forKey: turn.id) != nil {
                    self.timelineObservationState.invalidate()
                }
            }
            guard let self,
                  self.taskGraphAvailabilityState.finish(token) else { return }
            self.taskGraphAvailabilityTasks[turn.id] = nil
        }
    }

    private func cancelTaskGraphAvailabilityRequests() {
        let activeTurnIDs = Set(taskGraphAvailabilityTasks.keys)
        taskGraphAvailabilityTasks.values.forEach { $0.cancel() }
        taskGraphAvailabilityTasks.removeAll()
        taskGraphAvailabilityState.reset()
        for turnID in activeTurnIDs {
            taskGraphAvailabilityRevisions[turnID] = nil
        }
    }

    private func bootstrap(
        initialTurns: [ConversationTurn],
        shouldLoadRuntimeServices: Bool,
        token: ConversationBootstrapToken
    ) async {
        if shouldLoadRuntimeServices, isLocalAgentRuntimeReady {
            async let runtimeSettings: Void = loadRuntimeSettings(presentErrors: false)
            async let prompts: Void = refreshAskUserPrompts()
            _ = await (runtimeSettings, prompts)
        }
        await historyStore.mergeCachedTurns(initialTurns, sessionID: sessionID)
        await refreshSnapshot()
        let canFinish = bootstrapState.finish(token)
        bootstrapTask = nil
        if canFinish,
           isActive,
           isLocalAgentRuntimeReady,
           let activation = activationState.activeSession {
            refreshLatest()
            startRealtime(activation: activation)
        }
    }

    var selectedModelDisplayName: String {
        if let selectedModelID,
           let selected = availableModels.first(where: { $0.id == selectedModelID }) {
            return selected.displayName
        }
        return availableModels.first?.displayName ?? "选择模型"
    }

    var selectedModelOption: ConversationModelOption? {
        guard let selectedModelID else { return availableModels.first }
        return availableModels.first(where: { $0.id == selectedModelID })
            ?? availableModels.first
    }

    var reasoningLevels: [String] {
        guard let selectedModelOption, selectedModelOption.supportsReasoning else { return [] }
        return selectedModelOption.thinkingLevels
    }

    var effectiveReasoningLevel: String {
        guard reasoningEnabled else { return "none" }
        if let selectedThinkingLevel, reasoningLevels.contains(selectedThinkingLevel) {
            return selectedThinkingLevel
        }
        return enabledReasoningLevel
    }

    var defaultReasoningLevel: String {
        if let configured = selectedModelOption?.thinkingLevel,
           reasoningLevels.contains(configured) {
            return configured
        }
        if reasoningLevels.contains("auto") { return "auto" }
        if reasoningLevels.contains("medium") { return "medium" }
        return reasoningLevels.first(where: { $0 != "none" }) ?? "none"
    }

    private var enabledReasoningLevel: String {
        if defaultReasoningLevel != "none" { return defaultReasoningLevel }
        if reasoningLevels.contains("auto") { return "auto" }
        if reasoningLevels.contains("medium") { return "medium" }
        return reasoningLevels.first(where: { $0 != "none" }) ?? "none"
    }

    func setSelectedModelID(_ modelID: String) {
        guard let runtimeSettingsService,
              availableModels.contains(where: { $0.id == modelID }),
              selectedModelID != modelID else { return }
        let previous = selectedModelID
        let token = beginRuntimeSettingsMutation()
        selectedModelID = modelID
        runtimeSettingsError = nil
        runtimeSettingsMutationTask = Task { [weak self] in
            guard let self else { return }
            do {
                let settings = try await runtimeSettingsService.updateModel(
                    sessionID: self.sessionID,
                    modelID: modelID
                )
                guard !Task.isCancelled,
                      self.runtimeSettingsMutationState.accepts(token) else { return }
                self.applyRuntimeSettings(settings)
            } catch {
                guard !Task.isCancelled,
                      self.runtimeSettingsMutationState.accepts(token) else { return }
                self.selectedModelID = previous
                self.runtimeSettingsError = error.localizedDescription
            }
            self.finishRuntimeSettingsMutation(token)
        }
    }

    func setReasoningEnabled(_ enabled: Bool) {
        setReasoningLevel(enabled ? enabledReasoningLevel : "none")
    }

    func resetReasoningLevel() {
        setReasoningLevel(defaultReasoningLevel)
    }

    func setReasoningLevel(_ level: String) {
        guard let runtimeSettingsService,
              reasoningLevels.contains(level) else { return }
        let enabled = level != "none"
        guard selectedThinkingLevel != level || reasoningEnabled != enabled else { return }
        let previousLevel = selectedThinkingLevel
        let previous = reasoningEnabled
        let token = beginRuntimeSettingsMutation()
        selectedThinkingLevel = level
        reasoningEnabled = enabled
        runtimeSettingsError = nil
        runtimeSettingsMutationTask = Task { [weak self] in
            guard let self else { return }
            do {
                let settings = try await runtimeSettingsService.updateReasoningLevel(
                    sessionID: self.sessionID,
                    level: level,
                    enabled: enabled
                )
                guard !Task.isCancelled,
                      self.runtimeSettingsMutationState.accepts(token) else { return }
                self.applyRuntimeSettings(settings)
            } catch {
                guard !Task.isCancelled,
                      self.runtimeSettingsMutationState.accepts(token) else { return }
                self.selectedThinkingLevel = previousLevel
                self.reasoningEnabled = previous
                self.runtimeSettingsError = error.localizedDescription
            }
            self.finishRuntimeSettingsMutation(token)
        }
    }

    func setRemoteConnectionID(_ connectionID: String?) {
        guard let runtimeSettingsService,
              selectedRemoteConnectionID != connectionID else { return }
        let previous = selectedRemoteConnectionID
        let token = beginRuntimeSettingsMutation()
        selectedRemoteConnectionID = connectionID
        runtimeSettingsError = nil
        runtimeSettingsMutationTask = Task { [weak self] in
            guard let self else { return }
            do {
                let settings = try await runtimeSettingsService.updateRemoteConnection(
                    sessionID: self.sessionID,
                    connectionID: connectionID
                )
                guard !Task.isCancelled,
                      self.runtimeSettingsMutationState.accepts(token) else { return }
                self.applyRuntimeSettings(settings)
            } catch {
                guard !Task.isCancelled,
                      self.runtimeSettingsMutationState.accepts(token) else { return }
                self.selectedRemoteConnectionID = previous
                self.runtimeSettingsError = error.localizedDescription
            }
            self.finishRuntimeSettingsMutation(token)
        }
    }

    private func beginRuntimeSettingsMutation() -> ConversationRuntimeSettingsMutationToken {
        runtimeSettingsLoadGeneration &+= 1
        runtimeSettingsMutationTask?.cancel()
        isUpdatingRuntimeSettings = true
        return runtimeSettingsMutationState.begin()
    }

    private func finishRuntimeSettingsMutation(
        _ token: ConversationRuntimeSettingsMutationToken
    ) {
        guard runtimeSettingsMutationState.finish(token) else { return }
        runtimeSettingsMutationTask = nil
        isUpdatingRuntimeSettings = false
    }

    private func loadRuntimeSettings(presentErrors: Bool = true) async {
        guard let runtimeSettingsService else { return }
        runtimeSettingsLoadGeneration &+= 1
        let generation = runtimeSettingsLoadGeneration
        do {
            async let settings = runtimeSettingsService.fetchSettings(sessionID: sessionID)
            async let models = runtimeSettingsService.fetchAvailableModels()
            let (resolvedSettings, resolvedModels) = try await (settings, models)
            guard generation == runtimeSettingsLoadGeneration else { return }
            availableModels = resolvedModels
            applyRuntimeSettings(resolvedSettings)
            runtimeSettingsError = nil
        } catch NativeLocalAgentConversationRuntimeSettingsError.notConfigured {
            guard generation == runtimeSettingsLoadGeneration else { return }
            runtimeSettingsError = nil
        } catch {
            guard generation == runtimeSettingsLoadGeneration else { return }
            if presentErrors {
                runtimeSettingsError = error.localizedDescription
            }
        }
    }

    private func applyRuntimeSettings(_ settings: ConversationRuntimeSettings) {
        if let requestedID = settings.selectedModelID,
           availableModels.contains(where: { $0.id == requestedID }) {
            selectedModelID = requestedID
        } else {
            selectedModelID = availableModels.first?.id
        }
        selectedRemoteConnectionID = settings.remoteConnectionID
        selectedThinkingLevel = settings.selectedThinkingLevel
        reasoningEnabled = settings.reasoningEnabled
    }

    private func startRealtime(activation: ConversationSessionActivation) {
        guard activationState.accepts(activation),
              let realtimeService,
              realtimeTask == nil else { return }
        let sessionID = sessionID

        realtimeTask = Task { [weak self] in
            let stream = await realtimeService.events(sessionID: sessionID)
            guard let self,
                  !Task.isCancelled,
                  self.activationState.accepts(activation),
                  self.isLocalAgentRuntimeReady else { return }
            do {
                for try await signal in stream {
                    guard !Task.isCancelled,
                          self.activationState.accepts(activation),
                          self.isLocalAgentRuntimeReady else { return }
                    switch ConversationRealtimePolicy.action(for: signal) {
                    case .refreshAskUserPrompts:
                        await self.refreshAskUserPrompts()
                    case let .failure(message):
                        self.sendError = message
                        self.refreshLatestSilently()
                    case .refreshHistory:
                        self.refreshLatestSilently()
                    case .ignore:
                        break
                    }
                }
            } catch {
                guard !Task.isCancelled,
                      self.activationState.accepts(activation),
                      self.isLocalAgentRuntimeReady else { return }
                self.historyError = error.localizedDescription
            }
            guard !Task.isCancelled,
                  self.activationState.accepts(activation),
                  self.isLocalAgentRuntimeReady else { return }
            self.realtimeTask = nil
        }
    }

    func refreshSnapshot() async {
        let snapshot = await historyStore.snapshot(sessionID: sessionID)
        applyHistorySnapshot(snapshot)
    }

    func applyHistorySnapshot(_ snapshot: ConversationHistorySnapshot) {
        var timelineChanged = false
        if turns != snapshot.turns {
            turns = snapshot.turns
            rebuildTimelineItems()
            timelineChanged = true
        }
        preloadTaskGraphAvailability(for: snapshot.turns)
        olderCursor = snapshot.olderCursor
        if hasOlder != snapshot.hasOlder {
            hasOlder = snapshot.hasOlder
            timelineChanged = true
        }
        if unreadNewerCount != snapshot.unreadNewerCount {
            unreadNewerCount = snapshot.unreadNewerCount
            timelineChanged = true
        }
        if timelineChanged {
            timelineObservationState.invalidate()
        }
    }

    func replaceAskUserPrompts(_ prompts: [AskUserPrompt]) {
        guard askUserPrompts != prompts else { return }
        askUserPrompts = prompts
        rebuildTimelineItems()
        timelineObservationState.invalidate()
    }

    private func rebuildTimelineItems() {
        let promptsByTurnID = Dictionary(uniqueKeysWithValues: turns.map {
            ($0.id, prompts(for: $0.id))
        })
        timelineItems = ConversationTimelineItem.build(
            turns: turns,
            promptsByTurnID: promptsByTurnID,
            unattachedPrompts: unattachedPendingPrompts
        )
        timelineItemsBuildCount += 1
    }

    private func preloadTaskGraphAvailability(for turns: [ConversationTurn]) {
        let currentTurnIDs = Set(turns.map(\.id))
        let cachedTurnIDs = Set(taskGraphAvailability.keys)
            .union(taskGraphAvailabilityTasks.keys)
            .union(taskGraphAvailabilityRevisions.keys)
        let staleTurnIDs = cachedTurnIDs.subtracting(currentTurnIDs)
        for turnID in staleTurnIDs {
            let hadVisibleGraph = taskGraphAvailability[turnID] != nil
            taskGraphAvailabilityTasks[turnID]?.cancel()
            taskGraphAvailabilityTasks[turnID] = nil
            taskGraphAvailabilityState.invalidate(turnID: turnID)
            taskGraphAvailabilityRevisions[turnID] = nil
            taskGraphAvailability[turnID] = nil
            if hadVisibleGraph {
                timelineObservationState.invalidate()
            }
        }
        for turn in turns {
            resolveTaskGraphAvailability(for: turn)
        }
    }
}
