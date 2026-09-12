import ChatOSCore
import Foundation

@MainActor
final class ConversationSessionViewModel: ObservableObject {
    let sessionID: String

    @Published private(set) var turns: [ConversationTurn]
    @Published private(set) var unreadNewerCount = 0
    @Published var isSending = false
    @Published private(set) var isUpdatingRuntimeSettings = false
    @Published private(set) var availableModels: [ConversationModelOption] = []
    @Published private(set) var selectedModelID: String?
    @Published private(set) var selectedRemoteConnectionID: String?
    @Published private(set) var selectedThinkingLevel: String?
    @Published private(set) var reasoningEnabled = false
    @Published var askUserStateError: String?
    @Published var runtimeSettingsError: String?
    @Published var sendError: String?
    @Published var selectedTurnID: String?
    @Published var draft = ""
    @Published var attachments: [ConversationAttachmentDraft] = []
    @Published var attachmentError: String?
    @Published var askUserPrompts: [AskUserPrompt] = []
    @Published var submittingAskUserPromptIDs: Set<String> = []
    @Published var askUserPromptErrors: [String: String] = [:]
    @Published var localAgentRunControls: [LocalAgentRunControlState] = []
    @Published var localAgentToolApprovals: [LocalAgentToolApprovalRequest] = []
    @Published private(set) var localAgentTasks: [LocalAgentTaskState] = []
    @Published var localAgentControlOperationIDs: Set<String> = []
    @Published var localAgentControlErrors: [String: String] = [:]
    @Published private(set) var focusRequest: ConversationFocusRequest?

    let historyStore: any ConversationHistoryStoring
    let commandService: (any ConversationCommandServicing)?
    let messageTaskGraphService: (any MessageTaskGraphServicing)?
    private let runtimeSettingsService: (any ConversationRuntimeSettingsServicing)?
    let askUserPromptService: (any AskUserPromptServicing)?
    let localAgentRunControlService: (any LocalAgentRunControlServicing)?
    private let localAgentTaskStateStore: (any LocalAgentTaskStateStoring)?
    private var localAgentUpdateTask: Task<Void, Never>?
    private var localAgentTaskUpdateTask: Task<Void, Never>?
    private var viewportUpdateGeneration: Int64 = 0
    var pendingLocalAgentRunStatuses: [String: LocalAgentRunStatus] = [:]

    init(
        sessionID: String,
        historyStore: any ConversationHistoryStoring,
        commandService: (any ConversationCommandServicing)? = nil,
        messageTaskGraphService: (any MessageTaskGraphServicing)? = nil,
        runtimeSettingsService: (any ConversationRuntimeSettingsServicing)? = nil,
        askUserPromptService: (any AskUserPromptServicing)? = nil,
        localAgentRunControlService: (any LocalAgentRunControlServicing)? = nil,
        localAgentTaskStateStore: (any LocalAgentTaskStateStoring)? = nil
    ) {
        self.sessionID = sessionID
        self.turns = []
        self.selectedTurnID = nil
        self.historyStore = historyStore
        self.commandService = commandService
        self.messageTaskGraphService = messageTaskGraphService
        self.runtimeSettingsService = runtimeSettingsService
        self.askUserPromptService = askUserPromptService
        self.localAgentRunControlService = localAgentRunControlService
        self.localAgentTaskStateStore = localAgentTaskStateStore

        Task { await bootstrap() }
    }

    deinit {
        localAgentUpdateTask?.cancel()
        localAgentTaskUpdateTask?.cancel()
    }

    func activate() {
        startLocalAgentUpdates()
        startLocalAgentTaskUpdates()
    }

    private func startLocalAgentUpdates() {
        guard localAgentUpdateTask == nil,
              let updates = historyStore as? any LocalAgentConversationUpdateStreaming
        else { return }
        let sessionID = sessionID
        localAgentUpdateTask = Task { [weak self] in
            let stream = await updates.localAgentUpdates(sessionID: sessionID)
            // Subscribe before reading the snapshot so an event arriving during
            // activation is either present in this read or queued on the stream.
            guard let self else { return }
            await self.refreshSnapshot()
            await self.refreshAskUserPrompts()
            await self.refreshLocalAgentControls()
            for await _ in stream {
                guard !Task.isCancelled else { return }
                await self.refreshSnapshot()
                await self.refreshAskUserPrompts()
                await self.refreshLocalAgentControls()
            }
        }
    }

    private func startLocalAgentTaskUpdates() {
        guard localAgentTaskUpdateTask == nil, let localAgentTaskStateStore else { return }
        let sessionID = sessionID
        localAgentTaskUpdateTask = Task { [weak self] in
            guard let self else { return }
            let stream = await localAgentTaskStateStore.localAgentTaskUpdates(
                sessionID: sessionID
            )
            // Keep the same subscribe-then-snapshot ordering as Main Chat.
            await refreshLocalAgentTasks()
            for await _ in stream {
                guard !Task.isCancelled else { return }
                await self.refreshLocalAgentTasks()
                await self.refreshAskUserPrompts()
                await self.refreshLocalAgentControls()
            }
        }
    }

    func tasks(for turnID: String) -> [LocalAgentTaskState] {
        localAgentTasks.filter { $0.task.sourceTurnID == turnID }
    }

    private func refreshLocalAgentTasks() async {
        guard let localAgentTaskStateStore else { return }
        localAgentTasks = await localAgentTaskStateStore.localAgentTasks(sessionID: sessionID)
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
    }

    func consumeFocusRequest(id: UUID) {
        guard focusRequest?.id == id else { return }
        focusRequest = nil
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
        !tasks(for: turn.id).isEmpty
    }

    private func bootstrap() async {
        async let runtimeSettings: Void = loadRuntimeSettings()
        async let prompts: Void = refreshAskUserPrompts()
        async let controls: Void = refreshLocalAgentControls()
        _ = await (runtimeSettings, prompts, controls)
        await refreshSnapshot()
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
        do {
            async let settings = runtimeSettingsService.fetchSettings(sessionID: sessionID)
            async let models = runtimeSettingsService.fetchAvailableModels()
            let (resolvedSettings, resolvedModels) = try await (settings, models)
            availableModels = resolvedModels
            applyRuntimeSettings(resolvedSettings)
        } catch {
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

    func refreshSnapshot() async {
        let snapshot = await historyStore.snapshot(sessionID: sessionID)
        if turns != snapshot.turns {
            turns = snapshot.turns
        }
        if unreadNewerCount != snapshot.unreadNewerCount {
            unreadNewerCount = snapshot.unreadNewerCount
        }
    }

}
