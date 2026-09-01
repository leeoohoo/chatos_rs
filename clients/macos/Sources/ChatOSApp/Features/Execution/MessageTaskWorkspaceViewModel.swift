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
    @Published private(set) var executionContext: ProjectExecutionContext?
    @Published private(set) var executionActivity: [ConversationRealtimeProcessUpdate] = []
    @Published private(set) var executionFailureReason: String?
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
    @Published var isMutatingPlan = false
    @Published var isPlanStopped = false
    @Published var displayMode: MessageTaskGraphDisplayMode = .reduced
    @Published var inspectorSection: InspectorSection = .detail
    @Published var retryInstruction = ""
    @Published var planActionMessage: String?
    @Published var errorMessage: String?

    let graphService: any MessageTaskGraphServicing
    let projectExecutionService: (any ProjectExecutionServicing)?
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
        projectExecutionService: (any ProjectExecutionServicing)?,
        realtimeService: (any ConversationRealtimeStreaming)? = nil,
        initialTaskID: String? = nil,
        initialRunID: String? = nil
    ) {
        self.turn = turn
        self.executionContext = turn.projectExecutionContext
        self.graphService = graphService
        self.projectExecutionService = projectExecutionService
        self.realtimeService = realtimeService
        self.initialTaskID = initialTaskID
        self.initialRunID = initialRunID
        if turn.projectExecutionContext?.isProjectExecution == true {
            inspectorSection = .process
            executionActivity = [
                ConversationRealtimeProcessUpdate(
                    id: "execution-requested-\(turn.id)",
                    title: "已提交执行计划生成请求",
                    detail: "正在等待规划 Agent 返回实时进度",
                    status: "running",
                    timestamp: ISO8601DateFormatter().string(from: turn.startedAt)
                ),
            ]
        }
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

    var executionState: ProjectExecutionConfirmationState {
        ProjectExecutionConfirmationState(
            context: executionContext,
            graph: graph,
            conversationID: turn.sessionID
        )
    }

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
            || turn.projectExecutionContext != nil
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

    func applyExecution(_ launch: ProjectRequirementExecutionLaunch) {
        let previousStatus = normalizedExecutionStatus(executionContext?.overallStatus)
        executionContext = ProjectExecutionContext(
            projectID: launch.projectID,
            requirementID: launch.requirementID,
            executionGroupID: launch.executionGroupID,
            contactID: launch.contactID,
            mode: "project_requirement_execution",
            executionKind: "project_requirement_execution",
            confirmationStatus: launch.confirmationStatus,
            overallStatus: launch.overallStatus
        )
        executionFailureReason = launch.failureReason

        let nextStatus = normalizedExecutionStatus(launch.overallStatus ?? launch.confirmationStatus)
        guard nextStatus != previousStatus else { return }
        switch nextStatus {
        case "awaiting_confirmation":
            appendExecutionActivity(
                title: "执行计划已生成",
                detail: "请检查任务节点和依赖关系后确认执行",
                status: "completed"
            )
        case "processing", "running", "confirmed", "executing", "in_progress":
            appendExecutionActivity(
                title: "已确认执行，任务开始运行",
                detail: "任务将按照依赖顺序自动刷新",
                status: "running"
            )
        case "completed":
            planActionMessage = nil
            appendExecutionActivity(title: "全部任务已完成", status: "completed")
        case "failed", "error", "blocked":
            planActionMessage = nil
            appendExecutionActivity(
                title: "执行计划失败",
                detail: launch.failureReason,
                status: "failed"
            )
        case "stopped", "cancelled", "canceled":
            planActionMessage = nil
            appendExecutionActivity(title: "执行计划已停止", status: "cancelled")
        default:
            break
        }
    }

    func markExecutionStarted() {
        executionContext?.confirmationStatus = "confirmed"
        executionContext?.overallStatus = "processing"
        appendExecutionActivity(
            title: "已确认执行，正在启动根任务",
            status: "running"
        )
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

    private func appendExecutionActivity(
        title: String,
        detail: String? = nil,
        status: String
    ) {
        let update = ConversationRealtimeProcessUpdate(
            id: UUID().uuidString,
            title: title,
            detail: detail,
            status: status,
            timestamp: ISO8601DateFormatter().string(from: Date())
        )
        if executionActivity.last?.title != title {
            executionActivity.append(update)
        }
    }

    private func normalizedExecutionStatus(_ value: String?) -> String {
        value?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() ?? ""
    }

}
