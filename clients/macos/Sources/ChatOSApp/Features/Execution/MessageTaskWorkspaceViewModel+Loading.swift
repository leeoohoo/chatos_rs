import ChatOSCore
import Foundation

extension MessageTaskWorkspaceViewModel {
    func refreshWorkspaceState(refreshInspector: Bool) async {
        do {
            applyGraph(
                try await graphService.fetchGraph(
                    messageID: turn.userMessage.id,
                    lookup: baseLookup
                )
            )
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }

        if let service = projectExecutionService,
           let identity = executionState.identity {
            do {
                if let launch = try await service.fetchExecution(identity) {
                    applyExecution(launch)
                }
            } catch {
                if errorMessage == nil {
                    errorMessage = "执行计划状态刷新失败：\(error.localizedDescription)"
                }
            }
        }

        if refreshInspector {
            await refreshSelectedInspectorState()
        }
    }

    func loadInspector(for task: MessageTask) {
        isLoadingInspector = true
        isLoadingRun = false
        taskDetail = nil
        runDetail = nil
        loadedModelOutputRunID = nil
        isLoadingModelOutput = false
        isLoadingMoreRunEvents = false
        let target = target(for: task)
        let requestedTaskID = task.id
        Task {
            defer {
                if selectedTask?.id == requestedTaskID {
                    isLoadingInspector = false
                }
            }
            do {
                let detail = try await graphService.fetchTask(
                    messageID: target.messageID,
                    taskID: task.id,
                    lookup: target.lookup
                )
                guard selectedTask?.id == requestedTaskID else { return }
                taskDetail = detail
                if inspectorSection == .detail {
                    loadModelOutput(for: detail)
                } else if inspectorSection == .run {
                    loadRun(for: detail)
                }
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func loadModelOutput(for task: MessageTask) {
        guard let runID = task.lastRunID,
              loadedModelOutputRunID != runID,
              !isLoadingModelOutput else { return }
        isLoadingModelOutput = true
        let target = target(for: task)
        let requestedTaskID = task.id
        Task {
            defer {
                if selectedTask?.id == requestedTaskID {
                    isLoadingModelOutput = false
                }
            }
            do {
                let detail = try await graphService.fetchRun(
                    messageID: target.messageID,
                    runID: runID,
                    lookup: target.lookup,
                    includeEvents: false,
                    eventLimit: 1,
                    eventOffset: 0
                )
                guard selectedTask?.id == requestedTaskID else { return }
                loadedModelOutputRunID = runID
                taskDetail = (taskDetail ?? task).merging(run: detail.run)
            } catch {
                guard selectedTask?.id == requestedTaskID else { return }
                errorMessage = "模型输出加载失败：\(error.localizedDescription)"
            }
        }
    }

    func loadRun(for task: MessageTask) {
        let preferredRunID = task.id == initialTaskID ? initialRunID : nil
        guard let runID = preferredRunID ?? task.lastRunID, !isLoadingRun else { return }
        isLoadingRun = true
        let target = target(for: task)
        let requestedTaskID = task.id
        Task {
            defer {
                if selectedTask?.id == requestedTaskID {
                    isLoadingRun = false
                }
            }
            do {
                let detail = try await graphService.fetchRun(
                    messageID: target.messageID,
                    runID: runID,
                    lookup: target.lookup,
                    includeEvents: true,
                    eventLimit: 40,
                    eventOffset: 0
                )
                guard selectedTask?.id == requestedTaskID else { return }
                runDetail = detail
                loadedModelOutputRunID = runID
                taskDetail = detail.task.merging(run: detail.run)
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func loadMoreRunEvents() {
        guard let task = taskDetail ?? selectedTask,
              let current = runDetail,
              current.eventsHasMore,
              !isLoadingMoreRunEvents else { return }
        let target = target(for: task)
        isLoadingMoreRunEvents = true
        Task {
            defer { isLoadingMoreRunEvents = false }
            do {
                let page = try await graphService.fetchRun(
                    messageID: target.messageID,
                    runID: current.run.id,
                    lookup: target.lookup,
                    includeEvents: true,
                    eventLimit: 50,
                    eventOffset: current.events.count
                )
                guard runDetail?.run.id == current.run.id else { return }
                var merged = current
                let existingIDs = Set(current.events.map(\.id))
                merged.events.append(contentsOf: page.events.filter { !existingIDs.contains($0.id) })
                merged.eventsTotal = page.eventsTotal
                merged.eventsHasMore = page.eventsHasMore
                merged.task = page.task
                merged.run = page.run
                runDetail = merged
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func target(for task: MessageTask) -> (messageID: String, lookup: MessageTaskLookup) {
        let messageID = task.sourceUserMessageID?.isEmpty == false
            ? task.sourceUserMessageID!
            : turn.userMessage.id
        return (
            messageID,
            MessageTaskLookup(
                sessionID: task.sourceSessionID ?? graph?.sourceSessionID ?? turn.sessionID,
                turnID: task.sourceTurnID ?? graph?.sourceTurnID ?? turn.id,
                sourceUserMessageID: task.sourceUserMessageID
                    ?? graph?.sourceUserMessageID
                    ?? turn.userMessage.id
            )
        )
    }

    func refreshSelectedInspectorState() async {
        guard let selectedTask else { return }
        let requestedTaskID = selectedTask.id
        let target = target(for: selectedTask)
        do {
            let detail = try await graphService.fetchTask(
                messageID: target.messageID,
                taskID: selectedTask.id,
                lookup: target.lookup
            )
            guard self.selectedTask?.id == requestedTaskID else { return }
            taskDetail = detail

            guard let runID = detail.lastRunID else {
                runDetail = nil
                loadedModelOutputRunID = nil
                return
            }
            switch inspectorSection {
            case .process:
                break
            case .detail:
                let run = try await graphService.fetchRun(
                    messageID: target.messageID,
                    runID: runID,
                    lookup: target.lookup,
                    includeEvents: false,
                    eventLimit: 1,
                    eventOffset: 0
                )
                guard self.selectedTask?.id == requestedTaskID else { return }
                loadedModelOutputRunID = runID
                taskDetail = detail.merging(run: run.run)
            case .run:
                let run = try await graphService.fetchRun(
                    messageID: target.messageID,
                    runID: runID,
                    lookup: target.lookup,
                    includeEvents: true,
                    eventLimit: 40,
                    eventOffset: 0
                )
                guard self.selectedTask?.id == requestedTaskID else { return }
                runDetail = run
                loadedModelOutputRunID = runID
                taskDetail = run.task.merging(run: run.run)
            }
        } catch {
            guard self.selectedTask?.id == requestedTaskID else { return }
            errorMessage = error.localizedDescription
        }
    }

    func startRealtime() {
        guard let realtimeService, realtimeTask == nil else { return }
        let sessionID = turn.sessionID
        realtimeTask = Task { [weak self] in
            let stream = await realtimeService.events(sessionID: sessionID)
            do {
                for try await signal in stream {
                    guard let self, !Task.isCancelled else { return }
                    self.applyRealtimeSignal(signal)
                    if signal.turnID == self.turn.id,
                       [.completed, .failed, .cancelled, .persisted].contains(signal.kind) {
                        await self.refreshWorkspaceState(refreshInspector: true)
                        self.startPollingIfNeeded()
                    }
                }
            } catch {
                guard let self, !Task.isCancelled else { return }
                if self.errorMessage == nil {
                    self.errorMessage = "实时进度连接已中断：\(error.localizedDescription)"
                }
            }
        }
    }

    func startPollingIfNeeded(force: Bool = false) {
        stopPolling()
        let shouldPoll = executionState.isProjectExecution
            ? [.planning, .running].contains(executionState.phase)
            : graph?.nodes.contains(where: { $0.task.isActive }) == true
        guard force || shouldPoll else { return }
        pollingTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(2))
                guard let self, !Task.isCancelled else { return }
                await self.refreshWorkspaceState(refreshInspector: true)
                let shouldContinue = self.executionState.isProjectExecution
                    ? [.planning, .running].contains(self.executionState.phase)
                    : self.graph?.nodes.contains(where: { $0.task.isActive }) == true
                if !shouldContinue {
                    return
                }
            }
        }
    }
}

extension MessageTask {
    var normalizedStatus: String {
        status?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() ?? "unknown"
    }

    var isActive: Bool {
        ["pending", "queued", "ready", "running", "processing", "in_progress", "doing"]
            .contains(normalizedStatus)
    }
}
