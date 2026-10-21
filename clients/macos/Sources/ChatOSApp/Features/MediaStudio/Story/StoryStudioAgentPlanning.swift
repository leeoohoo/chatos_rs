import AppKit
import ChatOSCore
import Foundation

private struct StoryHistoryLoadOutcome: Sendable {
    var runs: [StoryPlanningRunSnapshot]
    var batches: [StoryMediaBatch]
    var runCursor: StoryProjectStore.PageCursor?
    var batchCursor: StoryProjectStore.PageCursor?
    var unreadableRuns: Int
    var unreadableBatches: Int
    var recoveredProject: StoryProject?
}

@MainActor
extension StoryStudioViewModel {
    func loadAgentHistory(_ projectID: UUID) {
        guard let owner else { return }
        let token = session
        let store = store
        let canonical = projects.first(where: { $0.id == projectID })
        historyTask?.cancel(); isLoadingAgentRuns = true
        historyTask = Task { [weak self, store] in
            let result: Result<StoryHistoryLoadOutcome, Error>
            do {
                result = .success(try await Self.loadStoryHistory(
                    owner: owner,
                    projectID: projectID,
                    canonical: canonical,
                    store: store
                ))
            } catch {
                result = .failure(error)
            }
            guard let self, self.session == token,
                  self.selectedProjectID == projectID, !Task.isCancelled else { return }
            self.isLoadingAgentRuns = false
            self.historyTask = nil
            switch result {
            case .success(let outcome):
                self.agentRunCursor = outcome.runCursor
                self.mediaBatchCursor = outcome.batchCursor
                self.hasMoreAgentRuns = outcome.runCursor != nil
                self.hasMoreMediaBatches = outcome.batchCursor != nil
                if let recovered = outcome.recoveredProject,
                   let projectIndex = self.projects.firstIndex(where: { $0.id == projectID }) {
                    self.projects[projectIndex] = recovered
                }
                for run in outcome.runs { self.publishAgentRun(run, token: token) }
                for batch in outcome.batches {
                    self.publishMediaBatch(batch, token: token, updateProject: false)
                }
                if outcome.unreadableRuns > 0 {
                    self.errorMessage = "有 \(outcome.unreadableRuns) 条规划运行记录无法读取，原文件已保留。"
                }
                if outcome.unreadableBatches > 0 {
                    self.errorMessage = "有 \(outcome.unreadableBatches) 条制作批次无法读取，原文件已保留。"
                }
            case .failure(let error):
                self.errorMessage = error.localizedDescription
            }
        }
    }

    nonisolated private static func loadStoryHistory(
        owner: String,
        projectID: UUID,
        canonical: StoryProject?,
        store: StoryProjectStore
    ) async throws -> StoryHistoryLoadOutcome {
        let runPage = try await store.loadRuns(owner: owner, projectID: projectID)
        let batchPage = try await store.loadMediaBatches(owner: owner, projectID: projectID)
        try Task.checkCancellation()
        var runs = runPage.runs
        var batches = batchPage.batches
        var recoveredProject: StoryProject?
        if let canonical {
            let canonicalDigest = try StoryAgentRun.digest(canonical)
            if let candidateIndex = runs.firstIndex(where: {
                !$0.applied && $0.abandonedAt == nil
                    && ((try? StoryAgentRun.matchesPersistedDigest(
                        $0.baseDigest,
                        project: canonical
                    )) == true || (try? StoryAgentRun.digest($0.draft)) == canonicalDigest)
            }), let recovered = try await store.applyValidatedRunIfPossible(
                runs[candidateIndex],
                owner: owner
            ) {
                runs[candidateIndex] = recovered.0
                recoveredProject = recovered.1
            }
            for index in batches.indices {
                try Task.checkCancellation()
                if let recovered = try await store.reconcileCompletedVideoBatch(batches[index]) {
                    batches[index] = recovered
                }
            }
        }
        return .init(
            runs: runs.map(\.planningSnapshot),
            batches: batches,
            runCursor: runPage.nextCursor,
            batchCursor: batchPage.nextCursor,
            unreadableRuns: runPage.unreadable,
            unreadableBatches: batchPage.unreadable,
            recoveredProject: recoveredProject
        )
    }

    func loadMoreAgentRuns() {
        guard let owner, let projectID = selectedProjectID, let cursor = agentRunCursor,
              !isLoadingAgentRuns, !isLoadingMoreAgentRuns else { return }
        let token = session
        let store = store
        isLoadingMoreAgentRuns = true
        agentRunPageTask = Task { [weak self, store] in
            let result: Result<([StoryPlanningRunSnapshot], Int, StoryProjectStore.PageCursor?), Error>
            do {
                let page = try await store.loadRuns(
                    owner: owner,
                    projectID: projectID,
                    after: cursor
                )
                result = .success((
                    page.runs.map(\.planningSnapshot),
                    page.unreadable,
                    page.nextCursor
                ))
            } catch {
                result = .failure(error)
            }
            guard let self, self.session == token,
                  self.selectedProjectID == projectID, !Task.isCancelled else { return }
            self.isLoadingMoreAgentRuns = false
            self.agentRunPageTask = nil
            switch result {
            case .success(let page):
                for run in page.0 { self.publishAgentRun(run, token: token) }
                self.agentRunCursor = page.2
                self.hasMoreAgentRuns = page.2 != nil
                if page.1 > 0 {
                    self.errorMessage = "有 \(page.1) 条更早规划运行记录无法读取，原文件已保留。"
                }
            case .failure(let error):
                self.errorMessage = error.localizedDescription
            }
        }
    }

    func loadMoreMediaBatches() {
        guard let owner, let projectID = selectedProjectID, let cursor = mediaBatchCursor,
              !isLoadingAgentRuns, !isLoadingMoreMediaBatches else { return }
        let token = session
        let store = store
        isLoadingMoreMediaBatches = true
        mediaBatchPageTask = Task { [weak self, store] in
            let result: Result<([StoryMediaBatch], Int, StoryProjectStore.PageCursor?), Error>
            do {
                let page = try await store.loadMediaBatches(
                    owner: owner,
                    projectID: projectID,
                    after: cursor
                )
                var batches = page.batches
                for index in batches.indices {
                    try Task.checkCancellation()
                    if let recovered = try await store.reconcileCompletedVideoBatch(batches[index]) {
                        batches[index] = recovered
                    }
                }
                result = .success((batches, page.unreadable, page.nextCursor))
            } catch {
                result = .failure(error)
            }
            guard let self, self.session == token,
                  self.selectedProjectID == projectID, !Task.isCancelled else { return }
            self.isLoadingMoreMediaBatches = false
            self.mediaBatchPageTask = nil
            switch result {
            case .success(let page):
                for batch in page.0 {
                    self.publishMediaBatch(batch, token: token, updateProject: false)
                }
                self.mediaBatchCursor = page.2
                self.hasMoreMediaBatches = page.2 != nil
                if page.1 > 0 {
                    self.errorMessage = "有 \(page.1) 条更早制作批次无法读取，原文件已保留。"
                }
            case .failure(let error):
                self.errorMessage = error.localizedDescription
            }
        }
    }

    func publishAgentRun(_ value: StoryPlanningRunSnapshot, token: UUID) {
        guard session == token, owner == value.ownerUserID else { return }
        if let index = agentRuns.firstIndex(where: { $0.id == value.id }) {
            if agentRuns[index].updatedAt <= value.updatedAt { agentRuns[index] = value }
        } else { agentRuns.append(value) }
        agentRuns.sort { $0.updatedAt > $1.updatedAt }
        if isBusy, activeProjectID == value.projectID, let event = value.events.last { operation = event.detail }
    }

    func startAgent(stage: StoryPlanningStage, targets: [String], userIdeas: String = "") {
        guard let project, let storyPlanning else { return }
        do {
            try validateStoryPlanningAvailability()
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        run("启动分步剧情规划") { [weak self, storyPlanning] owner, token in
            guard let self,
                  let request = self.storyPlanningExecutionState.begin(
                    ownerUserID: owner,
                    accountSessionID: token,
                    kind: .start(projectID: project.id, stage: stage)
                  ) else { throw CancellationError() }
            defer { _ = self.storyPlanningExecutionState.finish(request) }
            do {
                let outcome = try await storyPlanning.execute(
                    ownerUserID: owner,
                    command: .start(
                        project: project,
                        stage: stage,
                        targetIDs: targets,
                        userIdeas: userIdeas
                    ),
                    continuationCheck: { [weak self] in
                        guard let self else { throw CancellationError() }
                        try await self.checkStoryPlanning(request, session: token)
                    },
                    shouldPause: { [weak self] in await self?.shouldPauseAgent(token) ?? true },
                    onEvent: { [weak self] event in
                        await self?.receiveStoryPlanningEvent(
                            event,
                            request: request,
                            token: token
                        )
                    }
                )
                self.applyAgentExecutionOutcome(
                    outcome,
                    request: request,
                    token: token
                )
            } catch {
                self.clearActiveAgentRun(token: token)
                throw error
            }
        }
    }

    func resumeAgent(_ runID: UUID) {
        guard let project, let storyPlanning else { return }
        do {
            try validateStoryPlanningAvailability()
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        run("恢复剧情规划记录") { [weak self, storyPlanning] owner, token in
            guard let self,
                  let request = self.storyPlanningExecutionState.begin(
                    ownerUserID: owner,
                    accountSessionID: token,
                    kind: .resume(projectID: project.id, runID: runID)
                  ) else { throw CancellationError() }
            defer { _ = self.storyPlanningExecutionState.finish(request) }
            self.activeAgentRunID = runID
            do {
                let outcome = try await storyPlanning.execute(
                    ownerUserID: owner,
                    command: .resume(project: project, runID: runID),
                    continuationCheck: { [weak self] in
                        guard let self else { throw CancellationError() }
                        try await self.checkStoryPlanning(request, session: token)
                    },
                    shouldPause: { [weak self] in await self?.shouldPauseAgent(token) ?? true },
                    onEvent: { [weak self] event in
                        await self?.receiveStoryPlanningEvent(
                            event,
                            request: request,
                            token: token
                        )
                    }
                )
                self.applyAgentExecutionOutcome(
                    outcome,
                    request: request,
                    token: token
                )
            } catch {
                self.clearActiveAgentRun(runID, token: token)
                throw error
            }
        }
    }

    func abandonAgent(_ runID: UUID) {
        guard let projectID = selectedProjectID, let storyPlanning else { return }
        run("放弃中断的剧情规划草稿") { [weak self, storyPlanning] owner, token in
            guard let self,
                  let request = self.storyPlanningExecutionState.begin(
                    ownerUserID: owner,
                    accountSessionID: token,
                    kind: .abandon(projectID: projectID, runID: runID)
                  ) else { throw CancellationError() }
            defer { _ = self.storyPlanningExecutionState.finish(request) }
            let outcome = try await storyPlanning.execute(
                ownerUserID: owner,
                command: .abandon(projectID: projectID, runID: runID),
                continuationCheck: { [weak self] in
                    guard let self else { throw CancellationError() }
                    try await self.checkStoryPlanning(request, session: token)
                },
                shouldPause: { false },
                onEvent: { [weak self] event in
                    await self?.receiveStoryPlanningEvent(
                        event,
                        request: request,
                        token: token
                    )
                }
            )
            self.applyAgentExecutionOutcome(
                outcome,
                request: request,
                token: token
            )
        }
    }

    private func applyAgentExecutionOutcome(
        _ outcome: StoryPlanningExecutionOutcome,
        request: StoryPlanningExecutionToken,
        token: UUID
    ) {
        guard storyPlanningExecutionState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: token
        ) else { return }
        if let project = outcome.project {
            if let index = projects.firstIndex(where: { $0.id == project.id }) {
                projects[index] = project
            }
            if selectedSegmentID == nil { selectedSegmentID = project.segments.first?.id }
        }
        publishAgentRun(outcome.run, token: token)
        if let reason = outcome.stopReason { errorMessage = reason }
        clearActiveAgentRun(outcome.run.id, token: token)
    }

    func clearActiveAgentRun(_ id: UUID, token: UUID) {
        guard session == token, activeAgentRunID == id else { return }
        activeAgentRunID = nil
    }

    func clearActiveAgentRun(token: UUID) {
        guard session == token else { return }
        activeAgentRunID = nil
    }

    func shouldPauseAgent(_ token: UUID) -> Bool { session != token || pauseRequested }

    func checkStoryPlanning(
        _ request: StoryPlanningExecutionToken,
        session token: UUID
    ) throws {
        try check(token)
        guard storyPlanningExecutionState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: token
        ) else { throw CancellationError() }
    }

    func receiveStoryPlanningEvent(
        _ event: StoryPlanningExecutionEvent,
        request: StoryPlanningExecutionToken,
        token: UUID
    ) {
        guard storyPlanningExecutionState.accepts(
            request,
            ownerUserID: owner,
            accountSessionID: token
        ) else { return }
        switch event {
        case let .run(snapshot):
            if isBusy, activeProjectID == snapshot.projectID, activeAgentRunID == nil {
                activeAgentRunID = snapshot.id
            }
            publishAgentRun(snapshot, token: token)
        case let .modelStream(stream):
            receiveModelStream(stream, token: token)
        }
    }

    func receiveModelStream(_ event: StoryPlanningModelStreamEvent, token: UUID) {
        guard session == token else { return }
        switch event {
        case .activity:
            break
        case .responseCreated:
            streamingModelText = ""; streamingToolName = nil
            operation = "文本模型正在流式分析…"
        case let .textDelta(delta):
            streamingModelText += delta
            if streamingModelText.count > 12_000 { streamingModelText.removeFirst(streamingModelText.count - 12_000) }
            operation = "文本模型正在流式输出…"
        case let .toolCallDelta(name):
            if let name, !name.isEmpty { streamingToolName = (streamingToolName ?? "") + name }
            operation = "正在组装工具调用：\(streamingToolName ?? "参数")"
        case .completed:
            operation = streamingToolName.map { "已接收完整工具调用：\($0)" } ?? "本轮流式响应已完成"
        }
    }

}
