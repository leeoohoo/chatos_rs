import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentGroupChatScheduler {
    func runClaimedDelivery(
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
                  savedRun.context.lane == delivery.lane,
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
                hopCount: delivery.hopCount,
                lane: delivery.lane
            )
            let scope = LocalAgentGroupChatRun.runtimeScope(for: context)
            let policy = try settings.load().global
            try policy.validate()
            let selectedProfession = try await professionProvider(
                ownerUserID,
                profile.draft.professionKey
            )
            let fallbackProfession = try await professionProvider(
                ownerUserID,
                LocalAgentSkillCatalog.legacyProfessionKey
            )
            let profession = selectedProfession ?? fallbackProfession
                ?? LocalAgentSkillCatalog.profession(
                key: LocalAgentSkillCatalog.legacyProfessionKey
            )!
            let projectType: LocalProjectTypeDefinition?
            if room.conversationKind == .projectTeam {
                let projectTypeKey = try await projectTypeKeyProvider(ownerUserID, projectID)
                    ?? LocalAgentSkillCatalog.legacyProjectTypeKey
                projectType = try await projectTypeProvider(ownerUserID, projectTypeKey)
            } else {
                projectType = nil
            }
            let contextLanguage = await contextLanguageProvider(ownerUserID)
            let communicationSkill = LocalAgentCompactCommunicationSkill.snapshot(
                language: contextLanguage,
                audience: delivery.lane == .manager ? .manager : .executor
            )
            guard let triggerMessage = try await store.message(
                ownerUserID: ownerUserID,
                roomID: room.id,
                messageID: delivery.messageID
            ) else { throw AgentGroupChatError.notFound }
            var triggerAttachments: [ProjectAgentMessageAttachmentPayload] = []
            for attachment in triggerMessage.attachmentItems {
                guard let payload = try await store.messageAttachment(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    messageID: triggerMessage.id,
                    attachmentID: attachment.id
                ) else { throw AgentGroupChatError.storage("message attachment is missing") }
                triggerAttachments.append(payload)
            }
            var initial = AgentRunCheckpoint(
                scope: scope,
                messages: Self.initialMessages(
                    profile: profile,
                    member: member,
                    room: room,
                    delivery: delivery,
                    profession: profession,
                    projectType: projectType,
                    contextLanguage: contextLanguage,
                    communicationSkill: communicationSkill,
                    triggerMessage: triggerMessage,
                    triggerAttachments: triggerAttachments
                )
            )
            initial.id = runID
            initial.instructionBundles = [
                .init(
                    name: communicationSkill.name,
                    version: communicationSkill.version,
                    contentSHA256: communicationSkill.contentSHA256,
                    language: communicationSkill.language.rawValue,
                    audience: communicationSkill.audience.rawValue
                ),
            ]
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
            await service.publishChange(.init(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: profile.id,
                runID: run.id,
                kind: .deliveryClaimed
            ))
        }
        let runID = run.id
        let context = run.context
        let scope = run.checkpoint.scope
        let policy = run.policy
        var checkpoint = run.checkpoint
        let session = LocalAgentGroupChatRunSession(
            run: run,
            store: store,
            now: now,
            didPersist: { [service] saved in
                await service.publishChange(.init(
                    ownerUserID: saved.context.ownerUserID,
                    roomID: saved.context.roomID,
                    agentID: saved.context.agentID,
                    runID: saved.id,
                    kind: .runUpdated
                ))
            }
        )

        let executionTodo: LocalAgentTodo?
        if context.lane == .executor {
            executionTodo = try await store.todoForDelivery(
                ownerUserID: ownerUserID,
                deliveryID: delivery.id
            )
            guard executionTodo?.agentID == profile.id,
                  executionTodo?.teamRoomID == room.id else {
                throw AgentGroupChatError.conflict
            }
        } else {
            executionTodo = nil
        }

        if let todo = executionTodo, savedRun == nil {
            _ = try await store.appendAgentTodoProgress(
                ownerUserID: ownerUserID,
                agentID: profile.id,
                todoID: todo.id,
                kind: .started,
                runID: context.runID,
                stage: "started",
                detail: "Todo 执行线程已启动，并完成团队、项目和能力计划绑定。",
                nowUnixMs: now()
            )
        }

        var memoryProvider: AgentMemoryContextProvider?
        do {
            let memoryScope: AgentMemoryScope
            if let boundScope = checkpoint.memory?.scope {
                memoryScope = boundScope
            } else {
                if let executionTodo {
                    memoryScope = try AgentMemoryScope(
                        tenantID: ownerUserID,
                        todoID: executionTodo.id,
                        runID: runID,
                        runtimeScope: scope
                    )
                } else {
                    memoryScope = try AgentMemoryScope(
                        tenantID: ownerUserID,
                        agentID: profile.id,
                        projectID: projectID,
                        runID: runID,
                        runtimeScope: scope
                    )
                }
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
                    detail: context.lane == .executor
                        ? "已绑定当前 Todo 的独立执行 Memory"
                        : "已绑定当前 Agent 的长期通讯 Memory",
                    modelCalls: checkpoint.modelCalls
                )
            )
        } catch {
            // Continuity is part of the Agent identity contract. Never execute a fresh or resumed
            // delivery without its bound Memory thread: doing so makes one wake-up behave like a
            // new Agent and can produce decisions that contradict earlier work. This technical
            // pause is safe to retry because no model call or side-effect tool has started yet.
            try await session.record(
                checkpoint: checkpoint,
                event: .init(
                    kind: "memory_unavailable",
                    detail: Self.failureDetail(error),
                    modelCalls: checkpoint.modelCalls
                )
            )
            checkpoint.status = .paused
            checkpoint.stopReason = AgentContextError.unavailable.localizedDescription
            _ = try await session.finish(checkpoint: checkpoint)
            return .init(
                deliveryID: delivery.id,
                agentID: delivery.targetAgentID,
                outcome: .suspended,
                detail: "当前 Agent 的连续 Memory 暂时不可用，已安全暂停并等待自动重试。"
            )
        }

        let chatProvider = try await relayMCP.connect(
            context: context,
            professions: try await professionCatalogProvider(ownerUserID),
            todoPluginOptions: context.lane == .manager
                ? try await todoPluginCatalogProvider(ownerUserID)
                : []
        )
        let extraProviders = try await additionalToolProviders(profile, member, context)
        let toolRegistry = try await AgentToolProviderRegistry(
            providers: [chatProvider] + extraProviders
        )
        let model = try await services.makeAgentModel(
            configID: run.modelConfigID,
            policy: policy,
            thinkingLevel: profile.draft.thinkingLevel
        )
        var finalCheckpoint = try await runtime.run(
            checkpoint: checkpoint,
            scope: scope,
            policy: policy,
            model: model,
            tools: toolRegistry.definitions,
            execute: { call in
                if context.lane == .executor {
                    guard let currentDelivery = try await store.delivery(
                        ownerUserID: ownerUserID,
                        deliveryID: delivery.id
                    ), currentDelivery.status == .running,
                    let currentTodo = try await store.todoForDelivery(
                        ownerUserID: ownerUserID,
                        deliveryID: delivery.id
                    ), currentTodo.status == .inProgress,
                    currentTodo.agentID == context.agentID,
                    currentTodo.teamRoomID == context.roomID else {
                        return .failure(
                            "Todo 已被停止或执行身份已经失效；客户端已拒绝本次工具调用。"
                        )
                    }
                }
                return try await toolRegistry.execute(call)
            },
            completionCheck: {
                guard let current = try await store.delivery(
                    ownerUserID: ownerUserID,
                    deliveryID: delivery.id
                ) else { return nil }
                if current.status == .cancelled, context.lane == .executor {
                    return "Todo 已被项目经理停止；执行线程已经结束。"
                }
                guard current.status == .completed else { return nil }
                if current.triggerKind == .todo, current.responseMessageID == nil {
                    return "Todo 已更新并结束本轮执行。"
                }
                if current.lane == .manager, current.responseMessageID == nil {
                    return "Agent 通讯周期已完成。"
                }
                guard let responseMessageID = current.responseMessageID,
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
        let executorWasCancelled = context.lane == .executor
            && currentDelivery?.status == .cancelled
        if finalCheckpoint.status == .completed,
           currentDelivery?.status != .completed,
           !executorWasCancelled {
            finalCheckpoint.status = .failed
            finalCheckpoint.stopReason = switch delivery.triggerKind {
            case .todo: "Agent 未通过本地 Relay MCP 更新 Todo 状态并完成本轮执行。"
            default: "Agent 未通过本地 Relay MCP 的 agent_cycle_complete 完成本轮通讯处理。"
            }
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
                try await failDeliveryAndNotifyManager(
                    store: store,
                    ownerUserID: ownerUserID,
                    delivery: delivery,
                    runID: context.runID,
                    detail: detail,
                    stage: "executor_failed"
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

    func failDeliveryAndNotifyManager(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        delivery: ProjectAgentDelivery,
        runID: String?,
        detail: String,
        stage: String
    ) async throws {
        let timestamp = now()
        let todo = delivery.triggerKind == .todo
            ? try await store.todoForDelivery(
                ownerUserID: ownerUserID,
                deliveryID: delivery.id
            )
            : nil
        _ = try await store.failDelivery(
            ownerUserID: ownerUserID,
            deliveryID: delivery.id,
            error: detail,
            nowUnixMs: timestamp
        )
        guard let todo else { return }
        var currentTodo = try await store.agentTodo(
            ownerUserID: ownerUserID,
            agentID: delivery.targetAgentID,
            todoID: todo.id
        )
        if currentTodo?.status == .inProgress {
            do {
                currentTodo = try await store.updateAgentTodo(
                    ownerUserID: ownerUserID,
                    agentID: delivery.targetAgentID,
                    todoID: todo.id,
                    update: .init(status: .blocked, blockedReason: detail),
                    nowUnixMs: timestamp
                )
            } catch AgentGroupChatError.conflict {
                // A concurrent manager cancellation is authoritative. Re-read the terminal
                // state below and emit only the idempotent status wake-up it still needs.
                currentTodo = try await store.agentTodo(
                    ownerUserID: ownerUserID,
                    agentID: delivery.targetAgentID,
                    todoID: todo.id
                )
            }
        }
        if todo.status == .inProgress, currentTodo?.status == .blocked {
            _ = try await store.appendAgentTodoProgress(
                ownerUserID: ownerUserID,
                agentID: delivery.targetAgentID,
                todoID: todo.id,
                kind: .blocked,
                runID: runID,
                stage: stage,
                detail: detail,
                nowUnixMs: timestamp
            )
        }
        guard let terminalTodo = currentTodo,
              [.blocked, .completed, .cancelled].contains(terminalTodo.status) else { return }
        _ = try await store.enqueueAgentTodoStatus(
            ownerUserID: ownerUserID,
            agentID: delivery.targetAgentID,
            todoID: todo.id,
            excludingAgentID: nil,
            nowUnixMs: timestamp
        )
    }
}
