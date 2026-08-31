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
    @Published private(set) var isLoading = false
    @Published private(set) var isLoadingModelOutput = false
    @Published private(set) var isRetrying = false
    @Published var retryInstruction = ""
    @Published var errorMessage: String?
    @Published private(set) var modelOutputError: String?

    private let service: any MessageTaskGraphServicing
    private var loadTask: Task<Void, Never>?
    private var loadGeneration = 0

    init(selection: TaskReplySelection, service: any MessageTaskGraphServicing) {
        self.selection = selection
        self.section = selection.initialSection
        self.service = service
    }

    deinit {
        loadTask?.cancel()
    }

    func load() {
        guard task == nil else { return }
        refresh()
    }

    func update(selection: TaskReplySelection) {
        let needsRefresh = self.selection.refreshIdentity != selection.refreshIdentity
        self.selection = selection
        guard needsRefresh else { return }
        refresh()
    }

    func selectSection(_ section: TaskReplyInspectorSection) {
        let changed = self.section != section
        self.section = section
        if changed {
            refresh()
        }
    }

    func refresh() {
        guard let callback = selection.reply.taskCallback else { return }
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
                let loadedTask = try await service.fetchTask(
                    messageID: selection.reply.message.id,
                    taskID: callback.taskID,
                    lookup: lookup(callback, selection: selection)
                )
                guard !Task.isCancelled, generation == loadGeneration else { return }
                task = loadedTask
                await loadLatestRun(
                    for: loadedTask,
                    callback: callback,
                    selection: selection,
                    generation: generation
                )
            } catch {
                guard !Task.isCancelled, generation == loadGeneration else { return }
                errorMessage = error.localizedDescription
            }
            guard !Task.isCancelled, generation == loadGeneration else { return }
            isLoading = false
            isLoadingModelOutput = false
        }
    }

    func retry() {
        guard let callback = selection.reply.taskCallback,
              let runID = callback.runID ?? task?.lastRunID,
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
        guard let runID = callback.runID ?? loadedTask.lastRunID else { return }
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
            task = detail.task.merging(run: detail.run)
        } catch {
            guard !Task.isCancelled, generation == loadGeneration else { return }
            modelOutputError = error.localizedDescription
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
