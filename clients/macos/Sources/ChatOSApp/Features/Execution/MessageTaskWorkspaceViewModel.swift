import ChatOSCore
import Foundation

@MainActor
final class MessageTaskWorkspaceViewModel: ObservableObject {
    enum InspectorSection: String, CaseIterable {
        case detail = "任务详情"
        case process = "执行过程"
        case run = "运行详情"

        func title(language: ChatOSLanguage) -> String {
            guard language == .english else { return rawValue }
            return switch self {
            case .detail: "Task Details"
            case .process: "Execution Process"
            case .run: "Run Details"
            }
        }
    }

    let turn: ConversationTurn
    @Published private(set) var executionActivity: [ConversationRealtimeProcessUpdate] = []
    @Published private(set) var graph: MessageTaskGraphSnapshot?
    @Published private(set) var selectedTask: MessageTask?
    @Published var taskDetail: MessageTask?
    @Published var runDetail: MessageTaskRunDetail?
    @Published private(set) var isLoading = false
    @Published private(set) var isAwaitingInitialGraph = false
    @Published var isLoadingInspector = false
    @Published var isLoadingModelOutput = false
    @Published var isLoadingRun = false
    @Published var isLoadingMoreRunEvents = false
    @Published private(set) var isRetrying = false
    @Published var displayMode: MessageTaskGraphDisplayMode = .reduced
    @Published var inspectorSection: InspectorSection = .detail
    @Published var retryInstruction = ""
    @Published var errorMessage: String?

    let graphUseCase: MessageTaskGraphUseCase
    let runUseCase: MessageTaskRunUseCase
    let inspectorRefreshUseCase: MessageTaskInspectorRefreshUseCase
    let realtimeService: (any ConversationRealtimeStreaming)?
    let initialTaskID: String?
    let initialRunID: String?
    var initialLoadTask: Task<Void, Never>?
    var refreshTask: Task<Void, Never>?
    var retryTask: Task<Void, Never>?
    var pollingTask: Task<Void, Never>?
    var realtimeTask: Task<Void, Never>?
    var inspectorLoadTask: Task<Void, Never>?
    var modelOutputLoadTask: Task<Void, Never>?
    var runLoadTask: Task<Void, Never>?
    var moreRunEventsTask: Task<Void, Never>?
    var hasActiveRealtimeStream = false
    var loadedModelOutputRunID: String?
    var workspaceRefreshGeneration = 0
    var workspaceLifecycleState = MessageTaskWorkspaceLifecycleState()
    var graphPollingState = MessageTaskGraphPollingState()
    var inspectorRequestState = MessageTaskInspectorRequestState()
    var retryState = MessageTaskRetryState()

    init(
        turn: ConversationTurn,
        graphService: any MessageTaskGraphServicing,
        realtimeService: (any ConversationRealtimeStreaming)? = nil,
        initialTaskID: String? = nil,
        initialRunID: String? = nil
    ) {
        self.turn = turn
        self.graphUseCase = MessageTaskGraphUseCase(graphService: graphService)
        self.runUseCase = MessageTaskRunUseCase(graphService: graphService)
        self.inspectorRefreshUseCase = MessageTaskInspectorRefreshUseCase(
            graphService: graphService
        )
        self.realtimeService = realtimeService
        self.initialTaskID = initialTaskID
        self.initialRunID = initialRunID
        if initialRunID != nil {
            inspectorSection = .run
        }
    }

    deinit {
        initialLoadTask?.cancel()
        refreshTask?.cancel()
        retryTask?.cancel()
        pollingTask?.cancel()
        realtimeTask?.cancel()
        inspectorLoadTask?.cancel()
        modelOutputLoadTask?.cancel()
        runLoadTask?.cancel()
        moreRunEventsTask?.cancel()
    }

    var displayGraph: MessageTaskGraphSnapshot? {
        graph.map { MessageTaskGraphNormalizer.normalize($0, mode: displayMode) }
    }

    var selectedTaskID: String? { selectedTask?.id }

    func load() {
        guard !isLoading, workspaceLifecycleState.activeSession == nil else { return }
        let session = workspaceLifecycleState.start()
        isLoading = true
        errorMessage = nil
        initialLoadTask = Task { [weak self] in
            guard let self else { return }
            await self.refreshWorkspaceState(
                refreshInspector: false,
                workspaceSession: session
            )
            guard !Task.isCancelled,
                  self.workspaceLifecycleState.accepts(session) else { return }
            self.startRealtime(workspaceSession: session)
            self.startPollingIfNeeded(workspaceSession: session)
            self.initialLoadTask = nil
            self.isLoading = false
        }
    }

    func refresh() {
        guard let session = workspaceLifecycleState.activeSession else { return }
        refreshTask?.cancel()
        errorMessage = nil
        resetEmptyGraphRetryBudgetIfNeeded()
        refreshTask = Task { [weak self] in
            guard let self else { return }
            await self.refreshWorkspaceState(
                refreshInspector: true,
                workspaceSession: session
            )
            guard !Task.isCancelled,
                  self.workspaceLifecycleState.accepts(session) else { return }
            self.startPollingIfNeeded(workspaceSession: session)
            self.refreshTask = nil
        }
    }

    func select(_ task: MessageTask, section: InspectorSection? = nil) {
        if selectedTask?.id != task.id {
            cancelRetry()
            retryInstruction = ""
        }
        cancelInspectorRequests()
        selectedTask = task
        if let section { inspectorSection = section }
        loadInspector(for: task)
    }

    func cancelInspectorRequests() {
        inspectorRequestState.reset()
        inspectorLoadTask?.cancel()
        inspectorLoadTask = nil
        modelOutputLoadTask?.cancel()
        modelOutputLoadTask = nil
        runLoadTask?.cancel()
        runLoadTask = nil
        moreRunEventsTask?.cancel()
        moreRunEventsTask = nil
        isLoadingInspector = false
        isLoadingModelOutput = false
        isLoadingRun = false
        isLoadingMoreRunEvents = false
    }

    func ensureInspectorSectionLoaded() {
        guard let task = taskDetail ?? selectedTask else { return }
        switch inspectorSection {
        case .detail:
            loadModelOutput(for: task)
        case .process:
            cancelInspectorRequests()
        case .run:
            guard runDetail == nil, !isLoadingRun else { return }
            loadRun(for: task)
        }
    }

    func retrySelectedRun() {
        guard let task = selectedTask,
              let runID = task.lastRunID ?? runDetail?.run.id,
              !isRetrying,
              let session = workspaceLifecycleState.activeSession else { return }
        let token = retryState.begin(
            contextID: "\(turn.sessionID):\(task.id):\(runID)"
        )
        let instruction = retryInstruction
        let graphUseCase = graphUseCase
        isRetrying = true
        errorMessage = nil
        let target = target(for: task)
        retryTask = Task { [weak self] in
            do {
                try await graphUseCase.retryRun(
                    messageID: target.messageID,
                    runID: runID,
                    lookup: target.lookup,
                    instruction: instruction
                )
                guard !Task.isCancelled,
                      let self,
                      self.workspaceLifecycleState.accepts(session) else { return }
                guard self.retryState.accepts(token) else { return }
                self.retryInstruction = ""
                self.inspectorSection = .run
                self.refresh()
                self.loadInspector(for: task)
            } catch {
                guard !Task.isCancelled,
                      let self,
                      self.workspaceLifecycleState.accepts(session) else { return }
                guard self.retryState.accepts(token) else { return }
                self.errorMessage = error.localizedDescription
            }
            guard let self,
                  self.workspaceLifecycleState.accepts(session),
                  self.retryState.finish(token) else { return }
            self.retryTask = nil
            self.isRetrying = false
        }
    }

    func stop() {
        workspaceLifecycleState.stop()
        workspaceRefreshGeneration &+= 1
        initialLoadTask?.cancel()
        initialLoadTask = nil
        refreshTask?.cancel()
        refreshTask = nil
        isLoading = false
        cancelRetry()
        stopPolling()
        stopRealtime()
        cancelInspectorRequests()
    }

    func stopPolling() {
        graphPollingState.stop()
        pollingTask?.cancel()
        pollingTask = nil
    }

    private func cancelRetry() {
        retryState.reset()
        retryTask?.cancel()
        retryTask = nil
        isRetrying = false
    }

    func stopRealtime() {
        realtimeTask?.cancel()
        realtimeTask = nil
        hasActiveRealtimeStream = false
    }

    var baseLookup: MessageTaskLookup {
        turn.resolvedMessageTaskLookup
    }

    var expectsTaskGraph: Bool {
        turn.messageTaskLookup != nil
            || initialTaskID != nil
            || initialRunID != nil
    }

    var shouldRetryEmptyGraph: Bool {
        graphPollingState.shouldRetryEmptyGraph(
            expectsTaskGraph: expectsTaskGraph,
            graph: graph
        )
    }

    func resetEmptyGraphRetryBudgetIfNeeded() {
        guard let nextIsAwaitingInitialGraph = graphPollingState.resetRetryBudgetIfNeeded(
            expectsTaskGraph: expectsTaskGraph,
            graph: graph
        ) else { return }
        if isAwaitingInitialGraph != nextIsAwaitingInitialGraph {
            isAwaitingInitialGraph = nextIsAwaitingInitialGraph
        }
    }

    func recordEmptyGraphRetryAttempt() {
        guard let nextIsAwaitingInitialGraph = graphPollingState.recordEmptyGraphRetry(
            expectsTaskGraph: expectsTaskGraph,
            graph: graph
        ) else { return }
        if isAwaitingInitialGraph != nextIsAwaitingInitialGraph {
            isAwaitingInitialGraph = nextIsAwaitingInitialGraph
        }
    }

    func applyGraph(_ graph: MessageTaskGraphSnapshot) {
        let nextIsAwaitingInitialGraph = graphPollingState.observe(
            graph: graph,
            expectsTaskGraph: expectsTaskGraph
        )
        if graph.nodes.isEmpty {
            if self.graph != graph {
                self.graph = graph
            }
            if isAwaitingInitialGraph != nextIsAwaitingInitialGraph {
                isAwaitingInitialGraph = nextIsAwaitingInitialGraph
            }
            return
        }

        if isAwaitingInitialGraph != nextIsAwaitingInitialGraph {
            isAwaitingInitialGraph = nextIsAwaitingInitialGraph
        }
        guard self.graph != graph else { return }
        self.graph = graph
        var normalized = MessageTaskGraphNormalizer.normalize(graph, mode: displayMode)
        if let initialTaskID,
           !normalized.nodes.contains(where: { $0.id == initialTaskID }),
           graph.nodes.contains(where: { $0.id == initialTaskID }) {
            displayMode = .full
            normalized = MessageTaskGraphNormalizer.normalize(graph, mode: .full)
        }
        if let selectedTask,
           let refreshed = normalized.nodes.first(where: { $0.id == selectedTask.id })?.task {
            self.selectedTask = refreshed
        } else if self.selectedTask == nil {
            let initial = initialTaskID.flatMap { taskID in
                normalized.nodes.first(where: { $0.id == taskID })
            }
                ?? normalized.nodes.first(where: { $0.task.normalizedStatus == "blocked" })
                ?? normalized.nodes.first(where: \.isCurrentMessage)
                ?? normalized.nodes.first
            if let initial {
                select(initial.task, section: initialRunID == nil ? nil : .run)
            }
        }
    }

    func applyRealtimeSignal(_ signal: ConversationRealtimeSignal) {
        guard signal.turnID == turn.id,
              let update = signal.processUpdate else { return }
        if let last = executionActivity.last,
           last.title == update.title,
           last.status == update.status {
            executionActivity[executionActivity.count - 1] = update
        } else {
            executionActivity.append(update)
            if executionActivity.count > 80 {
                executionActivity.removeFirst(executionActivity.count - 80)
            }
        }
    }

}
