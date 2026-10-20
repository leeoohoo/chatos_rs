import ChatOSCore
import Foundation

enum TaskReplyInspectorPollingPolicy {
    static func interval(hasActiveRealtimeStream: Bool) -> Duration {
        hasActiveRealtimeStream ? .seconds(60) : .seconds(2)
    }
}

enum TaskReplyInspectorSection: String, CaseIterable {
    case process = "执行过程"
    case detail = "任务详情"

    func title(language: ChatOSLanguage) -> String {
        guard language == .english else { return rawValue }
        return switch self {
        case .process: "Execution Process"
        case .detail: "Task Details"
        }
    }
}

struct TaskReplySelection: Identifiable {
    var id: String { reply.id }
    let turn: ConversationTurn
    let reply: ConversationAssistantReply
    let initialSection: TaskReplyInspectorSection

    var refreshIdentity: String {
        let callback = reply.taskCallback
        return [
            reply.id,
            String(turn.revision),
            callback?.taskID,
            callback?.runID,
            callback?.event,
            callback?.status,
            reply.message.text,
        ]
        .compactMap { $0 }
        .joined(separator: "|")
    }

    var retryIdentity: String {
        let callback = reply.taskCallback
        return [
            turn.sessionID,
            reply.id,
            callback?.taskID,
            callback?.runID,
        ]
        .compactMap { $0 }
        .joined(separator: "|")
    }
}

@MainActor
final class TaskReplyInspectorViewModel: ObservableObject {
    struct LoadRequest: Sendable {
        let generation: Int
        let inspectorSession: MessageTaskInspectorSession
        let selectionIdentity: String
        let requestedSection: TaskReplyInspectorSection
        let messageID: String
        let taskID: String
        let lookup: MessageTaskLookup
        let fallbackRunID: String?
        let prefetchedTask: MessageTask?
        let graphUseCase: MessageTaskGraphUseCase
        let inspectorRefreshUseCase: MessageTaskInspectorRefreshUseCase
    }

    enum TaskLoadOutcome {
        case success(MessageTask)
        case failure(String)
    }

    enum RunLoadOutcome {
        case success(MessageTaskInspectorSectionUpdate)
        case failure(String)
    }

    private(set) var selection: TaskReplySelection
    @Published var section: TaskReplyInspectorSection
    @Published private(set) var task: MessageTask?
    @Published private(set) var processTimelineItems: [TaskProcessTimelineItem] = []
    @Published private(set) var isLoading = false
    @Published private(set) var isLoadingModelOutput = false
    @Published private(set) var isRetrying = false
    @Published var retryInstruction = ""
    @Published var errorMessage: String?
    @Published private(set) var modelOutputError: String?
    @Published private(set) var retryErrorMessage: String?

    private let graphUseCase: MessageTaskGraphUseCase
    private let inspectorRefreshUseCase: MessageTaskInspectorRefreshUseCase
    private let realtimeUseCase: ConversationRealtimeUseCase?
    private let pollingInterval: @Sendable (Bool) -> Duration
    private var loadTask: Task<Void, Never>?
    private var retryTask: Task<Void, Never>?
    private var pollingTask: Task<Void, Never>?
    private var realtimeTask: Task<Void, Never>?
    private var realtimeSessionID: String?
    private var hasActiveRealtimeStream = false
    private var loadGeneration = 0
    private var loadedModelOutputRunID: String?
    var retryState = MessageTaskRetryState()
    var lifecycleState = MessageTaskInspectorLifecycleState()

    init(
        selection: TaskReplySelection,
        service: any MessageTaskGraphServicing,
        realtimeService: (any ConversationRealtimeStreaming)? = nil,
        pollingInterval: @escaping @Sendable (Bool) -> Duration = {
            TaskReplyInspectorPollingPolicy.interval(hasActiveRealtimeStream: $0)
        }
    ) {
        self.selection = selection
        self.section = selection.initialSection
        self.graphUseCase = MessageTaskGraphUseCase(graphService: service)
        self.inspectorRefreshUseCase = MessageTaskInspectorRefreshUseCase(
            graphService: service
        )
        self.realtimeUseCase = realtimeService.map {
            ConversationRealtimeUseCase(service: $0)
        }
        self.pollingInterval = pollingInterval
    }

    deinit {
        loadTask?.cancel()
        retryTask?.cancel()
        pollingTask?.cancel()
        realtimeTask?.cancel()
    }

    func load() {
        let inspectorSession = lifecycleState.start()
        startRealtimeIfNeeded(inspectorSession: inspectorSession)
        refresh(
            showsLoadingState: task == nil,
            inspectorSession: inspectorSession
        )
    }

    func update(selection: TaskReplySelection) {
        let needsRefresh = self.selection.refreshIdentity != selection.refreshIdentity
        let sessionChanged = self.selection.turn.sessionID != selection.turn.sessionID
        let retryTargetChanged = self.selection.retryIdentity != selection.retryIdentity
        self.selection = selection
        if retryTargetChanged {
            cancelRetry()
            retryInstruction = ""
            retryErrorMessage = nil
        }
        if sessionChanged {
            stopRealtime()
            if let inspectorSession = lifecycleState.activeSession {
                startRealtimeIfNeeded(inspectorSession: inspectorSession)
            }
        }
        guard needsRefresh else { return }
        loadedModelOutputRunID = nil
        refresh()
    }

    func selectSection(_ section: TaskReplyInspectorSection) {
        let changed = self.section != section
        self.section = section
        guard changed else { return }
        switch section {
        case .process where task != nil:
            // The process timeline is fully contained in the task response. Reuse it instead of
            // issuing the task + run request pair again when the user switches tabs.
            loadGeneration += 1
            loadTask?.cancel()
            isLoading = false
            isLoadingModelOutput = false
            if let inspectorSession = lifecycleState.activeSession {
                scheduleActiveRefreshIfNeeded(inspectorSession: inspectorSession)
            }
        case .detail:
            let runID = task?.lastRunID ?? selection.reply.taskCallback?.runID
            if runID != loadedModelOutputRunID {
                if let task,
                   task.id == selection.reply.taskCallback?.taskID,
                   let inspectorSession = lifecycleState.activeSession {
                    refresh(
                        showsLoadingState: true,
                        inspectorSession: inspectorSession,
                        prefetchedTask: task
                    )
                } else {
                    refresh()
                }
            }
        default:
            refresh()
        }
    }

    func refresh() {
        guard let inspectorSession = lifecycleState.activeSession else { return }
        refresh(showsLoadingState: true, inspectorSession: inspectorSession)
    }

    private func refresh(
        showsLoadingState: Bool,
        inspectorSession: MessageTaskInspectorSession,
        prefetchedTask: MessageTask? = nil
    ) {
        guard lifecycleState.accepts(inspectorSession),
              let callback = selection.reply.taskCallback else { return }
        pollingTask?.cancel()
        pollingTask = nil
        loadGeneration += 1
        let generation = loadGeneration
        let selection = selection
        let requestedSection = section
        let request = LoadRequest(
            generation: generation,
            inspectorSession: inspectorSession,
            selectionIdentity: selection.refreshIdentity,
            requestedSection: requestedSection,
            messageID: selection.reply.message.id,
            taskID: callback.taskID,
            lookup: lookup(callback, selection: selection),
            fallbackRunID: callback.runID,
            prefetchedTask: prefetchedTask,
            graphUseCase: graphUseCase,
            inspectorRefreshUseCase: inspectorRefreshUseCase
        )
        loadTask?.cancel()
        if showsLoadingState {
            isLoading = true
            errorMessage = nil
            modelOutputError = nil
            isLoadingModelOutput = requestedSection == .detail
        }
        loadTask = Task { [weak self] in
            let taskOutcome = await Self.fetchTask(for: request)
            guard !Task.isCancelled,
                  self?.applyTaskLoadOutcome(taskOutcome, request: request) == true else { return }
            guard case let .success(loadedTask) = taskOutcome,
                  request.requestedSection == .detail else {
                self?.finishLoad(request: request)
                return
            }

            let runOutcome = await Self.fetchLatestRun(
                for: loadedTask,
                request: request
            )
            guard !Task.isCancelled else { return }
            self?.applyRunLoadOutcome(
                runOutcome,
                loadedTask: loadedTask,
                request: request
            )
            self?.finishLoad(request: request)
        }
    }

    nonisolated static func fetchTask(for request: LoadRequest) async -> TaskLoadOutcome {
        if let prefetchedTask = request.prefetchedTask {
            return .success(prefetchedTask)
        }
        do {
            return .success(try await request.graphUseCase.loadTask(
                messageID: request.messageID,
                taskID: request.taskID,
                lookup: request.lookup
            ))
        } catch {
            return .failure(error.localizedDescription)
        }
    }

    nonisolated static func fetchLatestRun(
        for loadedTask: MessageTask,
        request: LoadRequest
    ) async -> RunLoadOutcome {
        do {
            return .success(try await request.inspectorRefreshUseCase.refreshSection(
                .detail,
                messageID: request.messageID,
                lookup: request.lookup,
                task: loadedTask,
                fallbackRunID: request.fallbackRunID
            ))
        } catch {
            return .failure(error.localizedDescription)
        }
    }

    private func accepts(_ request: LoadRequest) -> Bool {
        lifecycleState.accepts(request.inspectorSession)
            && request.generation == loadGeneration
            && request.selectionIdentity == selection.refreshIdentity
            && request.requestedSection == section
    }

    @discardableResult
    private func applyTaskLoadOutcome(
        _ outcome: TaskLoadOutcome,
        request: LoadRequest
    ) -> Bool {
        guard accepts(request) else { return false }
        switch outcome {
        case let .success(loadedTask):
            apply(loadedTask)
        case let .failure(message):
            if errorMessage != message {
                errorMessage = message
            }
        }
        return true
    }

    private func applyRunLoadOutcome(
        _ outcome: RunLoadOutcome,
        loadedTask: MessageTask,
        request: LoadRequest
    ) {
        guard accepts(request), task?.id == loadedTask.id else { return }
        switch outcome {
        case let .success(update):
            guard case let .modelOutput(updatedTask, loadedRunID) = update else { return }
            apply(updatedTask)
            loadedModelOutputRunID = loadedRunID
        case let .failure(message):
            modelOutputError = message
        }
    }

    private func finishLoad(request: LoadRequest) {
        guard accepts(request) else { return }
        // A realtime or polling refresh can supersede the visible refresh that set these
        // flags. The latest generation owns cleanup even when that refresh is silent;
        // cancelled, older generations are excluded by the identity checks above.
        isLoading = false
        isLoadingModelOutput = false
        scheduleActiveRefreshIfNeeded(inspectorSession: request.inspectorSession)
    }

    func retry() {
        guard let callback = selection.reply.taskCallback,
              let runID = task?.lastRunID ?? callback.runID,
              !isRetrying,
              let inspectorSession = lifecycleState.activeSession else { return }
        let selection = selection
        let instruction = retryInstruction
        let lookup = lookup(callback, selection: selection)
        let token = retryState.begin(contextID: selection.retryIdentity)
        let graphUseCase = graphUseCase
        isRetrying = true
        errorMessage = nil
        retryErrorMessage = nil
        retryTask = Task { [weak self] in
            do {
                try await graphUseCase.retryRun(
                    messageID: selection.reply.message.id,
                    runID: runID,
                    lookup: lookup,
                    instruction: instruction
                )
                guard !Task.isCancelled,
                      let self,
                      self.lifecycleState.accepts(inspectorSession),
                      self.retryState.accepts(token) else { return }
                self.retryInstruction = ""
                self.task = nil
                self.processTimelineItems = []
                self.loadedModelOutputRunID = nil
                self.modelOutputError = nil
                self.refresh()
            } catch {
                guard !Task.isCancelled,
                      let self,
                      self.lifecycleState.accepts(inspectorSession),
                      self.retryState.accepts(token) else { return }
                self.retryErrorMessage = error.localizedDescription
            }
            guard let self,
                  self.lifecycleState.accepts(inspectorSession),
                  self.retryState.finish(token) else { return }
            self.retryTask = nil
            self.isRetrying = false
        }
    }

    func stop() {
        lifecycleState.stop()
        loadGeneration += 1
        loadTask?.cancel()
        loadTask = nil
        pollingTask?.cancel()
        pollingTask = nil
        stopRealtime()
        cancelRetry()
        isLoading = false
        isLoadingModelOutput = false
    }

    private func cancelRetry() {
        retryState.reset()
        retryTask?.cancel()
        retryTask = nil
        isRetrying = false
    }

    func apply(_ loadedTask: MessageTask) {
        guard task != loadedTask else { return }
        let nextTimelineItems = TaskProcessTimelineBuilder.build(
            processLog: loadedTask.processLog,
            taskStatus: loadedTask.status
        )
        task = loadedTask
        if processTimelineItems != nextTimelineItems {
            processTimelineItems = nextTimelineItems
        }
    }

    private func scheduleActiveRefreshIfNeeded(
        inspectorSession: MessageTaskInspectorSession
    ) {
        pollingTask?.cancel()
        pollingTask = nil
        guard lifecycleState.accepts(inspectorSession),
              task?.isActive == true else { return }
        let interval = pollingInterval(hasActiveRealtimeStream)
        pollingTask = Task { [weak self] in
            do {
                try await Task.sleep(for: interval)
            } catch {
                return
            }
            guard let self,
                  !Task.isCancelled,
                  self.lifecycleState.accepts(inspectorSession) else { return }
            self.refresh(
                showsLoadingState: false,
                inspectorSession: inspectorSession
            )
        }
    }

    private func startRealtimeIfNeeded(
        inspectorSession: MessageTaskInspectorSession
    ) {
        guard lifecycleState.accepts(inspectorSession),
              let realtimeUseCase else { return }
        let sessionID = selection.turn.sessionID
        guard realtimeTask == nil, realtimeSessionID != sessionID else { return }
        realtimeSessionID = sessionID
        realtimeTask = Task { [weak self, realtimeUseCase, sessionID] in
            let stream = await realtimeUseCase.events(sessionID: sessionID)
            guard !Task.isCancelled else { return }
            self?.realtimeDidStart(
                sessionID: sessionID,
                inspectorSession: inspectorSession
            )
            do {
                for try await signal in stream {
                    guard !Task.isCancelled,
                          signal.sessionID == sessionID else { return }
                    guard self?.acceptsRealtime(
                        sessionID: sessionID,
                        inspectorSession: inspectorSession
                    ) == true else { return }
                    self?.refresh(
                        showsLoadingState: false,
                        inspectorSession: inspectorSession
                    )
                }
            } catch {
                guard !Task.isCancelled else { return }
            }
            guard !Task.isCancelled else { return }
            self?.realtimeDidEnd(
                sessionID: sessionID,
                inspectorSession: inspectorSession
            )
        }
    }

    private func acceptsRealtime(
        sessionID: String,
        inspectorSession: MessageTaskInspectorSession
    ) -> Bool {
        lifecycleState.accepts(inspectorSession)
            && realtimeSessionID == sessionID
            && selection.turn.sessionID == sessionID
    }

    private func realtimeDidStart(
        sessionID: String,
        inspectorSession: MessageTaskInspectorSession
    ) {
        guard lifecycleState.accepts(inspectorSession),
              realtimeSessionID == sessionID else { return }
        hasActiveRealtimeStream = true
        scheduleActiveRefreshIfNeeded(inspectorSession: inspectorSession)
    }

    private func realtimeDidEnd(
        sessionID: String,
        inspectorSession: MessageTaskInspectorSession
    ) {
        guard lifecycleState.accepts(inspectorSession),
              realtimeSessionID == sessionID else { return }
        hasActiveRealtimeStream = false
        realtimeTask = nil
        realtimeSessionID = nil
        scheduleActiveRefreshIfNeeded(inspectorSession: inspectorSession)
    }

    private func stopRealtime() {
        realtimeTask?.cancel()
        realtimeTask = nil
        realtimeSessionID = nil
        hasActiveRealtimeStream = false
    }

    private func lookup(
        _ callback: TaskExecutionCallbackReference,
        selection: TaskReplySelection
    ) -> MessageTaskLookup {
        MessageTaskLookup(
            sessionID: callback.sourceSessionID ?? selection.turn.sessionID,
            turnID: callback.sourceTurnID ?? selection.turn.id,
            sourceUserMessageID: callback.sourceUserMessageID ?? selection.turn.userMessage.id
        )
    }
}
