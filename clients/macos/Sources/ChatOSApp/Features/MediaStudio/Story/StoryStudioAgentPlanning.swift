import AppKit
import ChatOSAgentRuntime
import ChatOSCore
import Foundation

private struct StoryHistoryLoadOutcome: Sendable {
    var runs: [StoryAgentRun]
    var batches: [StoryMediaBatch]
    var runCursor: StoryProjectStore.PageCursor?
    var batchCursor: StoryProjectStore.PageCursor?
    var unreadableRuns: Int
    var unreadableBatches: Int
    var recoveredProject: StoryProject?
}

private struct StoryAgentExecutionOutcome: Sendable {
    var run: StoryAgentRun
    var project: StoryProject?
    var stopReason: String?
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
            runs: runs,
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
            let result: Result<([StoryAgentRun], Int, StoryProjectStore.PageCursor?), Error>
            do {
                let page = try await store.loadRuns(
                    owner: owner,
                    projectID: projectID,
                    after: cursor
                )
                result = .success((page.runs, page.unreadable, page.nextCursor))
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

    func publishAgentRun(_ value: StoryAgentRun, token: UUID) {
        guard session == token, owner == value.owner else { return }
        if let index = agentRuns.firstIndex(where: { $0.id == value.id }) {
            if agentRuns[index].updatedAt <= value.updatedAt { agentRuns[index] = value }
        } else { agentRuns.append(value) }
        agentRuns.sort { $0.updatedAt > $1.updatedAt }
        if isBusy, activeProjectID == value.projectID, let event = value.events.last { operation = event.detail }
    }

    func startAgent(stage: StoryAgentRun.Stage, targets: [String], userIdeas: String = "") {
        guard let project, let owner, let services = agentServices else { return }
        let policy: AgentRunPolicy
        let draft: StoryAgentRun
        do {
            policy = try effectiveAgentPolicy()
            draft = try StoryAgentRun(
                project: project,
                owner: owner,
                stage: stage,
                targetIDs: targets,
                policy: policy,
                userIdeas: userIdeas
            )
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        let store = store
        run("启动分步剧情规划") { [weak self, store, services] owner, token in
            self?.activeAgentRunID = draft.id
            do {
                let outcome = try await Self.executeAgent(
                    draft,
                    resume: false,
                    resumePolicy: policy,
                    owner: owner,
                    store: store,
                    services: services,
                    check: { [weak self] in
                        guard let self else { throw CancellationError() }
                        try await self.check(token)
                    },
                    shouldPause: { [weak self] in await self?.shouldPauseAgent(token) ?? true },
                    publish: { [weak self] snapshot in
                        await self?.publishAgentRun(snapshot, token: token)
                    },
                    stream: { [weak self] event in
                        await self?.receiveModelStream(event, token: token)
                    }
                )
                self?.applyAgentExecutionOutcome(outcome, token: token)
            } catch {
                self?.clearActiveAgentRun(draft.id, token: token)
                throw error
            }
        }
    }

    func resumeAgent(_ runID: UUID) {
        guard let project, let services = agentServices else { return }
        let policy: AgentRunPolicy
        do {
            policy = try effectiveAgentPolicy()
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        let store = store
        run("恢复剧情规划记录") { [weak self, store, services] owner, token in
            let saved = try await store.loadRun(
                owner: owner,
                projectID: project.id,
                runID: runID
            )
            try Task.checkCancellation()
            guard !saved.applied, saved.abandonedAt == nil else { throw StoryAgentError.invalidRun }
            let digest = try StoryAgentRun.digest(project)
            let draftDigest = try StoryAgentRun.digest(saved.draft)
            guard try StoryAgentRun.matchesPersistedDigest(saved.baseDigest, project: project)
                    || (saved.checkpoint.status == .completed && digest == draftDigest) else {
                throw StoryAgentError.projectChanged
            }
            self?.activeAgentRunID = saved.id
            do {
                let outcome = try await Self.executeAgent(
                    saved,
                    resume: true,
                    resumePolicy: policy,
                    owner: owner,
                    store: store,
                    services: services,
                    check: { [weak self] in
                        guard let self else { throw CancellationError() }
                        try await self.check(token)
                    },
                    shouldPause: { [weak self] in await self?.shouldPauseAgent(token) ?? true },
                    publish: { [weak self] snapshot in
                        await self?.publishAgentRun(snapshot, token: token)
                    },
                    stream: { [weak self] event in
                        await self?.receiveModelStream(event, token: token)
                    }
                )
                self?.applyAgentExecutionOutcome(outcome, token: token)
            } catch {
                self?.clearActiveAgentRun(saved.id, token: token)
                throw error
            }
        }
    }

    func abandonAgent(_ runID: UUID) {
        guard let project else { return }
        let store = store
        run("放弃中断的剧情规划草稿") { [weak self, store] owner, token in
            var saved = try await store.loadRun(
                owner: owner,
                projectID: project.id,
                runID: runID
            )
            try Task.checkCancellation()
            guard !saved.applied, saved.abandonedAt == nil else { throw StoryAgentError.invalidRun }
            saved.abandonedAt = Date()
            saved.checkpoint.stopReason = "用户已放弃这份中断草稿；正式项目未被修改。"
            saved.events.append(.init(kind: "abandoned", detail: "用户放弃中断草稿，正式项目保持不变",
                                      modelCalls: saved.checkpoint.modelCalls))
            saved.updatedAt = Date()
            try await store.saveRun(saved, owner: owner)
            try Task.checkCancellation()
            self?.publishAgentRun(saved, token: token)
        }
    }

    nonisolated private static func executeAgent(
        _ initial: StoryAgentRun,
        resume: Bool,
        resumePolicy: AgentRunPolicy,
        owner: String,
        store: StoryProjectStore,
        services: any AgentServiceProviding,
        check: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        publish: @escaping @Sendable (StoryAgentRun) async -> Void,
        stream: @escaping @Sendable (AgentModelStreamEvent) async -> Void
    ) async throws -> StoryAgentExecutionOutcome {
        try await check()
        var saved = initial
        var context: AgentMemoryContextProvider?
        if saved.checkpoint.status != .completed {
            let scope = try AgentMemoryScope(tenantID: owner, profile: "story", projectID: saved.projectID,
                                            runID: saved.id, runtimeScope: saved.checkpoint.scope)
            let memory = try await services.makeAgentMemory(scope: scope)
            try await check()
            let provider = AgentMemoryContextProvider(scope: scope, service: memory)
            saved.checkpoint = try provider.bind(saved.checkpoint)
            context = provider
        }
        try await store.saveRun(saved, owner: owner)
        await publish(saved)
        let coordinator = StoryAgentSession(run: saved, store: store, publish: publish)
        do {
            if resume { saved = try await coordinator.prepareForResume(policy: resumePolicy) }
            if saved.checkpoint.status != .completed {
                let model = try await services.makeAgentModel(configID: saved.draft.models.textModelID, policy: saved.policy)
                try await check()
                let result = try await AgentRuntime().run(checkpoint: saved.checkpoint, scope: saved.checkpoint.scope, policy: saved.policy,
                    model: model, tools: StoryAgentTools.definitions(stage: saved.stage), execute: { call in try await coordinator.execute(call) },
                    completionCheck: { await coordinator.validatedCompletion() },
                    contextProvider: context, shouldPause: shouldPause,
                    onModelStreamEvent: stream,
                    record: { checkpoint, event in
                        try await coordinator.record(checkpoint, event: event)
                    })
                saved = try await coordinator.finish(result)
            }
            try await check()
            if saved.checkpoint.status == .completed {
                let (applied, project) = try await store.applyRun(saved, owner: owner)
                try await check()
                return .init(run: applied, project: project, stopReason: nil)
            }
            return .init(run: saved, project: nil, stopReason: saved.checkpoint.stopReason)
        } catch {
            try await coordinator.abort(error.localizedDescription)
            throw error
        }
    }

    private func applyAgentExecutionOutcome(_ outcome: StoryAgentExecutionOutcome, token: UUID) {
        guard session == token else { return }
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

    func shouldPauseAgent(_ token: UUID) -> Bool { session != token || pauseRequested }

    func receiveModelStream(_ event: AgentModelStreamEvent, token: UUID) {
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
        case let .toolCallDelta(_, _, name, _):
            if let name, !name.isEmpty { streamingToolName = (streamingToolName ?? "") + name }
            operation = "正在组装工具调用：\(streamingToolName ?? "参数")"
        case .completed:
            operation = streamingToolName.map { "已接收完整工具调用：\($0)" } ?? "本轮流式响应已完成"
        }
    }

}
