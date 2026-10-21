import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// macOS adapter for the shared Story planning application Port.
/// Concrete Agent checkpoints, tool receipts, memory scopes, and model clients never
/// cross into the ViewModel or the platform-neutral presentation snapshot.
struct StoryPlanningAgentAdapter: StoryPlanningExecuting, Sendable {
    let store: StoryProjectStore
    let services: any AgentServiceProviding
    let settings: AgentSettingsStore

    func validateStoryPlanningAvailability() throws {
        _ = try settings.load().effective(.story)
    }

    func executeStoryPlanning(
        ownerUserID: String,
        command: StoryPlanningCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        onEvent: @escaping @Sendable (StoryPlanningExecutionEvent) async -> Void
    ) async throws -> StoryPlanningExecutionOutcome {
        switch command {
        case let .start(project, stage, targetIDs, userIdeas):
            let policy = try settings.load().effective(.story)
            let run = try StoryAgentRun(
                project: project,
                owner: ownerUserID,
                stage: stage == .outline ? .outline : .refine,
                targetIDs: targetIDs,
                policy: policy,
                userIdeas: userIdeas
            )
            return try await execute(
                run,
                resume: false,
                resumePolicy: policy,
                ownerUserID: ownerUserID,
                continuationCheck: continuationCheck,
                shouldPause: shouldPause,
                onEvent: onEvent
            )

        case let .resume(project, runID):
            let policy = try settings.load().effective(.story)
            let saved = try await store.loadRun(
                owner: ownerUserID,
                projectID: project.id,
                runID: runID
            )
            try await continuationCheck()
            guard !saved.applied, saved.abandonedAt == nil else {
                throw StoryAgentError.invalidRun
            }
            let projectFingerprint = try StoryProjectFingerprint.current(project)
            let draftFingerprint = try StoryProjectFingerprint.current(saved.draft)
            guard try StoryProjectFingerprint.matches(
                saved.baseDigest,
                project: project
            ) || (saved.checkpoint.status == .completed
                && projectFingerprint == draftFingerprint) else {
                throw StoryAgentError.projectChanged
            }
            return try await execute(
                saved,
                resume: true,
                resumePolicy: policy,
                ownerUserID: ownerUserID,
                continuationCheck: continuationCheck,
                shouldPause: shouldPause,
                onEvent: onEvent
            )

        case let .abandon(projectID, runID):
            var saved = try await store.loadRun(
                owner: ownerUserID,
                projectID: projectID,
                runID: runID
            )
            try await continuationCheck()
            guard !saved.applied, saved.abandonedAt == nil else {
                throw StoryAgentError.invalidRun
            }
            saved.abandonedAt = Date()
            saved.checkpoint.stopReason = "用户已放弃这份中断草稿；正式项目未被修改。"
            saved.events.append(.init(
                kind: "abandoned",
                detail: "用户放弃中断草稿，正式项目保持不变",
                modelCalls: saved.checkpoint.modelCalls
            ))
            saved.updatedAt = Date()
            try await store.saveRun(saved, owner: ownerUserID)
            try await continuationCheck()
            let snapshot = saved.planningSnapshot
            await onEvent(.run(snapshot))
            return .init(run: snapshot)
        }
    }

    private func execute(
        _ initial: StoryAgentRun,
        resume: Bool,
        resumePolicy: AgentRunPolicy,
        ownerUserID: String,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        onEvent: @escaping @Sendable (StoryPlanningExecutionEvent) async -> Void
    ) async throws -> StoryPlanningExecutionOutcome {
        try await continuationCheck()
        var saved = initial
        var context: AgentMemoryContextProvider?
        if saved.checkpoint.status != .completed {
            let scope = try AgentMemoryScope(
                tenantID: ownerUserID,
                profile: "story",
                projectID: saved.projectID,
                runID: saved.id,
                runtimeScope: saved.checkpoint.scope
            )
            let memory = try await services.makeAgentMemory(scope: scope)
            try await continuationCheck()
            let provider = AgentMemoryContextProvider(scope: scope, service: memory)
            saved.checkpoint = try provider.bind(saved.checkpoint)
            context = provider
        }
        try await store.saveRun(saved, owner: ownerUserID)
        await onEvent(.run(saved.planningSnapshot))
        let coordinator = StoryAgentSession(
            run: saved,
            store: store,
            publish: { run in await onEvent(.run(run.planningSnapshot)) }
        )
        do {
            if resume {
                saved = try await coordinator.prepareForResume(policy: resumePolicy)
            }
            if saved.checkpoint.status != .completed {
                let model = try await services.makeAgentModel(
                    configID: saved.draft.models.textModelID,
                    policy: saved.policy
                )
                try await continuationCheck()
                let result = try await AgentRuntime().run(
                    checkpoint: saved.checkpoint,
                    scope: saved.checkpoint.scope,
                    policy: saved.policy,
                    model: model,
                    tools: StoryAgentTools.definitions(stage: saved.stage),
                    execute: { call in try await coordinator.execute(call) },
                    completionCheck: { await coordinator.validatedCompletion() },
                    contextProvider: context,
                    shouldPause: shouldPause,
                    onModelStreamEvent: { event in
                        await onEvent(.modelStream(Self.project(event)))
                    },
                    record: { checkpoint, event in
                        try await coordinator.record(checkpoint, event: event)
                    }
                )
                saved = try await coordinator.finish(result)
            }
            try await continuationCheck()
            if saved.checkpoint.status == .completed {
                let (applied, project) = try await store.applyRun(
                    saved,
                    owner: ownerUserID
                )
                try await continuationCheck()
                return .init(run: applied.planningSnapshot, project: project)
            }
            return .init(
                run: saved.planningSnapshot,
                stopReason: saved.checkpoint.stopReason
            )
        } catch {
            try await coordinator.abort(error.localizedDescription)
            throw error
        }
    }

    private static func project(
        _ event: AgentModelStreamEvent
    ) -> StoryPlanningModelStreamEvent {
        switch event {
        case let .activity(bytes):
            .activity(bytes: bytes)
        case .responseCreated:
            .responseCreated
        case let .textDelta(delta):
            .textDelta(delta)
        case let .toolCallDelta(_, _, name, _):
            .toolCallDelta(name: name)
        case .completed:
            .completed
        }
    }
}
