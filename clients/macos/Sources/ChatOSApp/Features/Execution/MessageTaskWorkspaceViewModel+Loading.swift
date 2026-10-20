import ChatOSCore
import Foundation

extension MessageTaskWorkspaceViewModel {
    struct WorkspaceGraphRefreshRequest: Sendable {
        let generation: Int
        let graphUseCase: MessageTaskGraphUseCase
        let messageID: String
        let lookup: MessageTaskLookup
        let stableGraph: MessageTaskGraphSnapshot?
        let pollingSession: MessageTaskGraphPollingSession?
        let workspaceSession: MessageTaskWorkspaceSession?
    }

    struct WorkspaceInspectorRefreshRequest: Sendable {
        let token: MessageTaskInspectorRequestToken
        let requestedTaskID: String
        let requestedSection: InspectorSection
        let selectedTask: MessageTask
        let messageID: String
        let lookup: MessageTaskLookup
        let graphUseCase: MessageTaskGraphUseCase
        let inspectorRefreshUseCase: MessageTaskInspectorRefreshUseCase
    }

    struct WorkspaceInspectorRefreshOutcome {
        let detail: MessageTask?
        let update: MessageTaskInspectorSectionUpdate?
        let error: Error?
    }

    func beginWorkspaceGraphRefresh(
        pollingSession: MessageTaskGraphPollingSession? = nil,
        workspaceSession: MessageTaskWorkspaceSession? = nil
    ) -> WorkspaceGraphRefreshRequest {
        workspaceRefreshGeneration += 1
        return WorkspaceGraphRefreshRequest(
            generation: workspaceRefreshGeneration,
            graphUseCase: graphUseCase,
            messageID: turn.userMessage.id,
            lookup: baseLookup,
            stableGraph: graph,
            pollingSession: pollingSession,
            workspaceSession: workspaceSession
        )
    }

    nonisolated static func fetchWorkspaceGraph(
        _ request: WorkspaceGraphRefreshRequest
    ) async -> Result<MessageTaskGraphSnapshot, Error> {
        do {
            return .success(try await request.graphUseCase.loadGraph(
                messageID: request.messageID,
                lookup: request.lookup,
                preservingStable: request.stableGraph
            ))
        } catch {
            return .failure(error)
        }
    }

    @discardableResult
    func applyWorkspaceGraphResult(
        _ result: Result<MessageTaskGraphSnapshot, Error>,
        request: WorkspaceGraphRefreshRequest
    ) -> Bool {
        guard request.generation == workspaceRefreshGeneration,
              acceptsWorkspaceSession(request.workspaceSession),
              request.pollingSession.map(graphPollingState.accepts) ?? true else { return false }
        switch result {
        case let .success(refreshedGraph):
            applyGraph(refreshedGraph)
            if errorMessage != nil {
                errorMessage = nil
            }
        case let .failure(error):
            let nextErrorMessage = error.localizedDescription
            if errorMessage != nextErrorMessage {
                errorMessage = nextErrorMessage
            }
        }
        return true
    }

    func beginSelectedInspectorRefresh() -> WorkspaceInspectorRefreshRequest? {
        guard let selectedTask else { return nil }
        cancelInspectorRequests()
        let requestedSection = inspectorSection
        let token = inspectorRequestState.beginSnapshot(
            contextID: "\(selectedTask.id):refresh:\(requestedSection.rawValue)"
        )
        let target = target(for: selectedTask)
        return WorkspaceInspectorRefreshRequest(
            token: token,
            requestedTaskID: selectedTask.id,
            requestedSection: requestedSection,
            selectedTask: selectedTask,
            messageID: target.messageID,
            lookup: target.lookup,
            graphUseCase: graphUseCase,
            inspectorRefreshUseCase: inspectorRefreshUseCase
        )
    }

    nonisolated static func fetchSelectedInspectorRefresh(
        _ request: WorkspaceInspectorRefreshRequest
    ) async -> WorkspaceInspectorRefreshOutcome {
        do {
            let detail = try await request.graphUseCase.loadTask(
                messageID: request.messageID,
                taskID: request.requestedTaskID,
                lookup: request.lookup
            )
            do {
                let update = try await request.inspectorRefreshUseCase.refreshSection(
                    request.requestedSection.applicationSection,
                    messageID: request.messageID,
                    lookup: request.lookup,
                    task: detail
                )
                return WorkspaceInspectorRefreshOutcome(
                    detail: detail,
                    update: update,
                    error: nil
                )
            } catch {
                return WorkspaceInspectorRefreshOutcome(
                    detail: detail,
                    update: nil,
                    error: error
                )
            }
        } catch {
            return WorkspaceInspectorRefreshOutcome(
                detail: nil,
                update: nil,
                error: error
            )
        }
    }

    @discardableResult
    func applySelectedInspectorRefresh(
        _ outcome: WorkspaceInspectorRefreshOutcome,
        request: WorkspaceInspectorRefreshRequest
    ) -> Bool {
        guard inspectorRequestState.accepts(request.token),
              selectedTask?.id == request.requestedTaskID,
              inspectorSection == request.requestedSection else { return false }
        defer { _ = inspectorRequestState.finish(request.token) }
        if let detail = outcome.detail {
            taskDetail = detail
        }
        if let error = outcome.error {
            errorMessage = error.localizedDescription
            return true
        }
        guard let update = outcome.update else { return true }
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
        return true
    }

    func refreshWorkspaceState(
        refreshInspector: Bool,
        pollingSession: MessageTaskGraphPollingSession? = nil,
        workspaceSession: MessageTaskWorkspaceSession? = nil
    ) async {
        guard acceptsWorkspaceSession(workspaceSession),
              pollingSession.map(graphPollingState.accepts) ?? true else { return }
        let request = beginWorkspaceGraphRefresh(
            pollingSession: pollingSession,
            workspaceSession: workspaceSession
        )
        let result = await Self.fetchWorkspaceGraph(request)
        guard applyWorkspaceGraphResult(result, request: request) else { return }

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
        guard let request = beginSelectedInspectorRefresh() else { return }
        let outcome = await Self.fetchSelectedInspectorRefresh(request)
        applySelectedInspectorRefresh(outcome, request: request)
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
        guard let realtimeUseCase, realtimeTask == nil else { return }
        let sessionID = turn.sessionID
        realtimeTask = Task { [weak self] in
            let stream = await realtimeUseCase.events(sessionID: sessionID)
            guard !Task.isCancelled,
                  self?.workspaceLifecycleState.accepts(workspaceSession) == true else { return }
            self?.hasActiveRealtimeStream = true
            self?.startPollingIfNeeded(workspaceSession: workspaceSession)
            do {
                for try await signal in stream {
                    guard !Task.isCancelled,
                          self?.workspaceLifecycleState.accepts(workspaceSession) == true else { return }
                    let shouldRefreshWorkspace = self?.graphUseCase.shouldRefreshWorkspace(
                        for: signal,
                        turnID: self?.turn.id ?? ""
                    ) == true
                    if signal.kind != .reconcile {
                        self?.applyRealtimeSignal(signal)
                    }
                    if shouldRefreshWorkspace {
                        guard let graphRequest = self?.beginWorkspaceGraphRefresh(
                            workspaceSession: workspaceSession
                        ) else { return }
                        let graphResult = await Self.fetchWorkspaceGraph(graphRequest)
                        guard !Task.isCancelled,
                              self?.applyWorkspaceGraphResult(
                                  graphResult,
                                  request: graphRequest
                              ) == true else { return }
                        if let inspectorRequest = self?.beginSelectedInspectorRefresh() {
                            let outcome = await Self.fetchSelectedInspectorRefresh(inspectorRequest)
                            guard !Task.isCancelled,
                                  self?.applySelectedInspectorRefresh(
                                      outcome,
                                      request: inspectorRequest
                                  ) == true else { return }
                        }
                        guard !Task.isCancelled,
                              self?.workspaceLifecycleState.accepts(workspaceSession) == true else { return }
                        self?.startPollingIfNeeded(workspaceSession: workspaceSession)
                    }
                }
            } catch {
                guard !Task.isCancelled,
                      self?.workspaceLifecycleState.accepts(workspaceSession) == true else { return }
                if self?.errorMessage == nil {
                    self?.errorMessage = "实时进度连接已中断：\(error.localizedDescription)"
                }
            }
            guard !Task.isCancelled,
                  self?.workspaceLifecycleState.accepts(workspaceSession) == true else { return }
            self?.hasActiveRealtimeStream = false
            self?.realtimeTask = nil
            self?.startPollingIfNeeded(workspaceSession: workspaceSession)
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
                guard !Task.isCancelled,
                      self?.acceptsWorkspaceSession(workspaceSession) == true,
                      self?.graphPollingState.accepts(session) == true else { return }
                let isEmptyGraphRetry = self?.shouldRetryEmptyGraph == true
                let interval = MessageTaskGraphPollingState.interval(
                    isEmptyGraphRetry: isEmptyGraphRetry,
                    hasActiveRealtimeStream: self?.hasActiveRealtimeStream == true
                )
                try? await Task.sleep(for: interval)
                guard !Task.isCancelled,
                      self?.acceptsWorkspaceSession(workspaceSession) == true,
                      self?.graphPollingState.accepts(session) == true,
                      let graphRequest = self?.beginWorkspaceGraphRefresh(
                          pollingSession: session,
                          workspaceSession: workspaceSession
                      ) else { return }
                let graphResult = await Self.fetchWorkspaceGraph(graphRequest)
                guard !Task.isCancelled,
                      self?.applyWorkspaceGraphResult(
                          graphResult,
                          request: graphRequest
                      ) == true else { return }
                if let inspectorRequest = self?.beginSelectedInspectorRefresh() {
                    let outcome = await Self.fetchSelectedInspectorRefresh(inspectorRequest)
                    guard !Task.isCancelled,
                          self?.applySelectedInspectorRefresh(
                              outcome,
                              request: inspectorRequest
                          ) == true else { return }
                }
                guard !Task.isCancelled,
                      self?.acceptsWorkspaceSession(workspaceSession) == true,
                      self?.graphPollingState.accepts(session) == true else { return }
                if isEmptyGraphRetry {
                    self?.recordEmptyGraphRetryAttempt()
                }
                let shouldContinue = self?.graphPollingState.shouldPoll(
                    graph: self?.graph,
                    expectsTaskGraph: self?.expectsTaskGraph == true
                ) == true
                if !shouldContinue {
                    if self?.graphPollingState.finish(session) == true {
                        self?.pollingTask = nil
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
    nonisolated var applicationSection: MessageTaskInspectorSection {
        switch self {
        case .detail: .detail
        case .process: .process
        case .run: .run
        }
    }
}
