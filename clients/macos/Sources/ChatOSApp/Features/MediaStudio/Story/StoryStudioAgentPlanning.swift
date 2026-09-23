import AppKit
import ChatOSAgentRuntime
import ChatOSCore
import Foundation

@MainActor
extension StoryStudioViewModel {
    func loadAgentHistory(_ projectID: UUID) {
        guard let owner else { return }
        let token = session
        historyTask?.cancel(); isLoadingAgentRuns = true
        historyTask = Task {
            defer { if session == token, selectedProjectID == projectID { isLoadingAgentRuns = false } }
            do {
                let result = try await store.loadRuns(owner: owner, projectID: projectID)
                let batches = try await store.loadMediaBatches(owner: owner, projectID: projectID)
                guard session == token, selectedProjectID == projectID, !Task.isCancelled else { return }
                var runs = result.runs
                var loadedBatches = batches.batches
                if let canonical = projects.first(where: { $0.id == projectID }) {
                    let canonicalDigest = try StoryAgentRun.digest(canonical)
                    if let candidateIndex = runs.firstIndex(where: {
                        !$0.applied && $0.abandonedAt == nil
                            && ((try? StoryAgentRun.matchesPersistedDigest($0.baseDigest, project: canonical)) == true
                                || (try? StoryAgentRun.digest($0.draft)) == canonicalDigest)
                    }), let recovered = try await store.applyValidatedRunIfPossible(runs[candidateIndex], owner: owner) {
                        runs[candidateIndex] = recovered.0
                        if let projectIndex = projects.firstIndex(where: { $0.id == projectID }) {
                            projects[projectIndex] = recovered.1
                        }
                    }
                    for index in loadedBatches.indices {
                        if let recovered = try await store.reconcileCompletedVideoBatch(loadedBatches[index]) {
                            guard session == token, selectedProjectID == projectID, !Task.isCancelled else { return }
                            loadedBatches[index] = recovered
                        }
                    }
                }
                for run in runs { publishAgentRun(run, token: token) }
                for batch in loadedBatches { publishMediaBatch(batch, token: token, updateProject: false) }
                if result.unreadable > 0 { errorMessage = "有 \(result.unreadable) 条规划运行记录无法读取，原文件已保留。" }
                if batches.unreadable > 0 { errorMessage = "有 \(batches.unreadable) 条制作批次无法读取，原文件已保留。" }
            } catch { if session == token { errorMessage = error.localizedDescription } }
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
        guard let project else { return }
        run("启动分步剧情规划") { owner, token in
            let draft = try StoryAgentRun(project: project, owner: owner, stage: stage, targetIDs: targets,
                                           policy: self.effectiveAgentPolicy(), userIdeas: userIdeas)
            try await self.executeAgent(draft, resume: false, owner: owner, token: token)
        }
    }

    func resumeAgent(_ runID: UUID) {
        guard let project else { return }
        run("恢复剧情规划记录") { owner, token in
            let history = try await self.store.loadRuns(owner: owner, projectID: project.id)
            try self.check(token)
            guard let saved = history.runs.first(where: { $0.id == runID }),
                  !saved.applied, saved.abandonedAt == nil else { throw StoryAgentError.invalidRun }
            let digest = try StoryAgentRun.digest(project)
            let draftDigest = try StoryAgentRun.digest(saved.draft)
            guard try StoryAgentRun.matchesPersistedDigest(saved.baseDigest, project: project)
                    || (saved.checkpoint.status == .completed && digest == draftDigest) else {
                throw StoryAgentError.projectChanged
            }
            try await self.executeAgent(saved, resume: true, owner: owner, token: token)
        }
    }

    func abandonAgent(_ runID: UUID) {
        guard let project else { return }
        run("放弃中断的剧情规划草稿") { owner, token in
            let history = try await self.store.loadRuns(owner: owner, projectID: project.id)
            try self.check(token)
            guard var saved = history.runs.first(where: { $0.id == runID }),
                  !saved.applied, saved.abandonedAt == nil else { throw StoryAgentError.invalidRun }
            saved.abandonedAt = Date()
            saved.checkpoint.stopReason = "用户已放弃这份中断草稿；正式项目未被修改。"
            saved.events.append(.init(kind: "abandoned", detail: "用户放弃中断草稿，正式项目保持不变",
                                      modelCalls: saved.checkpoint.modelCalls))
            saved.updatedAt = Date()
            try await self.store.saveRun(saved, owner: owner)
            try self.check(token)
            self.publishAgentRun(saved, token: token)
        }
    }

    func executeAgent(_ initial: StoryAgentRun, resume: Bool, owner: String, token: UUID) async throws {
        guard let services = agentServices else { throw StoryAgentError.unavailable }
        try check(token)
        var saved = initial
        activeAgentRunID = saved.id
        defer {
            if session == token, activeAgentRunID == initial.id { activeAgentRunID = nil }
        }
        var context: AgentMemoryContextProvider?
        if saved.checkpoint.status != .completed {
            let scope = try AgentMemoryScope(tenantID: owner, profile: "story", projectID: saved.projectID,
                                            runID: saved.id, runtimeScope: saved.checkpoint.scope)
            let memory = try await services.makeAgentMemory(scope: scope)
            try check(token)
            let provider = AgentMemoryContextProvider(scope: scope, service: memory)
            saved.checkpoint = try provider.bind(saved.checkpoint)
            context = provider
        }
        try await store.saveRun(saved, owner: owner)
        publishAgentRun(saved, token: token)
        let coordinator = StoryAgentSession(run: saved, store: store) { [weak self] snapshot in
            await self?.publishAgentRun(snapshot, token: token)
        }
        do {
            if resume { saved = try await coordinator.prepareForResume(policy: effectiveAgentPolicy()) }
            if saved.checkpoint.status != .completed {
                let model = try await services.makeAgentModel(configID: saved.draft.models.textModelID, policy: saved.policy)
                try check(token)
                let result = try await AgentRuntime().run(checkpoint: saved.checkpoint, scope: saved.checkpoint.scope, policy: saved.policy,
                    model: model, tools: StoryAgentTools.definitions(stage: saved.stage), execute: { call in try await coordinator.execute(call) },
                    completionCheck: { await coordinator.validatedCompletion() },
                    contextProvider: context, shouldPause: { [weak self] in
                        await self?.shouldPauseAgent(token) ?? true
                    }, onModelStreamEvent: { [weak self] event in
                        await self?.receiveModelStream(event, token: token)
                    }, record: { checkpoint, event in try await coordinator.record(checkpoint, event: event) })
                saved = try await coordinator.finish(result)
            }
            try check(token)
            if saved.checkpoint.status == .completed {
                let (applied, project) = try await store.applyRun(saved, owner: owner)
                try check(token)
                if let index = projects.firstIndex(where: { $0.id == project.id }) { projects[index] = project }
                publishAgentRun(applied, token: token)
                if selectedSegmentID == nil { selectedSegmentID = project.segments.first?.id }
            } else if let reason = saved.checkpoint.stopReason { errorMessage = reason }
        } catch {
            try await coordinator.abort(error.localizedDescription)
            throw error
        }
    }

    func shouldPauseAgent(_ token: UUID) -> Bool { session != token || pauseRequested }

    func receiveModelStream(_ event: AgentModelStreamEvent, token: UUID) {
        guard session == token else { return }
        switch event {
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
