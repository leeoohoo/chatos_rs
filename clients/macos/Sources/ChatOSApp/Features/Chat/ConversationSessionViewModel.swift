import ChatOSCore
import Foundation

@MainActor
final class ConversationComposerState: ObservableObject {
    @Published var draft = ""
    @Published var attachments: [ConversationAttachmentDraft] = []
    @Published var attachmentError: String?
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
    private static let historyPageSize = 10
    private static let realtimeRefreshDebounce: Duration = .milliseconds(250)

    private enum LatestRefreshPresentation: Equatable {
        case silent
        case visible

        func merged(with other: Self) -> Self {
            self == .visible || other == .visible ? .visible : .silent
        }
    }

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

    let historyStore: any ConversationHistoryStoring
    let commandService: (any ConversationCommandServicing)?
    let turnProcessService: (any TurnProcessServicing)?
    let messageTaskGraphService: (any MessageTaskGraphServicing)?
    private let remoteService: (any ConversationRemoteServicing)?
    let realtimeService: (any ConversationRealtimeStreaming)?
    private let runtimeSettingsService: (any ConversationRuntimeSettingsServicing)?
    let askUserPromptService: (any AskUserPromptServicing)?
    private var olderCursor: String?
    private var requestGeneration: Int64 = 0
    private var inFlightOlderCursor: String?
    private var realtimeTask: Task<Void, Never>?
    private var historyRetryTask: Task<Void, Never>?
    private var latestRefreshDebounceTask: Task<Void, Never>?
    private var latestRefreshDebouncePresentation: LatestRefreshPresentation?
    private var historyRetryAttempt = 0
    private var latestRefreshInFlight = false
    private var latestRefreshPending: LatestRefreshPresentation?
    private var viewportUpdateGeneration: Int64 = 0
    private var taskGraphAvailabilityTasks: [String: Task<Void, Never>] = [:]
    private var taskGraphAvailabilityRevisions: [String: Int64] = [:]
    var attachmentFileLoadTasks: [UUID: Task<Void, Never>] = [:]
    var pastedImageNormalizationTasks: [UUID: Task<Void, Never>] = [:]
    private var runtimeSettingsLoadGeneration: UInt64 = 0
    var askUserPromptRefreshGeneration: UInt64 = 0
    private var isActive = false

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
        self.remoteService = remoteService
        self.realtimeService = realtimeService
        self.commandService = commandService
        self.turnProcessService = turnProcessService
        self.messageTaskGraphService = messageTaskGraphService
        self.runtimeSettingsService = runtimeSettingsService
        self.askUserPromptService = askUserPromptService

        Task { await bootstrap(initialTurns: initialTurns) }
    }

    deinit {
        realtimeTask?.cancel()
        historyRetryTask?.cancel()
        latestRefreshDebounceTask?.cancel()
        taskGraphAvailabilityTasks.values.forEach { $0.cancel() }
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
        refreshLatestSilently()
        startRealtime()
    }

    func deactivate() {
        guard isActive else { return }
        isActive = false
        realtimeTask?.cancel()
        realtimeTask = nil
        historyRetryTask?.cancel()
        historyRetryTask = nil
        latestRefreshDebounceTask?.cancel()
        latestRefreshDebounceTask = nil
        latestRefreshDebouncePresentation = nil
        latestRefreshPending = nil
        taskGraphAvailabilityTasks.values.forEach { $0.cancel() }
        taskGraphAvailabilityTasks.removeAll()
    }

    func localAgentRuntimeDidBecomeReady() {
        realtimeTask?.cancel()
        realtimeTask = nil
        historyRetryTask?.cancel()
        historyRetryTask = nil
        historyRetryAttempt = 0
        runtimeSettingsError = nil
        Task { [weak self] in
            guard let self else { return }
            await loadRuntimeSettings()
            guard runtimeSettingsError == nil, isActive else { return }
            refreshLatest()
            startRealtime()
        }
    }

    func refreshLatestSilently() {
        enqueueLatestRefresh(presentation: .silent, debounce: true)
    }

    private func enqueueLatestRefresh(
        presentation: LatestRefreshPresentation,
        debounce: Bool
    ) {
        guard isActive, let remoteService else { return }

        if debounce {
            let scheduledPresentation = latestRefreshDebouncePresentation?
                .merged(with: presentation) ?? presentation
            latestRefreshDebouncePresentation = scheduledPresentation
            latestRefreshDebounceTask?.cancel()
            latestRefreshDebounceTask = Task { [weak self] in
                do {
                    try await Task.sleep(for: Self.realtimeRefreshDebounce)
                } catch {
                    return
                }
                guard !Task.isCancelled, let self else { return }
                self.latestRefreshDebounceTask = nil
                self.latestRefreshDebouncePresentation = nil
                self.performLatestRefresh(
                    using: remoteService,
                    presentation: scheduledPresentation
                )
            }
            return
        }

        latestRefreshDebounceTask?.cancel()
        latestRefreshDebounceTask = nil
        latestRefreshDebouncePresentation = nil
        performLatestRefresh(using: remoteService, presentation: presentation)
    }

    private func performLatestRefresh(
        using remoteService: any ConversationRemoteServicing,
        presentation: LatestRefreshPresentation
    ) {
        guard !latestRefreshInFlight else {
            latestRefreshPending = latestRefreshPending?
                .merged(with: presentation) ?? presentation
            return
        }

        latestRefreshInFlight = true
        latestRefreshPending = nil
        requestGeneration += 1
        let generation = requestGeneration
        if presentation == .visible {
            isRefreshing = true
            historyError = nil
        }

        Task {
            do {
                let page = try await remoteService.fetchHistory(
                    ConversationHistoryQuery(
                        sessionID: sessionID,
                        limit: Self.historyPageSize,
                        requestGeneration: generation
                    )
                )
                await historyStore.mergePage(page, sessionID: sessionID, origin: .latest)
                await refreshSnapshot()
                if historyError != nil {
                    historyError = nil
                }
                historyRetryAttempt = 0
                historyRetryTask?.cancel()
                historyRetryTask = nil
            } catch {
                if historyError != error.localizedDescription {
                    historyError = error.localizedDescription
                }
                scheduleHistoryRetry()
            }
            if presentation == .visible {
                isRefreshing = false
            }
            latestRefreshInFlight = false
            if let pending = latestRefreshPending {
                latestRefreshPending = nil
                enqueueLatestRefresh(presentation: pending, debounce: true)
            }
        }
    }

    private func scheduleHistoryRetry() {
        let delays: [Duration] = [
            .seconds(2),
            .seconds(5),
            .seconds(10),
            .seconds(20),
        ]
        guard isActive,
              historyRetryTask == nil,
              historyRetryAttempt < delays.count else { return }
        let delay = delays[historyRetryAttempt]
        historyRetryAttempt += 1
        historyRetryTask = Task { [weak self] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled, let self else { return }
            self.historyRetryTask = nil
            self.enqueueLatestRefresh(presentation: .silent, debounce: false)
        }
    }

    func loadOlder() {
        guard let remoteService,
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

        Task {
            do {
                let page = try await remoteService.fetchHistory(
                    ConversationHistoryQuery(
                        sessionID: sessionID,
                        limit: Self.historyPageSize,
                        before: cursor,
                        requestGeneration: generation
                    )
                )
                await historyStore.mergePage(page, sessionID: sessionID, origin: .older)
                await refreshSnapshot()
            } catch {
                historyError = error.localizedDescription
            }
            if inFlightOlderCursor == cursor {
                inFlightOlderCursor = nil
            }
            isLoadingOlder = false
            timelineObservationState.invalidate()
        }
    }

    func markNewerContentRead() {
        Task {
            await historyStore.markNewerContentRead(sessionID: sessionID)
            await refreshSnapshot()
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

    func consumeFocusRequest(id: UUID) {
        guard focusRequest?.id == id else { return }
        focusRequest = nil
        timelineObservationState.invalidate()
    }

    func setTimelinePinnedToBottom(_ isPinned: Bool) {
        viewportUpdateGeneration += 1
        let generation = viewportUpdateGeneration
        let turnID = turns.last?.id ?? sessionID
        Task {
            await historyStore.setViewportAnchor(
                ViewportAnchor(
                    turnID: turnID,
                    relativeOffset: 0,
                    isPinnedToBottom: isPinned
                ),
                sessionID: sessionID
            )
            guard generation == viewportUpdateGeneration else { return }
            await refreshSnapshot()
        }
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
            taskGraphAvailabilityTasks[turn.id]?.cancel()
            taskGraphAvailabilityTasks[turn.id] = nil
            if hadVisibleGraph {
                timelineObservationState.invalidate()
            }
            return
        }
        guard taskGraphAvailabilityRevisions[turn.id] != turn.revision else { return }

        taskGraphAvailabilityRevisions[turn.id] = turn.revision
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
                      self?.taskGraphAvailabilityRevisions[turn.id] == turn.revision else {
                    return
                }
                if graph.nodes.isEmpty {
                    if self?.taskGraphAvailability.removeValue(forKey: turn.id) != nil {
                        self?.timelineObservationState.invalidate()
                    }
                } else {
                    if self?.taskGraphAvailability[turn.id] != true {
                        self?.taskGraphAvailability[turn.id] = true
                        self?.timelineObservationState.invalidate()
                    }
                }
            } catch {
                guard !Task.isCancelled,
                      self?.taskGraphAvailabilityRevisions[turn.id] == turn.revision else {
                    return
                }
                if self?.taskGraphAvailability.removeValue(forKey: turn.id) != nil {
                    self?.timelineObservationState.invalidate()
                }
            }
            self?.taskGraphAvailabilityTasks[turn.id] = nil
        }
    }

    private func bootstrap(initialTurns: [ConversationTurn]) async {
        async let runtimeSettings: Void = loadRuntimeSettings()
        async let prompts: Void = refreshAskUserPrompts()
        _ = await (runtimeSettings, prompts)
        await historyStore.mergeCachedTurns(initialTurns, sessionID: sessionID)
        await refreshSnapshot()
        if isActive {
            refreshLatest()
            startRealtime()
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
        selectedModelID = modelID
        runtimeSettingsError = nil
        isUpdatingRuntimeSettings = true
        Task {
            do {
                let settings = try await runtimeSettingsService.updateModel(
                    sessionID: sessionID,
                    modelID: modelID
                )
                applyRuntimeSettings(settings)
            } catch {
                selectedModelID = previous
                runtimeSettingsError = error.localizedDescription
            }
            isUpdatingRuntimeSettings = false
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
        selectedThinkingLevel = level
        reasoningEnabled = enabled
        runtimeSettingsError = nil
        isUpdatingRuntimeSettings = true
        Task {
            do {
                let settings = try await runtimeSettingsService.updateReasoningLevel(
                    sessionID: sessionID,
                    level: level,
                    enabled: enabled
                )
                applyRuntimeSettings(settings)
            } catch {
                selectedThinkingLevel = previousLevel
                reasoningEnabled = previous
                runtimeSettingsError = error.localizedDescription
            }
            isUpdatingRuntimeSettings = false
        }
    }

    func setRemoteConnectionID(_ connectionID: String?) {
        guard let runtimeSettingsService,
              selectedRemoteConnectionID != connectionID else { return }
        let previous = selectedRemoteConnectionID
        selectedRemoteConnectionID = connectionID
        runtimeSettingsError = nil
        isUpdatingRuntimeSettings = true
        Task {
            do {
                let settings = try await runtimeSettingsService.updateRemoteConnection(
                    sessionID: sessionID,
                    connectionID: connectionID
                )
                applyRuntimeSettings(settings)
            } catch {
                selectedRemoteConnectionID = previous
                runtimeSettingsError = error.localizedDescription
            }
            isUpdatingRuntimeSettings = false
        }
    }

    private func loadRuntimeSettings() async {
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
        } catch {
            guard generation == runtimeSettingsLoadGeneration else { return }
            runtimeSettingsError = error.localizedDescription
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

    private func startRealtime() {
        guard let realtimeService, realtimeTask == nil else { return }
        let sessionID = sessionID

        realtimeTask = Task { [weak self] in
            let stream = await realtimeService.events(sessionID: sessionID)
            do {
                for try await signal in stream {
                    guard let self else { return }
                    if signal.askUserPromptUpdate != nil {
                        await self.refreshAskUserPrompts()
                        continue
                    }
                    switch signal.kind {
                    case .failed:
                        self.sendError = signal.processUpdate?.detail?.trimmingCharacters(
                            in: .whitespacesAndNewlines
                        ).nonEmptyValue ?? "AI 处理失败，请检查模型配置后重试。"
                        self.refreshLatestSilently()
                    case .reconcile, .persisted, .completed, .cancelled:
                        self.refreshLatestSilently()
                    case .started, .updated, .unknown:
                        break
                    }
                }
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.historyError = error.localizedDescription
            }
            self?.realtimeTask = nil
        }
    }

    func refreshSnapshot() async {
        let snapshot = await historyStore.snapshot(sessionID: sessionID)
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

private extension String {
    var nonEmptyValue: String? {
        isEmpty ? nil : self
    }
}
