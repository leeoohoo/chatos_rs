import ChatOSCore
import ChatOSConnector
import Foundation

private typealias ConversationRuntimeSettingsLoadRequest = (
    token: ConversationRuntimeSettingsLoadToken,
    task: Task<(ConversationRuntimeSettings, [ConversationModelOption]), Error>
)

private func awaitRuntimeSettingsLoad(
    _ task: Task<(ConversationRuntimeSettings, [ConversationModelOption]), Error>
) async -> Result<(ConversationRuntimeSettings, [ConversationModelOption]), Error> {
    do {
        let value = try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
        try Task.checkCancellation()
        return .success(value)
    } catch {
        return .failure(error)
    }
}

private func awaitRuntimeSettingsLoadRequest(
    _ request: ConversationRuntimeSettingsLoadRequest?
) async -> Result<(ConversationRuntimeSettings, [ConversationModelOption]), Error>? {
    guard let request else { return nil }
    return await awaitRuntimeSettingsLoad(request.task)
}

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
    private var latestRefreshTask: Task<Void, Never>?
    private var olderHistoryLoadTask: Task<Void, Never>?
    private var latestRefreshState = ConversationLatestRefreshState()
    var historyRetryState = ConversationHistoryRetryState()
    private var viewportMutationTasks: [
        ConversationViewportMutationKind: Task<Void, Never>
    ] = [:]
    var viewportMutationState = ConversationViewportMutationState()
    private var taskGraphAvailabilityTasks: [String: Task<Void, Never>] = [:]
    private var taskGraphAvailabilityRevisions: [String: Int64] = [:]
    var taskGraphAvailabilityState = ConversationTaskGraphAvailabilityState()
    var attachmentFileLoadTasks: [UUID: Task<Void, Never>] = [:]
    var pastedImageNormalizationTasks: [UUID: Task<Void, Never>] = [:]
    var attachmentImportState = ConversationAttachmentImportState()
    var runtimeSettingsLoadState = ConversationRuntimeSettingsLoadState()
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
    private var askUserPromptActivationTask: Task<Void, Never>?
    var askUserPromptRefreshTask: Task<[AskUserPrompt], Error>?
    var askUserPromptActionTasks: [String: Task<Void, Never>] = [:]
    private var focusAfterPromptRefreshTask: Task<Void, Never>?
    var focusRefreshState = ConversationFocusRefreshState()
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
        let settingsRequest = shouldLoadRuntimeServices ? beginRuntimeSettingsLoad() : nil
        let promptRequest = shouldLoadRuntimeServices ? beginAskUserPromptRefresh() : nil
        let historyStore = historyStore
        let sessionID = sessionID
        bootstrapTask = Task { [weak self] in
            async let settingsResult = awaitRuntimeSettingsLoadRequest(settingsRequest)
            async let promptResult = awaitAskUserPromptRefreshRequest(promptRequest)
            let (resolvedSettings, resolvedPrompts) = await (settingsResult, promptResult)

            if let self {
                if let settingsRequest, let resolvedSettings {
                    self.finishRuntimeSettingsLoad(
                        resolvedSettings,
                        token: settingsRequest.token,
                        presentErrors: false
                    )
                }
                if let promptRequest, let resolvedPrompts {
                    self.finishAskUserPromptRefresh(
                        resolvedPrompts,
                        token: promptRequest.token
                    )
                }
            }

            await historyStore.mergeCachedTurns(initialTurns, sessionID: sessionID)
            let snapshot = await historyStore.snapshot(sessionID: sessionID)
            guard let self else { return }
            self.applyHistorySnapshot(snapshot)
            let canFinish = self.bootstrapState.finish(bootstrapToken)
            self.bootstrapTask = nil
            if canFinish,
               self.isActive,
               self.isLocalAgentRuntimeReady,
               let activation = self.activationState.activeSession {
                self.refreshLatest()
                self.startRealtime(activation: activation)
            }
        }
    }

    deinit {
        realtimeTask?.cancel()
        bootstrapTask?.cancel()
        historyRetryTask?.cancel()
        latestRefreshDebounceTask?.cancel()
        latestRefreshTask?.cancel()
        olderHistoryLoadTask?.cancel()
        taskGraphAvailabilityTasks.values.forEach { $0.cancel() }
        askUserPromptActivationTask?.cancel()
        askUserPromptRefreshTask?.cancel()
        askUserPromptActionTasks.values.forEach { $0.cancel() }
        focusAfterPromptRefreshTask?.cancel()
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
        historyRetryState.reset()
        enqueueLatestRefresh(presentation: .visible, debounce: false)
    }

    func activate() {
        guard !isActive else { return }
        isActive = true
        let activation = activationState.activate()
        guard isLocalAgentRuntimeReady else { return }
        preloadTaskGraphAvailability(for: turns)
        refreshLatestSilently()
        askUserPromptActivationTask?.cancel()
        if let request = beginAskUserPromptRefresh() {
            askUserPromptActivationTask = Task { [weak self] in
                let result = await awaitAskUserPromptRefresh(request.task)
                guard !Task.isCancelled,
                      let self,
                      self.activationState.accepts(activation) else { return }
                self.finishAskUserPromptRefresh(result, token: request.token)
                guard self.activationState.accepts(activation) else { return }
                self.askUserPromptActivationTask = nil
            }
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
        historyRetryState.cancelScheduled()
        latestRefreshDebounceTask?.cancel()
        latestRefreshDebounceTask = nil
        latestRefreshState.cancelScheduledAndPending()
        isRefreshing = false
        olderHistoryLoadTask?.cancel()
        olderHistoryLoadTask = nil
        isLoadingOlder = false
        inFlightOlderCursor = nil
        askUserPromptActivationTask?.cancel()
        askUserPromptActivationTask = nil
        askUserPromptLifecycleState.invalidateRefresh()
        askUserPromptRefreshTask?.cancel()
        askUserPromptRefreshTask = nil
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
        historyRetryState.reset()
        runtimeSettingsError = nil
        runtimeRecoveryTask?.cancel()
        let token = runtimeRecoveryState.begin()
        let settingsRequest = beginRuntimeSettingsLoad()
        let promptRequest = beginAskUserPromptRefresh()
        runtimeRecoveryTask = Task { [weak self] in
            async let settingsResult = awaitRuntimeSettingsLoadRequest(settingsRequest)
            async let promptResult = awaitAskUserPromptRefreshRequest(promptRequest)
            let (resolvedSettings, resolvedPrompts) = await (settingsResult, promptResult)

            guard let self else { return }
            if let settingsRequest, let resolvedSettings {
                self.finishRuntimeSettingsLoad(
                    resolvedSettings,
                    token: settingsRequest.token,
                    presentErrors: true
                )
            }
            if let promptRequest, let resolvedPrompts {
                self.finishAskUserPromptRefresh(
                    resolvedPrompts,
                    token: promptRequest.token
                )
            }
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
        historyRetryState.cancelScheduled()
        latestRefreshDebounceTask?.cancel()
        latestRefreshDebounceTask = nil
        latestRefreshState.cancelScheduledAndPending()
        olderHistoryLoadTask?.cancel()
        olderHistoryLoadTask = nil
        runtimeSettingsLoadState.reset()
        runtimeSettingsMutationState.reset()
        runtimeSettingsMutationTask?.cancel()
        runtimeSettingsMutationTask = nil
        askUserPromptActivationTask?.cancel()
        askUserPromptActivationTask = nil
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

        let sessionID = sessionID
        latestRefreshTask = Task { [weak self] in
            var resolvedSnapshot: ConversationHistorySnapshot?
            var resolvedError: Error?
            do {
                resolvedSnapshot = try await historyUseCase.loadLatest(
                    sessionID: sessionID,
                    requestGeneration: generation
                )
            } catch {
                resolvedError = error
            }
            guard let self else { return }
            if let snapshot = resolvedSnapshot {
                if self.activationState.accepts(activation), self.isLocalAgentRuntimeReady {
                    self.applyHistorySnapshot(snapshot)
                    if self.historyError != nil {
                        self.historyError = nil
                    }
                    self.historyRetryState.reset()
                    self.historyRetryTask?.cancel()
                    self.historyRetryTask = nil
                }
            } else if let resolvedError {
                if self.activationState.accepts(activation), self.isLocalAgentRuntimeReady {
                    if self.historyError != resolvedError.localizedDescription {
                        self.historyError = resolvedError.localizedDescription
                    }
                    self.scheduleHistoryRetry()
                }
            }
            if presentation == .visible,
               self.activationState.accepts(activation) {
                self.isRefreshing = false
            }
            let pending = self.latestRefreshState.complete()
            self.latestRefreshTask = nil
            if let pending {
                self.enqueueLatestRefresh(presentation: pending, debounce: true)
            }
        }
    }

    private func scheduleHistoryRetry() {
        guard isActive,
              isLocalAgentRuntimeReady,
              historyRetryTask == nil,
              let activation = activationState.activeSession,
              let schedule = historyRetryState.schedule(
                activation: activation
              ) else { return }
        historyRetryTask = Task { [weak self] in
            do {
                try await Task.sleep(for: schedule.delay)
            } catch {
                return
            }
            guard !Task.isCancelled,
                  let self,
                  self.isLocalAgentRuntimeReady,
                  self.historyRetryState.accepts(
                    schedule.token,
                    activation: self.activationState.activeSession
                  ),
                  self.historyRetryState.finish(schedule.token) else { return }
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

        let sessionID = sessionID
        olderHistoryLoadTask = Task { [weak self] in
            let result: Result<ConversationHistorySnapshot, Error>
            do {
                result = .success(
                    try await historyUseCase.loadOlder(
                        sessionID: sessionID,
                        before: cursor,
                        requestGeneration: generation
                    )
                )
            } catch {
                result = .failure(error)
            }

            guard let self else { return }
            switch result {
            case let .success(snapshot):
                guard !Task.isCancelled,
                      self.activationState.accepts(activation),
                      self.isLocalAgentRuntimeReady,
                      self.inFlightOlderCursor == cursor else { return }
                self.applyHistorySnapshot(snapshot)
            case let .failure(error):
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
        let viewportUseCase = viewportUseCase
        let sessionID = sessionID
        viewportMutationTasks[kind] = Task { [weak self] in
            let snapshot = await viewportUseCase.markNewerContentRead(
                sessionID: sessionID
            )
            guard !Task.isCancelled,
                  let self,
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
    ) {
        focusAfterPromptRefreshTask?.cancel()
        let token = focusRefreshState.begin()
        guard promptID != nil,
              let promptRequest = beginAskUserPromptRefresh() else {
            focus(turnID: turnID, promptID: promptID, taskID: taskID, runID: runID)
            finishFocusRefresh(token)
            return
        }

        focusAfterPromptRefreshTask = Task { [weak self] in
            let result = await awaitAskUserPromptRefresh(promptRequest.task)
            guard !Task.isCancelled,
                  let self,
                  self.focusRefreshState.accepts(token) else { return }
            self.finishAskUserPromptRefresh(result, token: promptRequest.token)
            guard self.focusRefreshState.accepts(token) else { return }
            self.focus(
                turnID: turnID,
                promptID: promptID,
                taskID: taskID,
                runID: runID
            )
            self.finishFocusRefresh(token)
        }
    }

    private func finishFocusRefresh(_ token: ConversationFocusRefreshToken) {
        guard focusRefreshState.finish(token) else { return }
        focusAfterPromptRefreshTask = nil
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
        let viewportUseCase = viewportUseCase
        let sessionID = sessionID
        viewportMutationTasks[kind] = Task { [weak self] in
            let snapshot = await viewportUseCase.updateTimelineAnchor(
                sessionID: sessionID,
                latestTurnID: latestTurnID,
                isPinnedToBottom: isPinned
            )
            guard !Task.isCancelled,
                  let self,
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
        let sessionID = sessionID
        runtimeSettingsMutationTask = Task { [weak self] in
            let result: Result<ConversationRuntimeSettings, Error>
            do {
                result = .success(
                    try await runtimeSettingsService.updateModel(
                        sessionID: sessionID,
                        modelID: modelID
                    )
                )
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.runtimeSettingsMutationState.accepts(token) else { return }
            switch result {
            case let .success(settings):
                self.applyRuntimeSettings(settings)
            case let .failure(error):
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
        let sessionID = sessionID
        runtimeSettingsMutationTask = Task { [weak self] in
            let result: Result<ConversationRuntimeSettings, Error>
            do {
                result = .success(
                    try await runtimeSettingsService.updateReasoningLevel(
                        sessionID: sessionID,
                        level: level,
                        enabled: enabled
                    )
                )
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.runtimeSettingsMutationState.accepts(token) else { return }
            switch result {
            case let .success(settings):
                self.applyRuntimeSettings(settings)
            case let .failure(error):
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
        let sessionID = sessionID
        runtimeSettingsMutationTask = Task { [weak self] in
            let result: Result<ConversationRuntimeSettings, Error>
            do {
                result = .success(
                    try await runtimeSettingsService.updateRemoteConnection(
                        sessionID: sessionID,
                        connectionID: connectionID
                    )
                )
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.runtimeSettingsMutationState.accepts(token) else { return }
            switch result {
            case let .success(settings):
                self.applyRuntimeSettings(settings)
            case let .failure(error):
                self.selectedRemoteConnectionID = previous
                self.runtimeSettingsError = error.localizedDescription
            }
            self.finishRuntimeSettingsMutation(token)
        }
    }

    private func beginRuntimeSettingsMutation() -> ConversationRuntimeSettingsMutationToken {
        runtimeSettingsLoadState.reset()
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

    private func beginRuntimeSettingsLoad() -> ConversationRuntimeSettingsLoadRequest? {
        guard let runtimeSettingsService else { return nil }
        let token = runtimeSettingsLoadState.begin()
        let sessionID = sessionID
        let task = Task {
            async let settings = runtimeSettingsService.fetchSettings(sessionID: sessionID)
            async let models = runtimeSettingsService.fetchAvailableModels()
            return try await (settings, models)
        }
        return (token, task)
    }

    private func finishRuntimeSettingsLoad(
        _ result: Result<(ConversationRuntimeSettings, [ConversationModelOption]), Error>,
        token: ConversationRuntimeSettingsLoadToken,
        presentErrors: Bool
    ) {
        switch result {
        case let .success((resolvedSettings, resolvedModels)):
            guard runtimeSettingsLoadState.accepts(token) else { return }
            availableModels = resolvedModels
            applyRuntimeSettings(resolvedSettings)
            runtimeSettingsError = nil
        case let .failure(error):
            guard runtimeSettingsLoadState.accepts(token) else { return }
            if let runtimeError = error as? NativeLocalAgentConversationRuntimeSettingsError,
               case .notConfigured = runtimeError {
                runtimeSettingsError = nil
            } else if presentErrors {
                runtimeSettingsError = error.localizedDescription
            }
        }
        _ = runtimeSettingsLoadState.finish(token)
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
            do {
                for try await signal in stream {
                    guard !Task.isCancelled else { return }
                    if let self {
                        guard self.activationState.accepts(activation),
                              self.isLocalAgentRuntimeReady else { return }
                    } else {
                        return
                    }
                    switch ConversationRealtimePolicy.action(for: signal) {
                    case .refreshAskUserPrompts:
                        let request: AskUserPromptRefreshRequest?
                        if let self {
                            guard !Task.isCancelled,
                                  self.activationState.accepts(activation),
                                  self.isLocalAgentRuntimeReady else { return }
                            request = self.beginAskUserPromptRefresh()
                        } else {
                            return
                        }
                        let result = await awaitAskUserPromptRefreshRequest(request)
                        guard !Task.isCancelled,
                              let self,
                              self.activationState.accepts(activation),
                              self.isLocalAgentRuntimeReady else { return }
                        if let request, let result {
                            self.finishAskUserPromptRefresh(result, token: request.token)
                        }
                    case let .failure(message):
                        guard !Task.isCancelled,
                              let self,
                              self.activationState.accepts(activation),
                              self.isLocalAgentRuntimeReady else { return }
                        self.sendError = message
                        self.refreshLatestSilently()
                    case .refreshHistory:
                        guard !Task.isCancelled,
                              let self,
                              self.activationState.accepts(activation),
                              self.isLocalAgentRuntimeReady else { return }
                        self.refreshLatestSilently()
                    case .ignore:
                        break
                    }
                }
            } catch {
                guard !Task.isCancelled,
                      let self,
                      self.activationState.accepts(activation),
                      self.isLocalAgentRuntimeReady else { return }
                self.historyError = error.localizedDescription
            }
            guard !Task.isCancelled,
                  let self,
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
