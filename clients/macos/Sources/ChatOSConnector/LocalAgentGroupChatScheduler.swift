import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// Consumes the durable room delivery queue entirely in the macOS client. The server is used
/// only through `AgentServiceProviding` for account-owned model credentials and Memory Engine;
/// it never selects an Agent, routes a message, or owns a run checkpoint.
public struct LocalAgentGroupChatScheduler: Sendable {
    public enum RunOutcome: String, Sendable, Equatable {
        case completed
        case suspended
        case failed
    }

    public struct RunResult: Sendable {
        public let deliveryID: String
        public let agentID: String
        public let outcome: RunOutcome
        public let detail: String?
    }

    public typealias AdditionalToolProviderFactory = @Sendable (
        _ profile: LocalAgentProfile,
        _ member: ProjectAgentRoomMember,
        _ context: LocalAgentChatRunContext
    ) async throws -> [any AgentToolProvider]

    private let service: NativeAgentGroupChatService
    private let services: any AgentServiceProviding
    private let settings: AgentSettingsStore
    private let runtime: AgentRuntime
    private let limits: AgentGroupChatRoutingLimits
    private let additionalToolProviders: AdditionalToolProviderFactory
    private let now: @Sendable () -> Int64

    public init(
        service: NativeAgentGroupChatService,
        services: any AgentServiceProviding,
        settings: AgentSettingsStore = .init(),
        runtime: AgentRuntime = .init(),
        limits: AgentGroupChatRoutingLimits = .init(),
        additionalToolProviders: @escaping AdditionalToolProviderFactory = { _, _, _ in [] },
        now: @escaping @Sendable () -> Int64 = {
            Int64(Date().timeIntervalSince1970 * 1_000)
        }
    ) {
        self.service = service
        self.services = services
        self.settings = settings
        self.runtime = runtime
        self.limits = limits
        self.additionalToolProviders = additionalToolProviders
        self.now = now
    }

    /// Drains all currently reachable deliveries in one project. Re-reading the member queue
    /// after each round lets an Agent's `@mention` wake another Agent without server polling.
    public func drainProject(
        ownerUserID: String,
        projectID: String,
        maximumRuns: Int = 32
    ) async throws -> [RunResult] {
        guard maximumRuns > 0 else { return [] }
        let store = try await service.store()
        guard let room = try await store.activeRoom(
            ownerUserID: ownerUserID,
            projectID: projectID
        ) else { throw AgentGroupChatError.notFound }

        var results: [RunResult] = []
        while results.count < maximumRuns {
            if Task.isCancelled { break }
            let members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
            var madeProgress = false
            for member in members where results.count < maximumRuns {
                if Task.isCancelled { break }
                guard let result = try await runNext(
                    store: store,
                    ownerUserID: ownerUserID,
                    projectID: projectID,
                    room: room,
                    member: member
                ) else { continue }
                results.append(result)
                madeProgress = true
            }
            if !madeProgress { break }
        }
        return results
    }

    /// Stops both queued and active work for the current project. The caller should first cancel
    /// its in-process scheduler task so no model or Plugin call remains active while SQLite closes
    /// the durable queue.
    @discardableResult
    public func stopProject(ownerUserID: String, projectID: String) async throws -> Int {
        let store = try await service.store()
        guard let room = try await store.activeRoom(
            ownerUserID: ownerUserID,
            projectID: projectID
        ) else { throw AgentGroupChatError.notFound }
        return try await store.stopOutstandingDeliveries(
            ownerUserID: ownerUserID,
            roomID: room.id,
            reason: "用户已停止当前项目中的全部本地 Agent。",
            nowUnixMs: now()
        )
    }

    /// Explicitly resumes one durable running delivery. This is intentionally not automatic:
    /// a checkpoint may contain an in-flight side-effecting Plugin call, and AgentRuntime must
    /// surface that as `needsReview` instead of replaying it after a crash.
    public func resumeDelivery(
        ownerUserID: String,
        projectID: String,
        deliveryID: String
    ) async throws -> RunResult {
        let store = try await service.store()
        guard let room = try await store.activeRoom(
            ownerUserID: ownerUserID,
            projectID: projectID
        ), let delivery = try await store.delivery(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
        ), delivery.roomID == room.id,
           delivery.status == .running,
           let member = try await store.listMembers(
            ownerUserID: ownerUserID,
            roomID: room.id
           ).first(where: { $0.agentID == delivery.targetAgentID }),
           let savedRun = try await store.run(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
           ) else {
            throw AgentGroupChatError.conflict
        }
        do {
            return try await runClaimedDelivery(
                store: store,
                ownerUserID: ownerUserID,
                projectID: projectID,
                room: room,
                member: member,
                delivery: delivery,
                savedRun: savedRun
            )
        } catch {
            var paused = savedRun
            paused.checkpoint.status = .paused
            paused.checkpoint.stopReason = "恢复失败：\(Self.failureDetail(error))"
            paused.events.append(.init(
                kind: "resume_failed",
                detail: paused.checkpoint.stopReason ?? "恢复失败",
                modelCalls: paused.checkpoint.modelCalls
            ))
            paused.updatedAtUnixMs = max(now(), paused.updatedAtUnixMs)
            try await store.saveRun(paused)
            return .init(
                deliveryID: delivery.id,
                agentID: delivery.targetAgentID,
                outcome: .suspended,
                detail: paused.checkpoint.stopReason
            )
        }
    }

    public func abandonDelivery(
        ownerUserID: String,
        projectID: String,
        deliveryID: String
    ) async throws {
        let store = try await service.store()
        guard let room = try await store.activeRoom(
            ownerUserID: ownerUserID,
            projectID: projectID
        ), let delivery = try await store.delivery(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID
        ), delivery.roomID == room.id,
           delivery.status == .running,
           var run = try await store.run(ownerUserID: ownerUserID, deliveryID: deliveryID) else {
            throw AgentGroupChatError.conflict
        }
        let detail = "用户已结束这个未完成的本地 Agent Run。"
        run.checkpoint.status = .failed
        run.checkpoint.stopReason = detail
        run.events.append(.init(
            kind: "abandoned",
            detail: detail,
            modelCalls: run.checkpoint.modelCalls
        ))
        run.updatedAtUnixMs = max(now(), run.updatedAtUnixMs)
        try await store.saveRun(run)
        _ = try await store.failDelivery(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            error: detail,
            nowUnixMs: now()
        )
    }

    private func runNext(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        projectID: String,
        room: ProjectAgentRoom,
        member: ProjectAgentRoomMember
    ) async throws -> RunResult? {
        guard let delivery = try await store.claimNextDelivery(
            ownerUserID: ownerUserID,
            agentID: member.agentID,
            nowUnixMs: now()
        ) else { return nil }

        do {
            return try await runClaimedDelivery(
                store: store,
                ownerUserID: ownerUserID,
                projectID: projectID,
                room: room,
                member: member,
                delivery: delivery
            )
        } catch {
            let detail = Self.failureDetail(error)
            if var savedRun = try await store.run(
                ownerUserID: ownerUserID,
                deliveryID: delivery.id
            ) {
                savedRun.checkpoint.status = .failed
                savedRun.checkpoint.stopReason = detail
                savedRun.events.append(.init(
                    kind: "stopped",
                    detail: detail,
                    modelCalls: savedRun.checkpoint.modelCalls
                ))
                savedRun.updatedAtUnixMs = max(now(), savedRun.updatedAtUnixMs)
                try await store.saveRun(savedRun)
            }
            if try await store.delivery(ownerUserID: ownerUserID, deliveryID: delivery.id)?.status == .running {
                _ = try await store.failDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: delivery.id,
                    error: detail,
                    nowUnixMs: now()
                )
            }
            return .init(
                deliveryID: delivery.id,
                agentID: delivery.targetAgentID,
                outcome: .failed,
                detail: detail
            )
        }
    }

    private func runClaimedDelivery(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        projectID: String,
        room: ProjectAgentRoom,
        member: ProjectAgentRoomMember,
        delivery: ProjectAgentDelivery,
        savedRun: LocalAgentGroupChatRun? = nil
    ) async throws -> RunResult {
        guard room.ownerUserID == ownerUserID,
              room.projectID == projectID,
              room.id == delivery.roomID,
              member.ownerUserID == ownerUserID,
              member.roomID == room.id,
              member.agentID == delivery.targetAgentID,
              member.status == .active else {
            throw AgentGroupChatError.conflict
        }
        let profiles = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: false)
        guard let profile = profiles.first(where: { $0.id == delivery.targetAgentID }) else {
            throw AgentGroupChatError.notFound
        }

        let run: LocalAgentGroupChatRun
        if let savedRun {
            guard savedRun.context.ownerUserID == ownerUserID,
                  savedRun.context.projectID == projectID,
                  savedRun.context.roomID == room.id,
                  savedRun.context.agentID == profile.id,
                  savedRun.context.deliveryID == delivery.id,
                  savedRun.context.triggerMessageID == delivery.messageID,
                  savedRun.context.rootMessageID == delivery.rootMessageID,
                  savedRun.checkpoint.status != .completed else {
                throw AgentGroupChatError.conflict
            }
            run = savedRun
        } else {
            let runID = UUID()
            let context = try LocalAgentChatRunContext(
                ownerUserID: ownerUserID,
                projectID: projectID,
                roomID: room.id,
                agentID: profile.id,
                deliveryID: delivery.id,
                triggerMessageID: delivery.messageID,
                rootMessageID: delivery.rootMessageID,
                runID: runID.uuidString.lowercased(),
                hopCount: delivery.hopCount
            )
            let scope = LocalAgentGroupChatRun.runtimeScope(for: context)
            let policy = try settings.load().global
            try policy.validate()
            var initial = AgentRunCheckpoint(
                scope: scope,
                messages: Self.initialMessages(
                    profile: profile,
                    member: member,
                    room: room,
                    delivery: delivery
                )
            )
            initial.id = runID
            let createdAt = now()
            run = try LocalAgentGroupChatRun(
                id: runID,
                context: context,
                modelConfigID: profile.draft.modelConfigID,
                policy: policy,
                checkpoint: initial,
                createdAtUnixMs: createdAt,
                updatedAtUnixMs: createdAt
            )
            try await store.saveRun(run)
        }
        let runID = run.id
        let context = run.context
        let scope = run.checkpoint.scope
        let policy = run.policy
        var checkpoint = run.checkpoint
        let session = LocalAgentGroupChatRunSession(run: run, store: store, now: now)

        var memoryProvider: AgentMemoryContextProvider?
        do {
            let memoryScope: AgentMemoryScope
            if let boundScope = checkpoint.memory?.scope {
                memoryScope = boundScope
            } else {
                memoryScope = try AgentMemoryScope(
                    tenantID: ownerUserID,
                    agentID: profile.id,
                    projectID: projectID,
                    runID: runID,
                    runtimeScope: scope
                )
            }
            let memory = try await services.makeAgentMemory(scope: memoryScope)
            // Establish the bound thread before attaching Memory to the checkpoint. A fresh run
            // may continue without Memory when this preflight is offline; a resumed run pauses
            // instead of silently switching or dropping its already-bound Memory scope.
            try await memory.ensureThread()
            let provider = AgentMemoryContextProvider(scope: memoryScope, service: memory)
            if checkpoint.memory == nil {
                checkpoint = try provider.bind(checkpoint)
            }
            checkpoint.memory?.threadCreated = true
            memoryProvider = provider
            try await session.record(
                checkpoint: checkpoint,
                event: .init(
                    kind: savedRun == nil ? "memory_bound" : "memory_reconnected",
                    detail: "已绑定当前 Agent 的独立项目 Memory",
                    modelCalls: checkpoint.modelCalls
                )
            )
        } catch {
            if savedRun != nil, checkpoint.memory != nil {
                checkpoint.status = .paused
                checkpoint.stopReason = "恢复运行前无法连接原有 Memory：\(Self.failureDetail(error))"
                _ = try await session.finish(checkpoint: checkpoint)
                return .init(
                    deliveryID: delivery.id,
                    agentID: delivery.targetAgentID,
                    outcome: .suspended,
                    detail: checkpoint.stopReason
                )
            }
            // A fresh local run remains usable when Memory Engine is temporarily unreachable.
            // The checkpoint records that no remote memory was bound; it never shares another
            // Agent's subject or silently substitutes room transcript as memory.
            try await session.record(
                checkpoint: checkpoint,
                event: .init(
                    kind: "memory_unavailable",
                    detail: Self.failureDetail(error),
                    modelCalls: checkpoint.modelCalls
                )
            )
        }

        let chatProvider = try LocalAgentChatToolProvider(
            store: store,
            context: context,
            limits: limits,
            now: now
        )
        let extraProviders = try await additionalToolProviders(profile, member, context)
        let toolRegistry = try await AgentToolProviderRegistry(
            providers: [chatProvider] + extraProviders
        )
        let model = try await services.makeAgentModel(
            configID: run.modelConfigID,
            policy: policy
        )
        var finalCheckpoint = try await runtime.run(
            checkpoint: checkpoint,
            scope: scope,
            policy: policy,
            model: model,
            tools: toolRegistry.definitions,
            execute: { call in try await toolRegistry.execute(call) },
            completionCheck: {
                guard let current = try await store.delivery(
                    ownerUserID: ownerUserID,
                    deliveryID: delivery.id
                ), current.status == .completed,
                      let responseMessageID = current.responseMessageID,
                      let response = try await store.message(
                        ownerUserID: ownerUserID,
                        roomID: room.id,
                        messageID: responseMessageID
                      ) else { return nil }
                return response.content
            },
            contextProvider: memoryProvider,
            record: { saved, event in
                try await session.record(checkpoint: saved, event: event)
            }
        )

        let currentDelivery = try await store.delivery(
            ownerUserID: ownerUserID,
            deliveryID: delivery.id
        )
        if finalCheckpoint.status == .completed, currentDelivery?.status != .completed {
            finalCheckpoint.status = .failed
            finalCheckpoint.stopReason = "Agent 未通过 chat_send_message 完成当前群聊回复。"
        }
        _ = try await session.finish(checkpoint: finalCheckpoint)

        switch finalCheckpoint.status {
        case .completed:
            return .init(
                deliveryID: delivery.id,
                agentID: delivery.targetAgentID,
                outcome: .completed,
                detail: finalCheckpoint.result
            )
        case .paused, .needsReview, .limitReached:
            return .init(
                deliveryID: delivery.id,
                agentID: delivery.targetAgentID,
                outcome: .suspended,
                detail: finalCheckpoint.stopReason
            )
        case .ready, .running, .failed:
            let detail = finalCheckpoint.stopReason ?? "本地 Agent 运行未完成。"
            if currentDelivery?.status == .running {
                _ = try await store.failDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: delivery.id,
                    error: detail,
                    nowUnixMs: now()
                )
            }
            return .init(
                deliveryID: delivery.id,
                agentID: delivery.targetAgentID,
                outcome: .failed,
                detail: detail
            )
        }
    }

    private static func initialMessages(
        profile: LocalAgentProfile,
        member: ProjectAgentRoomMember,
        room: ProjectAgentRoom,
        delivery: ProjectAgentDelivery
    ) -> [AgentMessage] {
        let system = """
        你是项目群聊中的本地 Agent「\(profile.draft.name)」。
        你的角色：\(member.draft.role)
        你的职责：\(member.draft.responsibility.isEmpty ? profile.draft.description : member.draft.responsibility)
        角色指令：\(profile.draft.rolePrompt)
        项目群目标：\(room.draft.goal.isEmpty ? "未单独设置" : room.draft.goal)

        群聊记录不是你的私有记忆，也不会整段注入提示词。先调用 chat_get_trigger 读取本次消息；需要上下文时再调用 chat_read_messages，需要成员身份时调用 chat_list_members。本次提供的其他工具来自用户为你明确选择的本机 Plugin，可以按职责调用。完成工作后必须单独调用 chat_send_message 回复群聊；只有该工具成功才算完成本次 delivery。不得假冒其他 Agent，也不得自行猜测成员 ID。
        """
        let envelope = """
        你收到一个本地群聊 delivery：
        - delivery_id: \(delivery.id)
        - trigger_message_id: \(delivery.messageID)
        - root_message_id: \(delivery.rootMessageID)
        - trigger_kind: \(delivery.triggerKind.rawValue)
        - hop_count: \(delivery.hopCount)

        请使用群聊工具读取消息并完成回复。
        """
        return [
            .init(role: .system, content: system),
            .init(role: .user, content: envelope),
        ]
    }

    private static func failureDetail(_ error: Error) -> String {
        let value = error.localizedDescription.trimmingCharacters(in: .whitespacesAndNewlines)
        return String((value.isEmpty ? "本地 Agent 运行失败。" : value).prefix(8_000))
    }
}
