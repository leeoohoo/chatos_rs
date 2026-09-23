import ChatOSCore
import Foundation

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
}

@MainActor
final class TaskReplyInspectorViewModel: ObservableObject {
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

    private let service: any MessageTaskGraphServicing
    private var loadTask: Task<Void, Never>?
    private var pollingTask: Task<Void, Never>?
    private var loadGeneration = 0
    private var loadedModelOutputRunID: String?

    init(selection: TaskReplySelection, service: any MessageTaskGraphServicing) {
        self.selection = selection
        self.section = selection.initialSection
        self.service = service
    }

    deinit {
        loadTask?.cancel()
        pollingTask?.cancel()
    }

    func load() {
        guard task == nil else { return }
        refresh()
    }

    func update(selection: TaskReplySelection) {
        let needsRefresh = self.selection.refreshIdentity != selection.refreshIdentity
        self.selection = selection
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
        case .detail:
            let runID = task?.lastRunID ?? selection.reply.taskCallback?.runID
            if runID != loadedModelOutputRunID {
                refresh()
            }
        default:
            refresh()
        }
    }

    func refresh() {
        guard let callback = selection.reply.taskCallback else { return }
        pollingTask?.cancel()
        pollingTask = nil
        loadGeneration += 1
        let generation = loadGeneration
        let selection = selection
        let requestedSection = section
        loadTask?.cancel()
        isLoading = true
        errorMessage = nil
        modelOutputError = nil
        isLoadingModelOutput = requestedSection == .detail
        loadTask = Task { [weak self] in
            guard let self else { return }
            do {
                switch requestedSection {
                case .process:
                    let loadedTask = try await fetchTask(callback: callback, selection: selection)
                    guard !Task.isCancelled, generation == loadGeneration else { return }
                    apply(loadedTask)
                case .detail:
                    // A reply callback identifies the run that originally emitted the reply.
                    // After a retry that ID is historical, so fetch the task first and follow its
                    // current lastRunID instead of repeatedly rendering the original failure.
                    let loadedTask = try await fetchTask(callback: callback, selection: selection)
                    guard !Task.isCancelled, generation == loadGeneration else { return }
                    apply(loadedTask)
                    await loadLatestRun(
                        for: loadedTask,
                        callback: callback,
                        selection: selection,
                        generation: generation
                    )
                }
            } catch {
                guard !Task.isCancelled, generation == loadGeneration else { return }
                errorMessage = error.localizedDescription
            }
            guard !Task.isCancelled, generation == loadGeneration else { return }
            isLoading = false
            isLoadingModelOutput = false
            scheduleActiveRefreshIfNeeded()
        }
    }

    func retry() {
        guard let callback = selection.reply.taskCallback,
              let runID = task?.lastRunID ?? callback.runID,
              !isRetrying else { return }
        isRetrying = true
        errorMessage = nil
        Task {
            do {
                _ = try await service.retryRun(
                    messageID: selection.reply.message.id,
                    runID: runID,
                    lookup: lookup(callback, selection: selection),
                    instruction: retryInstruction
                )
                retryInstruction = ""
                task = nil
                processTimelineItems = []
                loadedModelOutputRunID = nil
                modelOutputError = nil
                isRetrying = false
                refresh()
            } catch {
                errorMessage = error.localizedDescription
                isRetrying = false
            }
        }
    }

    private func loadLatestRun(
        for loadedTask: MessageTask,
        callback: TaskRunnerCallbackReference,
        selection: TaskReplySelection,
        generation: Int
    ) async {
        guard let runID = loadedTask.lastRunID ?? callback.runID else { return }
        do {
            let detail = try await service.fetchRun(
                messageID: selection.reply.message.id,
                runID: runID,
                lookup: lookup(callback, selection: selection),
                includeEvents: false,
                eventLimit: 1,
                eventOffset: 0
            )
            guard !Task.isCancelled,
                  generation == loadGeneration,
                  task?.id == loadedTask.id else { return }
            apply(detail.task.merging(run: detail.run))
            loadedModelOutputRunID = detail.run.id
        } catch {
            guard !Task.isCancelled, generation == loadGeneration else { return }
            modelOutputError = error.localizedDescription
        }
    }

    private func fetchTask(
        callback: TaskRunnerCallbackReference,
        selection: TaskReplySelection
    ) async throws -> MessageTask {
        try await service.fetchTask(
            messageID: selection.reply.message.id,
            taskID: callback.taskID,
            lookup: lookup(callback, selection: selection)
        )
    }

    private func apply(_ loadedTask: MessageTask) {
        task = loadedTask
        processTimelineItems = TaskProcessTimelineBuilder.build(
            processLog: loadedTask.processLog,
            taskStatus: loadedTask.status
        )
    }

    private func scheduleActiveRefreshIfNeeded() {
        pollingTask?.cancel()
        pollingTask = nil
        guard task?.isActive == true else { return }
        pollingTask = Task { [weak self] in
            do {
                try await Task.sleep(for: .seconds(2))
            } catch {
                return
            }
            guard let self, !Task.isCancelled else { return }
            self.refresh()
        }
    }

    private func lookup(
        _ callback: TaskRunnerCallbackReference,
        selection: TaskReplySelection
    ) -> MessageTaskLookup {
        MessageTaskLookup(
            sessionID: callback.sourceSessionID ?? selection.turn.sessionID,
            turnID: callback.sourceTurnID ?? selection.turn.id,
            sourceUserMessageID: callback.sourceUserMessageID ?? selection.turn.userMessage.id
        )
    }
}
