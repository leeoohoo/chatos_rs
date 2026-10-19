import ChatOSCore
import Foundation

extension MessageTaskWorkspaceViewModel {
    func refreshWorkspaceState(
        refreshInspector: Bool,
        pollingSession: MessageTaskGraphPollingSession? = nil,
        workspaceSession: MessageTaskWorkspaceSession? = nil
    ) async {
        guard acceptsWorkspaceSession(workspaceSession),
              pollingSession.map(graphPollingState.accepts) ?? true else { return }
        workspaceRefreshGeneration += 1
        let refreshGeneration = workspaceRefreshGeneration

        do {
            let refreshedGraph = try await graphUseCase.loadGraph(
                messageID: turn.userMessage.id,
                lookup: baseLookup,
                preservingStable: graph
            )
            guard refreshGeneration == workspaceRefreshGeneration,
                  acceptsWorkspaceSession(workspaceSession),
                  pollingSession.map(graphPollingState.accepts) ?? true else { return }
            applyGraph(refreshedGraph)
            if errorMessage != nil {
                errorMessage = nil
            }
        } catch {
            guard refreshGeneration == workspaceRefreshGeneration,
                  acceptsWorkspaceSession(workspaceSession),
                  pollingSession.map(graphPollingState.accepts) ?? true else { return }
            let nextErrorMessage = error.localizedDescription
            if errorMessage != nextErrorMessage {
                errorMessage = nextErrorMessage
            }
        }

        if refreshInspector,
           acceptsWorkspaceSession(workspaceSession),
           pollingSession.map(graphPollingState.accepts) ?? true {
            await refreshSelectedInspectorState()
        }
    }

    func loadInspector(for task: MessageTask) {
        let token = beginInspectorSnapshotRequest(
            contextID: "\(task.id):task"
        )
        isLoadingInspector = true
        taskDetail = nil
        runDetail = nil
        loadedModelOutputRunID = nil
        let target = target(for: task)
        let requestedTaskID = task.id
        let graphUseCase = graphUseCase
        inspectorLoadTask = Task { [weak self] in
            let result: Result<MessageTask, Error>
            do {
                result = .success(
                    try await graphUseCase.loadTask(
                        messageID: target.messageID,
                        taskID: requestedTaskID,
                        lookup: target.lookup
                    )
                )
            } catch {
                result = .failure(error)
            }

            guard let self else { return }
            var loadedDetail: MessageTask?
            switch result {
            case let .success(detail):
                guard inspectorRequestState.accepts(token),
                      selectedTask?.id == requestedTaskID else { return }
                taskDetail = detail
                loadedDetail = detail
            case .failure(is CancellationError):
                guard !Task.isCancelled,
                      inspectorRequestState.accepts(token),
                      selectedTask?.id == requestedTaskID else { return }
                errorMessage = CancellationError().localizedDescription
            case let .failure(error):
                guard inspectorRequestState.accepts(token),
                      selectedTask?.id == requestedTaskID else { return }
                errorMessage = error.localizedDescription
            }
            guard inspectorRequestState.finish(token) else { return }
            inspectorLoadTask = nil
            isLoadingInspector = false
            guard let loadedDetail else { return }
            if inspectorSection == .detail {
                loadModelOutput(for: loadedDetail)
            } else if inspectorSection == .run {
                loadRun(for: loadedDetail)
            }
        }
    }

    func loadModelOutput(for task: MessageTask) {
        guard let runID = task.lastRunID,
              loadedModelOutputRunID != runID,
              !isLoadingModelOutput else { return }
        let token = beginInspectorSnapshotRequest(
            contextID: "\(task.id):model-output:\(runID)"
        )
        isLoadingModelOutput = true
        let target = target(for: task)
        let requestedTaskID = task.id
        let runUseCase = runUseCase
        modelOutputLoadTask = Task { [weak self] in
            let result: Result<MessageTaskRunDetail, Error>
            do {
                result = .success(
                    try await runUseCase.loadModelOutput(
                        messageID: target.messageID,
                        runID: runID,
                        lookup: target.lookup
                    )
                )
            } catch {
                result = .failure(error)
            }

            guard let self else { return }
            defer {
                if inspectorRequestState.finish(token) {
                    modelOutputLoadTask = nil
                    isLoadingModelOutput = false
                }
            }
            switch result {
            case let .success(detail):
                guard inspectorRequestState.accepts(token),
                      selectedTask?.id == requestedTaskID else { return }
                loadedModelOutputRunID = runID
                taskDetail = (taskDetail ?? task).merging(run: detail.run)
            case .failure(is CancellationError):
                return
            case let .failure(error):
                guard inspectorRequestState.accepts(token),
                      selectedTask?.id == requestedTaskID else { return }
                errorMessage = "模型输出加载失败：\(error.localizedDescription)"
            }
        }
    }

    func loadRun(for task: MessageTask) {
        let preferredRunID = task.id == initialTaskID ? initialRunID : nil
        guard let runID = preferredRunID ?? task.lastRunID, !isLoadingRun else { return }
        let token = beginInspectorSnapshotRequest(
            contextID: "\(task.id):run:\(runID)"
        )
        isLoadingRun = true
        let target = target(for: task)
        let requestedTaskID = task.id
        let runUseCase = runUseCase
        runLoadTask = Task { [weak self] in
            let result: Result<MessageTaskRunDetail, Error>
            do {
                result = .success(
                    try await runUseCase.loadLatestRun(
                        messageID: target.messageID,
                        runID: runID,
                        lookup: target.lookup
                    )
                )
            } catch {
                result = .failure(error)
            }

            guard let self else { return }
            defer {
                if inspectorRequestState.finish(token) {
                    runLoadTask = nil
                    isLoadingRun = false
                }
            }
            switch result {
            case let .success(detail):
                guard inspectorRequestState.accepts(token),
                      selectedTask?.id == requestedTaskID else { return }
                runDetail = detail
                loadedModelOutputRunID = runID
                taskDetail = detail.task.merging(run: detail.run)
            case .failure(is CancellationError):
                return
            case let .failure(error):
                guard inspectorRequestState.accepts(token),
                      selectedTask?.id == requestedTaskID else { return }
                errorMessage = error.localizedDescription
            }
        }
    }

    func loadMoreRunEvents() {
        guard let task = taskDetail ?? selectedTask,
              let current = runDetail,
              current.eventsHasMore,
              !isLoadingMoreRunEvents else { return }
        moreRunEventsTask?.cancel()
        let target = target(for: task)
        let token = inspectorRequestState.beginOlderEvents(
            contextID: "\(current.run.id):\(current.events.count)"
        )
        isLoadingMoreRunEvents = true
        let runUseCase = runUseCase
        moreRunEventsTask = Task { [weak self] in
            let result: Result<MessageTaskRunDetail, Error>
            do {
                result = .success(
                    try await runUseCase.loadOlderEvents(
                        messageID: target.messageID,
                        lookup: target.lookup,
                        current: current
                    )
                )
            } catch {
                result = .failure(error)
            }

            guard let self else { return }
            defer {
                if inspectorRequestState.finish(token) {
                    moreRunEventsTask = nil
                    isLoadingMoreRunEvents = false
                }
            }
            switch result {
            case let .success(merged):
                guard inspectorRequestState.accepts(token),
                      runDetail?.run.id == current.run.id else { return }
                runDetail = merged
            case .failure(is CancellationError):
                return
            case let .failure(error):
                guard inspectorRequestState.accepts(token),
                      runDetail?.run.id == current.run.id else { return }
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
        cancelInspectorRequests()
        let requestedTaskID = selectedTask.id
        let requestedSection = inspectorSection
        let token = inspectorRequestState.beginSnapshot(
            contextID: "\(requestedTaskID):refresh:\(requestedSection.rawValue)"
        )
        defer { _ = inspectorRequestState.finish(token) }
        let target = target(for: selectedTask)
        do {
            let detail = try await graphUseCase.loadTask(
                messageID: target.messageID,
                taskID: selectedTask.id,
                lookup: target.lookup
            )
            guard inspectorRequestState.accepts(token),
                  self.selectedTask?.id == requestedTaskID else { return }
            taskDetail = detail
            let update = try await inspectorRefreshUseCase.refreshSection(
                requestedSection.applicationSection,
                messageID: target.messageID,
                lookup: target.lookup,
                task: detail
            )
            guard inspectorRequestState.accepts(token),
                  self.selectedTask?.id == requestedTaskID,
                  inspectorSection == requestedSection else { return }
            switch update {
            case .preserve:
                break
            case .clearRun:
                runDetail = nil
                loadedModelOutputRunID = nil
            case let .modelOutput(task, runID):
                loadedModelOutputRunID = runID
                taskDetail = task
            case let .run(task, detail):
                runDetail = detail
                loadedModelOutputRunID = detail.run.id
                taskDetail = task
            }
        } catch {
            guard inspectorRequestState.accepts(token),
                  self.selectedTask?.id == requestedTaskID else { return }
            errorMessage = error.localizedDescription
        }
    }

    private func beginInspectorSnapshotRequest(
        contextID: String
    ) -> MessageTaskInspectorRequestToken {
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
        return inspectorRequestState.beginSnapshot(contextID: contextID)
    }

    func startRealtime(workspaceSession: MessageTaskWorkspaceSession) {
        guard let realtimeService, realtimeTask == nil else { return }
        let sessionID = turn.sessionID
        realtimeTask = Task { [weak self] in
            let stream = await realtimeService.events(sessionID: sessionID)
            guard let self,
                  !Task.isCancelled,
                  self.workspaceLifecycleState.accepts(workspaceSession) else { return }
            self.hasActiveRealtimeStream = true
            self.startPollingIfNeeded(workspaceSession: workspaceSession)
            do {
                for try await signal in stream {
                    guard !Task.isCancelled,
                          self.workspaceLifecycleState.accepts(workspaceSession) else { return }
                    let shouldRefreshWorkspace = self.graphUseCase.shouldRefreshWorkspace(
                        for: signal,
                        turnID: self.turn.id
                    )
                    if signal.kind != .reconcile {
                        self.applyRealtimeSignal(signal)
                    }
                    if shouldRefreshWorkspace {
                        await self.refreshWorkspaceState(
                            refreshInspector: true,
                            workspaceSession: workspaceSession
                        )
                        guard !Task.isCancelled,
                              self.workspaceLifecycleState.accepts(workspaceSession) else { return }
                        self.startPollingIfNeeded(workspaceSession: workspaceSession)
                    }
                }
            } catch {
                guard !Task.isCancelled,
                      self.workspaceLifecycleState.accepts(workspaceSession) else { return }
                if self.errorMessage == nil {
                    self.errorMessage = "实时进度连接已中断：\(error.localizedDescription)"
                }
            }
            guard !Task.isCancelled,
                  self.workspaceLifecycleState.accepts(workspaceSession) else { return }
            self.hasActiveRealtimeStream = false
            self.realtimeTask = nil
            self.startPollingIfNeeded(workspaceSession: workspaceSession)
        }
    }

    func startPollingIfNeeded(
        force: Bool = false,
        workspaceSession: MessageTaskWorkspaceSession? = nil
    ) {
        guard acceptsWorkspaceSession(workspaceSession) else { return }
        stopPolling()
        let shouldPoll = graphPollingState.shouldPoll(
            graph: graph,
            expectsTaskGraph: expectsTaskGraph
        )
        guard force || shouldPoll else { return }
        let session = graphPollingState.startSession()
        pollingTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self,
                      !Task.isCancelled,
                      self.acceptsWorkspaceSession(workspaceSession),
                      self.graphPollingState.accepts(session) else { return }
                let isEmptyGraphRetry = self.shouldRetryEmptyGraph
                let interval = MessageTaskGraphPollingState.interval(
                    isEmptyGraphRetry: isEmptyGraphRetry,
                    hasActiveRealtimeStream: self.hasActiveRealtimeStream
                )
                try? await Task.sleep(for: interval)
                guard !Task.isCancelled,
                      self.acceptsWorkspaceSession(workspaceSession),
                      self.graphPollingState.accepts(session) else { return }
                await self.refreshWorkspaceState(
                    refreshInspector: true,
                    pollingSession: session,
                    workspaceSession: workspaceSession
                )
                guard !Task.isCancelled,
                      self.acceptsWorkspaceSession(workspaceSession),
                      self.graphPollingState.accepts(session) else { return }
                if isEmptyGraphRetry {
                    self.recordEmptyGraphRetryAttempt()
                }
                let shouldContinue = self.graphPollingState.shouldPoll(
                    graph: self.graph,
                    expectsTaskGraph: self.expectsTaskGraph
                )
                if !shouldContinue {
                    if self.graphPollingState.finish(session) {
                        self.pollingTask = nil
                    }
                    return
                }
            }
        }
    }

    private func acceptsWorkspaceSession(
        _ session: MessageTaskWorkspaceSession?
    ) -> Bool {
        session.map(workspaceLifecycleState.accepts) ?? true
    }
}

private extension MessageTaskWorkspaceViewModel.InspectorSection {
    var applicationSection: MessageTaskInspectorSection {
        switch self {
        case .detail: .detail
        case .process: .process
        case .run: .run
        }
    }
}
