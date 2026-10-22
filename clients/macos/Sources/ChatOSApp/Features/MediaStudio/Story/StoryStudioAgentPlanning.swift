import AppKit
import ChatOSCore
import Foundation

@MainActor
extension StoryStudioViewModel {
    func loadAgentHistory(_ projectID: UUID) {
        guard let owner else { return }
        let token = session
        let request = storyHistoryLoadState.begin(
            ownerUserID: owner,
            accountSessionID: token,
            kind: .initial(projectID: projectID)
        )
        let storyHistory = storyHistory
        let canonical = projects.first(where: { $0.id == projectID })
        historyTask?.cancel(); isLoadingAgentRuns = true
        historyTask = Task { [weak self, storyHistory] in
            let result: Result<StoryHistoryInitialPage, Error>
            do {
                result = .success(try await storyHistory.loadInitial(
                    ownerUserID: owner,
                    projectID: projectID,
                    currentProject: canonical
                ))
            } catch {
                result = .failure(error)
            }
            guard let self,
                  self.storyHistoryLoadState.accepts(
                    request,
                    ownerUserID: self.owner,
                    accountSessionID: self.session,
                    projectID: self.selectedProjectID
                  ), !Task.isCancelled else { return }
            _ = self.storyHistoryLoadState.finish(request)
            self.isLoadingAgentRuns = false
            self.historyTask = nil
            switch result {
            case .success(let outcome):
                self.agentRunCursor = outcome.planningRuns.nextCursor
                self.mediaBatchCursor = outcome.mediaBatches.nextCursor
                self.hasMoreAgentRuns = outcome.planningRuns.nextCursor != nil
                self.hasMoreMediaBatches = outcome.mediaBatches.nextCursor != nil
                if let recovered = outcome.planningRuns.recoveredProject,
                   let projectIndex = self.projects.firstIndex(where: { $0.id == projectID }) {
                    self.projects[projectIndex] = recovered
                }
                for run in outcome.planningRuns.runs {
                    self.publishAgentRun(run, token: token)
                }
                for batch in outcome.mediaBatches.batches {
                    self.publishMediaBatch(batch, token: token)
                }
                if outcome.planningRuns.unreadableCount > 0 {
                    self.errorMessage = "有 \(outcome.planningRuns.unreadableCount) 条规划运行记录无法读取，原文件已保留。"
                }
                if outcome.mediaBatches.unreadableCount > 0 {
                    self.errorMessage = "有 \(outcome.mediaBatches.unreadableCount) 条制作批次无法读取，原文件已保留。"
                }
            case .failure(let error):
                self.errorMessage = error.localizedDescription
            }
        }
    }

    func loadMoreAgentRuns() {
        guard let owner, let projectID = selectedProjectID, let cursor = agentRunCursor,
              !isLoadingAgentRuns, !isLoadingMoreAgentRuns else { return }
        let token = session
        let request = storyHistoryLoadState.begin(
            ownerUserID: owner,
            accountSessionID: token,
            kind: .planningRuns(projectID: projectID, cursor: cursor)
        )
        let storyHistory = storyHistory
        isLoadingMoreAgentRuns = true
        agentRunPageTask = Task { [weak self, storyHistory] in
            let result: Result<StoryPlanningRunHistoryPage, Error>
            do {
                result = .success(try await storyHistory.loadMorePlanningRuns(
                    ownerUserID: owner,
                    projectID: projectID,
                    after: cursor
                ))
            } catch {
                result = .failure(error)
            }
            guard let self,
                  self.storyHistoryLoadState.accepts(
                    request,
                    ownerUserID: self.owner,
                    accountSessionID: self.session,
                    projectID: self.selectedProjectID
                  ), !Task.isCancelled else { return }
            _ = self.storyHistoryLoadState.finish(request)
            self.isLoadingMoreAgentRuns = false
            self.agentRunPageTask = nil
            switch result {
            case .success(let page):
                for run in page.runs { self.publishAgentRun(run, token: token) }
                self.agentRunCursor = page.nextCursor
                self.hasMoreAgentRuns = page.nextCursor != nil
                if page.unreadableCount > 0 {
                    self.errorMessage = "有 \(page.unreadableCount) 条更早规划运行记录无法读取，原文件已保留。"
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
        let request = storyHistoryLoadState.begin(
            ownerUserID: owner,
            accountSessionID: token,
            kind: .mediaBatches(projectID: projectID, cursor: cursor)
        )
        let storyHistory = storyHistory
        isLoadingMoreMediaBatches = true
        mediaBatchPageTask = Task { [weak self, storyHistory] in
            let result: Result<StoryMediaBatchHistoryPage, Error>
            do {
                result = .success(try await storyHistory.loadMoreMediaBatches(
                    ownerUserID: owner,
                    projectID: projectID,
                    after: cursor
                ))
            } catch {
                result = .failure(error)
            }
            guard let self,
                  self.storyHistoryLoadState.accepts(
                    request,
                    ownerUserID: self.owner,
                    accountSessionID: self.session,
                    projectID: self.selectedProjectID
                  ), !Task.isCancelled else { return }
            _ = self.storyHistoryLoadState.finish(request)
            self.isLoadingMoreMediaBatches = false
            self.mediaBatchPageTask = nil
            switch result {
            case .success(let page):
                for batch in page.batches {
                    self.publishMediaBatch(batch, token: token)
                }
                self.mediaBatchCursor = page.nextCursor
                self.hasMoreMediaBatches = page.nextCursor != nil
                if page.unreadableCount > 0 {
                    self.errorMessage = "有 \(page.unreadableCount) 条更早制作批次无法读取，原文件已保留。"
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
            planningActivity.beginResponse()
            operation = "文本模型正在流式分析…"
        case let .textDelta(delta):
            planningActivity.appendModelText(delta)
            if operation != "文本模型正在流式输出…" {
                operation = "文本模型正在流式输出…"
            }
        case let .toolCallDelta(name):
            planningActivity.appendToolName(name)
            if operation != "正在组装工具调用…" {
                operation = "正在组装工具调用…"
            }
        case .completed:
            operation = streamingToolName.map { "已接收完整工具调用：\($0)" } ?? "本轮流式响应已完成"
        }
    }

}
