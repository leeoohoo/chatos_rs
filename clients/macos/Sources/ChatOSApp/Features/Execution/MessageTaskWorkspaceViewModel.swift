import ChatOSCore
import Foundation

@MainActor
final class MessageTaskWorkspaceViewModel: ObservableObject {
    static let emptyGraphRetryLimit = 3

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

    let graphService: any MessageTaskGraphServicing
    let realtimeService: (any ConversationRealtimeStreaming)?
    let initialTaskID: String?
    let initialRunID: String?
    var pollingTask: Task<Void, Never>?
    var realtimeTask: Task<Void, Never>?
    var loadedModelOutputRunID: String?
    var workspaceRefreshGeneration = 0
    var emptyGraphRetryAttemptsRemaining = MessageTaskWorkspaceViewModel.emptyGraphRetryLimit

    init(
        turn: ConversationTurn,
        graphService: any MessageTaskGraphServicing,
        realtimeService: (any ConversationRealtimeStreaming)? = nil,
        initialTaskID: String? = nil,
        initialRunID: String? = nil
    ) {
        self.turn = turn
        self.graphService = graphService
        self.realtimeService = realtimeService
        self.initialTaskID = initialTaskID
        self.initialRunID = initialRunID
        if initialRunID != nil {
            inspectorSection = .run
        }
    }

    deinit {
        pollingTask?.cancel()
        realtimeTask?.cancel()
    }

    var displayGraph: MessageTaskGraphSnapshot? {
        graph.map { MessageTaskGraphNormalizer.normalize($0, mode: displayMode) }
    }

    var selectedTaskID: String? { selectedTask?.id }

    func load() {
        guard !isLoading else { return }
        isLoading = true
        errorMessage = nil
        Task {
            await refreshWorkspaceState(refreshInspector: false)
            startRealtime()
            startPollingIfNeeded()
            isLoading = false
        }
    }

    func refresh() {
        errorMessage = nil
        resetEmptyGraphRetryBudgetIfNeeded()
        Task {
            await refreshWorkspaceState(refreshInspector: true)
            startPollingIfNeeded()
        }
    }

    func select(_ task: MessageTask, section: InspectorSection? = nil) {
        selectedTask = task
        if let section { inspectorSection = section }
        loadInspector(for: task)
        ensureInspectorSectionLoaded()
    }

    func ensureInspectorSectionLoaded() {
        guard let task = taskDetail ?? selectedTask else { return }
        switch inspectorSection {
        case .detail:
            loadModelOutput(for: task)
        case .process:
            break
        case .run:
            guard runDetail == nil, !isLoadingRun else { return }
            loadRun(for: task)
        }
    }

    func retrySelectedRun() {
        guard let task = selectedTask,
              let runID = task.lastRunID ?? runDetail?.run.id,
              !isRetrying else { return }
        isRetrying = true
        errorMessage = nil
        let target = target(for: task)
        Task {
            do {
                _ = try await graphService.retryRun(
                    messageID: target.messageID,
                    runID: runID,
                    lookup: target.lookup,
                    instruction: retryInstruction
                )
                retryInstruction = ""
                inspectorSection = .run
                refresh()
                loadInspector(for: task)
            } catch {
                errorMessage = error.localizedDescription
            }
            isRetrying = false
        }
    }

    func stopPolling() {
        pollingTask?.cancel()
        pollingTask = nil
    }

    func stopRealtime() {
        realtimeTask?.cancel()
        realtimeTask = nil
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
        expectsTaskGraph
            && graph?.nodes.isEmpty == true
            && emptyGraphRetryAttemptsRemaining > 0
    }

    func resetEmptyGraphRetryBudgetIfNeeded() {
        guard graph?.nodes.isEmpty != false, expectsTaskGraph else { return }
        emptyGraphRetryAttemptsRemaining = Self.emptyGraphRetryLimit
        isAwaitingInitialGraph = graph != nil
    }

    func recordEmptyGraphRetryAttempt() {
        guard graph?.nodes.isEmpty == true else { return }
        emptyGraphRetryAttemptsRemaining = max(0, emptyGraphRetryAttemptsRemaining - 1)
        isAwaitingInitialGraph = shouldRetryEmptyGraph
    }

    func applyGraph(_ graph: MessageTaskGraphSnapshot) {
        if graph.nodes.isEmpty {
            // A graph that has already been observed is stable for the lifetime of a
            // message. Do not let a transient empty gateway response erase the canvas.
            guard self.graph?.nodes.isEmpty != false else { return }
            self.graph = graph
            isAwaitingInitialGraph = shouldRetryEmptyGraph
            return
        }

        self.graph = graph
        emptyGraphRetryAttemptsRemaining = 0
        isAwaitingInitialGraph = false
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
