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

    private struct ClaimedWork: Sendable {
        let order: Int
        let member: ProjectAgentRoomMember
        let delivery: ProjectAgentDelivery
    }

    private struct OrderedRunResult: Sendable {
        let order: Int
        let result: RunResult
    }

    public typealias AdditionalToolProviderFactory = @Sendable (
        _ profile: LocalAgentProfile,
        _ member: ProjectAgentRoomMember,
        _ context: LocalAgentChatRunContext
    ) async throws -> [any AgentToolProvider]
    public typealias ProjectTypeKeyProvider = @Sendable (
        _ ownerUserID: String,
        _ projectID: String
    ) async throws -> String?
    public typealias ProfessionProvider = @Sendable (
        _ ownerUserID: String,
        _ professionKey: String
    ) async throws -> LocalAgentProfessionDefinition?
    public typealias ProjectTypeProvider = @Sendable (
        _ ownerUserID: String,
        _ projectTypeKey: String
    ) async throws -> LocalProjectTypeDefinition?
    public typealias ProfessionCatalogProvider = @Sendable (
        _ ownerUserID: String
    ) async throws -> [LocalAgentProfessionDefinition]

    private let service: NativeAgentGroupChatService
    private let services: any AgentServiceProviding
    private let settings: AgentSettingsStore
    private let runtime: AgentRuntime
    private let limits: AgentGroupChatRoutingLimits
    private let relayMCP: LocalAgentRelayMCPServer
    private let additionalToolProviders: AdditionalToolProviderFactory
    private let projectTypeKeyProvider: ProjectTypeKeyProvider
    private let professionProvider: ProfessionProvider
    private let projectTypeProvider: ProjectTypeProvider
    private let professionCatalogProvider: ProfessionCatalogProvider
    private let now: @Sendable () -> Int64

    public init(
        service: NativeAgentGroupChatService,
        services: any AgentServiceProviding,
        settings: AgentSettingsStore = .init(),
        runtime: AgentRuntime = .init(),
        limits: AgentGroupChatRoutingLimits = .init(),
        projectTypeKeyProvider: @escaping ProjectTypeKeyProvider = { _, _ in nil },
        professionProvider: @escaping ProfessionProvider = { _, key in
            LocalAgentSkillCatalog.profession(key: key)
        },
        projectTypeProvider: @escaping ProjectTypeProvider = { _, key in
            LocalAgentSkillCatalog.projectType(key: key)
        },
        professionCatalogProvider: @escaping ProfessionCatalogProvider = { _ in
            LocalAgentSkillCatalog.professions
        },
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
        self.relayMCP = LocalAgentRelayMCPServer(service: service, limits: limits, now: now)
        self.projectTypeKeyProvider = projectTypeKeyProvider
        self.professionProvider = professionProvider
        self.projectTypeProvider = projectTypeProvider
        self.professionCatalogProvider = professionCatalogProvider
        self.additionalToolProviders = additionalToolProviders
        self.now = now
    }

    /// Drains all currently reachable deliveries in one project. Each round first claims at most
    /// one delivery per Agent, then runs different Agents concurrently. Waiting for the round to
    /// finish before claiming again preserves per-Agent serialization while re-reading the member
    /// queue lets an Agent's `@mention` wake another Agent without server polling.
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

        return try await drain(
            store: store,
            ownerUserID: ownerUserID,
            room: room,
            maximumRuns: maximumRuns
        )
    }

    /// Drains one durable conversation, whether it is a project team or a private chat.
    public func drainConversation(
        ownerUserID: String,
        roomID: String,
        maximumRuns: Int = 32
    ) async throws -> [RunResult] {
        guard maximumRuns > 0 else { return [] }
        let store = try await service.store()
        guard let room = try await store.room(ownerUserID: ownerUserID, roomID: roomID),
              room.status == .active else {
            throw AgentGroupChatError.notFound
        }
        return try await drain(
            store: store,
            ownerUserID: ownerUserID,
            room: room,
            maximumRuns: maximumRuns
        )
    }

    /// Drains all account-owned conversations so a Relay message that opens another private
    /// conversation is delivered without requiring that destination to be visible in the UI.
    public func drainAccount(
        ownerUserID: String,
        maximumRuns: Int = 64
    ) async throws -> [RunResult] {
        guard maximumRuns > 0 else { return [] }
        let store = try await service.store()
        var results: [RunResult] = []
        while results.count < maximumRuns, !Task.isCancelled {
            let teams = try await store.listRooms(ownerUserID: ownerUserID, includeArchived: false)
            let directs = try await store.listDirectConversations(
                ownerUserID: ownerUserID,
                includeArchived: false
            )
            let rooms = teams + directs
            var madeProgress = false
            for room in rooms where results.count < maximumRuns {
                let round = try await drain(
                    store: store,
                    ownerUserID: ownerUserID,
                    room: room,
                    maximumRuns: maximumRuns - results.count
                )
                if !round.isEmpty {
                    madeProgress = true
                    results.append(contentsOf: round)
                }
            }
            if !madeProgress { break }
        }
        return results
    }

    private func drain(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        room: ProjectAgentRoom,
        maximumRuns: Int
    ) async throws -> [RunResult] {
        let projectID = room.projectID

        var results: [RunResult] = []
        while results.count < maximumRuns {
            if Task.isCancelled { break }
            let members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
            let remainingCapacity = maximumRuns - results.count
            var claimedWork: [ClaimedWork] = []
            claimedWork.reserveCapacity(min(remainingCapacity, members.count))
            for member in members where claimedWork.count < remainingCapacity {
                if Task.isCancelled { break }
                guard let delivery = try await store.claimNextDelivery(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    agentID: member.agentID,
                    nowUnixMs: now()
                ) else { continue }
                claimedWork.append(.init(
                    order: claimedWork.count,
                    member: member,
                    delivery: delivery
                ))
            }
            if claimedWork.isEmpty { break }

            let round = try await withThrowingTaskGroup(of: OrderedRunResult.self) { group in
                for work in claimedWork {
                    group.addTask {
                        let result = try await runClaimedDeliveryHandlingFailure(
                            store: store,
                            ownerUserID: ownerUserID,
                            projectID: projectID,
                            room: room,
                            member: work.member,
                            delivery: work.delivery
                        )
                        return .init(order: work.order, result: result)
                    }
                }
                var completed: [OrderedRunResult] = []
                completed.reserveCapacity(claimedWork.count)
                for try await result in group {
                    completed.append(result)
                }
                return completed.sorted { $0.order < $1.order }
            }
            results.append(contentsOf: round.map(\.result))
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

    private func runClaimedDeliveryHandlingFailure(
        store: SQLiteAgentGroupChatStore,
        ownerUserID: String,
        projectID: String,
        room: ProjectAgentRoom,
        member: ProjectAgentRoomMember,
        delivery: ProjectAgentDelivery
    ) async throws -> RunResult {
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
                    triggerAttachments: triggerAttachments
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

        let chatProvider = try await relayMCP.connect(
            context: context,
            professions: try await professionCatalogProvider(ownerUserID)
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
            finalCheckpoint.stopReason = "Agent 未通过本地 Relay MCP 的 chat_send_message 完成当前群聊回复。"
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
        delivery: ProjectAgentDelivery,
        profession: LocalAgentProfessionDefinition,
        projectType: LocalProjectTypeDefinition?,
        triggerAttachments: [ProjectAgentMessageAttachmentPayload]
    ) -> [AgentMessage] {
        let conversationRole: String
        let conversationContext: String
        switch room.conversationKind {
        case .projectTeam:
            conversationRole = "项目团队会话"
            conversationContext = "项目群目标：\(room.draft.goal.isEmpty ? "未单独设置" : room.draft.goal)"
        case .humanAgentDirect:
            conversationRole = "你与 Human 的私聊"
            conversationContext = "这是独立私聊，不绑定项目；不要假定可以读取任何项目文件。"
        case .agentAgentDirect:
            conversationRole = "Agent 之间的私聊"
            conversationContext = "这是独立私聊，不绑定项目；通过 Relay 回复对方，不要假定可以读取任何项目文件。"
        }
        let staffingInstructions = LocalAgentPermission.canManageStaff(
            profile.draft.defaultSkillIDs
        ) ? """

        Human 已明确授予你人员管理权限。确有长期职责缺口时，可调用 agent_propose_member 提交招募草案；需要移出当前团队成员时，可调用 agent_propose_member_removal，并给出事实理由与交接计划。两种动作都只会生成提案，必须等待 Human 确认，不能声称人员变更已经发生。
        """ : ""
        let projectInstructions = LocalAgentPermission.canAccessLocalProjects(
            profile.draft.defaultSkillIDs
        ) ? """

        Human 已明确授予你本地项目与团队创建权限。需要创建团队时调用 team_propose，并从工具 schema 提供的项目单选项中选择已有项目或“新建项目”。真实项目 ID 与本机路径由 ChatOS 内部映射，不会提供给你，也不得猜测或要求用户提供。该工具只生成提案，必须等待 Human 确认。
        """ : ""
        let professionSkill = """
        <skill name="\(profession.chatOSSkillName)" binding="program-owned" key="\(profession.key)">
        这是 ChatOS 根据当前 Agent 持久职业绑定自动注入的完整职业 Skill。模型不得更改、替换或声称选择了其他职业。内容迁移自 Relay；其中对旧 Relay Trigger、company/task 工具和公司组织的引用只表示协作方法与质量门禁，实际通信、身份、任务和权限必须使用本轮 ChatOS Relay MCP 及客户端提供的工具，未提供的旧工具不得调用。

        \(profession.skillMarkdown)
        </skill>
        """
        let projectSkill: String
        if let projectType {
            projectSkill = """

            <skill name="\(projectType.skillName)" binding="program-owned" key="\(projectType.key)">
            这是 ChatOS 根据当前项目持久类型绑定自动注入的完整项目 Rule。模型不得更改项目类型或用其他规则替代。规则中的流程与质量门禁按当前任务范围执行；实际工具和权限以 ChatOS 本轮提供内容为准。

            \(projectType.ruleMarkdown)
            </skill>
            """
        } else {
            projectSkill = ""
        }
        let system = """
        你是\(conversationRole)中的本地 Agent「\(profile.draft.name)」。
        你的角色：\(member.draft.role)
        你的职责：\(member.draft.responsibility.isEmpty ? profile.draft.description : member.draft.responsibility)
        角色指令：\(profile.draft.rolePrompt)
        \(conversationContext)

        你通过 ChatOS 本机唯一的 Relay MCP 协作。聊天记录不是你的私有记忆，也不会整段注入提示词。先调用 relay_bootstrap 获取当前身份、会话参与者、唤醒消息和你的独立未读页；需要继续处理未读时调用 chat_read_unread，需要历史上下文时用稳定消息 ID 游标调用 chat_read_messages。处理完消息后调用 chat_mark_read 推进你自己的已读游标。你可以用 chat_direct_open 和 chat_direct_send 与另一个 Agent 建立私聊。本次提供的其他工具来自用户明确授予的本机权限和 Plugin，可以按职责调用。完成工作后必须单独调用 chat_send_message 回复当前会话；只有该 MCP 工具成功才算完成本次 delivery，成功回复也会确认当前触发消息。不得假冒其他 Agent，也不得自行猜测成员 ID。
        \(LocalAgentCapabilityDiscoverySkill.instructions)
        \(staffingInstructions)
        \(projectInstructions)

        \(professionSkill)
        \(projectSkill)
        """
        let envelope = """
        你收到一个本地会话 delivery：
        - delivery_id: \(delivery.id)
        - trigger_message_id: \(delivery.messageID)
        - root_message_id: \(delivery.rootMessageID)
        - trigger_kind: \(delivery.triggerKind.rawValue)
        - hop_count: \(delivery.hopCount)
        - attachment_count: \(triggerAttachments.count)

        请通过本地 Relay MCP 读取消息并完成回复。
        """
        return [
            .init(role: .system, content: system),
            .init(
                role: .user,
                content: envelope,
                attachments: triggerAttachments.map { payload in
                    AgentMessageAttachment(
                        name: payload.attachment.name,
                        mimeType: payload.attachment.mimeType,
                        kind: AgentMessageAttachment.Kind(
                            rawValue: payload.attachment.kind.rawValue
                        ) ?? .file,
                        localFileURL: payload.localFileURL
                    )
                }
            ),
        ]
    }

    private static func failureDetail(_ error: Error) -> String {
        let value = error.localizedDescription.trimmingCharacters(in: .whitespacesAndNewlines)
        return String((value.isEmpty ? "本地 Agent 运行失败。" : value).prefix(8_000))
    }
}
