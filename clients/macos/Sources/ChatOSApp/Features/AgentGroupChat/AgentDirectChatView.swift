import ChatOSConnector
import ChatOSCore
import SwiftUI

private enum AgentDirectTimelineItem: Identifiable {
    case message(ProjectAgentMessage)
    case agentProposal(LocalAgentCreationProposal)
    case teamProposal(LocalAgentTeamCreationProposal)
    case membershipProposal(LocalAgentMembershipProposal)

    var id: String {
        switch self {
        case let .message(value): "message:\(value.id)"
        case let .agentProposal(value): "agent-proposal:\(value.id)"
        case let .teamProposal(value): "team-proposal:\(value.id)"
        case let .membershipProposal(value): "membership-proposal:\(value.id)"
        }
    }

    var createdAtUnixMs: Int64 {
        switch self {
        case let .message(value): value.createdAtUnixMs
        case let .agentProposal(value): value.createdAtUnixMs
        case let .teamProposal(value): value.createdAtUnixMs
        case let .membershipProposal(value): value.createdAtUnixMs
        }
    }
}

@MainActor
private final class AgentDirectChatViewModel: ObservableObject {
    @Published private(set) var conversation: ProjectAgentRoom?
    @Published private(set) var agents: [LocalAgentProfile] = []
    @Published private(set) var messages: [ProjectAgentMessage] = []
    @Published private(set) var pendingAgentProposals: [LocalAgentCreationProposal] = []
    @Published private(set) var pendingTeamProposals: [LocalAgentTeamCreationProposal] = []
    @Published private(set) var pendingMembershipProposals: [LocalAgentMembershipProposal] = []
    @Published private(set) var teams: [ProjectAgentRoom] = []
    @Published var draftMessage = ""
    @Published var attachments: [ConversationAttachmentDraft] = []
    @Published var attachmentError: String?
    @Published private(set) var attachmentDataByID: [String: Data] = [:]
    @Published private(set) var isLoading = false
    @Published private(set) var hasCompletedInitialLoad = false
    @Published private(set) var isLoadingOlderMessages = false
    @Published private(set) var hasOlderMessages = false
    @Published private(set) var isSending = false
    @Published private(set) var isRunningAgents = false
    @Published private(set) var isInitialTimelineReady = false
    @Published private(set) var proposalActionIDs: Set<String> = []
    @Published var errorMessage: String?
    @Published private(set) var scrollToLatestRequest = 0

    private let ownerUserID: String
    private let conversationID: String
    private let service: NativeAgentGroupChatService
    private let scheduler: LocalAgentGroupChatScheduler
    private let builderService: LocalAgentBuilderService
    private let projectsService: NativeLocalProjectsService
    private var openedStore: SQLiteAgentGroupChatStore?
    private var schedulerTask: Task<Void, Never>?
    private var changeObservationTask: Task<Void, Never>?
    private var supplementaryLoadTask: Task<Void, Never>?
    private let messagePageSize = 20

    init(
        ownerUserID: String,
        conversationID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler,
        builderService: LocalAgentBuilderService,
        projectsService: NativeLocalProjectsService
    ) {
        self.ownerUserID = ownerUserID
        self.conversationID = conversationID
        self.service = service
        self.scheduler = scheduler
        self.builderService = builderService
        self.projectsService = projectsService
    }

    deinit {
        changeObservationTask?.cancel()
        supplementaryLoadTask?.cancel()
    }

    var profilesByID: [String: LocalAgentProfile] {
        Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0) })
    }

    var teamsByID: [String: ProjectAgentRoom] {
        Dictionary(uniqueKeysWithValues: teams.map { ($0.id, $0) })
    }

    var title: String { conversation?.draft.name ?? "私聊" }

    var isHumanDirect: Bool { conversation?.conversationKind == .humanAgentDirect }

    var timelineItems: [AgentDirectTimelineItem] {
        let items = messages.map(AgentDirectTimelineItem.message)
            + pendingAgentProposals.map(AgentDirectTimelineItem.agentProposal)
            + pendingTeamProposals.map(AgentDirectTimelineItem.teamProposal)
            + pendingMembershipProposals.map(AgentDirectTimelineItem.membershipProposal)
        return items.sorted {
            ($0.createdAtUnixMs, $0.id) < ($1.createdAtUnixMs, $1.id)
        }
    }

    func displayName(for message: ProjectAgentMessage) -> String {
        switch message.senderKind {
        case .human: "你"
        case .system: "系统"
        case .agent: profilesByID[message.senderID]?.draft.name ?? "Agent"
        }
    }

    func activate() async {
        startChangeObservation()
        await load()
        startScheduler()
    }

    private func startChangeObservation() {
        guard changeObservationTask == nil else { return }
        let service = service
        let ownerUserID = ownerUserID
        let conversationID = conversationID
        changeObservationTask = Task { [weak self] in
            let changes = await service.changes(
                ownerUserID: ownerUserID,
                roomID: conversationID
            )
            let refreshCoalescer = AgentChangeRefreshCoalescer { [weak self] in
                await self?.load()
            }
            defer { refreshCoalescer.cancel() }
            for await _ in changes {
                guard !Task.isCancelled else { break }
                refreshCoalescer.signal()
            }
        }
    }

    func load() async {
        guard !isLoading else { return }
        isLoading = true
        do {
            let store = try await resolveStore()
            async let loadedConversation = store.room(
                ownerUserID: ownerUserID,
                roomID: conversationID
            )
            async let loadedAgents = store.listAgents(
                ownerUserID: ownerUserID,
                includeArchived: false
            )
            async let loadedMessages = store.pageRecentMessages(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                beforeMessageID: nil,
                limit: messagePageSize
            )
            guard let conversation = try await loadedConversation,
                  conversation.conversationKind.isDirect else {
                throw AgentGroupChatError.notFound
            }
            let messagePage = try await loadedMessages
            self.conversation = conversation
            agents = try await loadedAgents
            let wasEmpty = messages.isEmpty
            messages = mergeMessages(messages, with: messagePage.messages)
            if wasEmpty {
                hasOlderMessages = messagePage.hasMore
            }
            isLoading = false
            hasCompletedInitialLoad = true
            startSupplementaryLoad(
                conversation: conversation,
                messages: messagePage.messages,
                store: store
            )
        } catch {
            isLoading = false
            hasCompletedInitialLoad = true
            errorMessage = error.localizedDescription
        }
    }

    private func startSupplementaryLoad(
        conversation: ProjectAgentRoom,
        messages: [ProjectAgentMessage],
        store: SQLiteAgentGroupChatStore
    ) {
        supplementaryLoadTask?.cancel()
        let loadedAttachmentIDs = Set(attachmentDataByID.keys)
        supplementaryLoadTask = Task { [weak self] in
            guard let self else { return }
            do {
                let loadedAttachmentData = try await loadAttachmentData(
                    messages: messages,
                    excluding: loadedAttachmentIDs,
                    store: store
                )
                guard !Task.isCancelled else { return }
                attachmentDataByID.merge(loadedAttachmentData) { _, new in new }

                guard conversation.conversationKind == .humanAgentDirect else {
                    pendingTeamProposals = []
                    pendingAgentProposals = []
                    pendingMembershipProposals = []
                    teams = []
                    isInitialTimelineReady = true
                    return
                }
                async let loadedProposals = store.listTeamProposals(
                    ownerUserID: ownerUserID,
                    sourceRoomID: conversationID,
                    status: .pending
                )
                async let loadedAgentProposals = store.listAgentProposals(
                    ownerUserID: ownerUserID,
                    roomID: conversationID,
                    status: .pending
                )
                async let loadedMembershipProposals = store.listMembershipProposals(
                    ownerUserID: ownerUserID,
                    sourceRoomID: conversationID,
                    status: .pending
                )
                async let loadedTeams = store.listRooms(
                    ownerUserID: ownerUserID,
                    includeArchived: false
                )
                let teamProposals = try await loadedProposals
                let agentProposals = try await loadedAgentProposals
                let membershipProposals = try await loadedMembershipProposals
                let rooms = try await loadedTeams
                guard !Task.isCancelled else { return }
                pendingTeamProposals = teamProposals
                pendingAgentProposals = agentProposals
                pendingMembershipProposals = membershipProposals
                teams = rooms
                isInitialTimelineReady = true
            } catch {
                guard !Task.isCancelled else { return }
                errorMessage = error.localizedDescription
                // Messages are already available. A supplementary-card failure must not leave
                // the transcript waiting forever before it performs its initial positioning.
                isInitialTimelineReady = true
            }
        }
    }

    @discardableResult
    func loadOlderMessages() async -> String? {
        guard !isLoadingOlderMessages,
              hasOlderMessages,
              let firstMessageID = messages.first?.id else { return nil }
        isLoadingOlderMessages = true
        defer { isLoadingOlderMessages = false }
        do {
            let store = try await resolveStore()
            let page = try await store.pageRecentMessages(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                beforeMessageID: firstMessageID,
                limit: messagePageSize
            )
            messages = mergeMessages(messages, with: page.messages)
            hasOlderMessages = page.hasMore
            let loadedAttachmentData = try await loadAttachmentData(
                messages: page.messages,
                store: store
            )
            attachmentDataByID.merge(loadedAttachmentData) { _, new in new }
            return firstMessageID
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func sendMessage() async {
        guard isHumanDirect, !isSending else { return }
        let content = draftMessage.trimmingCharacters(in: .whitespacesAndNewlines)
        let outgoingAttachments = attachments
        guard !content.isEmpty || !outgoingAttachments.isEmpty else { return }
        errorMessage = nil
        isSending = true
        defer { isSending = false }
        do {
            let store = try await resolveStore()
            let post = try await store.postMessage(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                draft: .init(
                    senderKind: .human,
                    senderID: ownerUserID,
                    content: content,
                    attachments: outgoingAttachments.map(ProjectAgentMessageAttachmentDraft.init)
                ),
                limits: .init()
            )
            draftMessage = ""
            attachments = []
            attachmentError = nil
            await load()
            scrollToLatestRequest &+= 1
            if !post.deliveries.isEmpty { startScheduler() }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func approveTeamProposal(
        _ proposal: LocalAgentTeamCreationProposal
    ) async -> WorkspaceProject? {
        guard proposalActionIDs.insert(proposal.id).inserted else { return nil }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let createdProject: WorkspaceProject?
            let resolvedProjectID: String
            if let existingProjectID = proposal.draft.existingProjectID {
                let registry = try await projectsService.registry()
                guard let project = try await registry.get(
                    ownerUserID: ownerUserID,
                    id: existingProjectID
                ), project.status == .active else {
                    throw ProjectRegistryError.notFound
                }
                createdProject = nil
                resolvedProjectID = project.id
            } else if let newProjectName = proposal.draft.newProjectName {
                let project = try await projectsService.createInDefaultWorkspace(
                    ownerUserID: ownerUserID,
                    name: newProjectName,
                    description: proposal.draft.newProjectDescription,
                    projectTypeKey: proposal.draft.newProjectTypeKey
                        ?? LocalAgentSkillCatalog.legacyProjectTypeKey
                )
                createdProject = project
                resolvedProjectID = project.id
            } else {
                throw AgentGroupChatError.conflict
            }
            let store = try await resolveStore()
            _ = try await store.approveTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: conversationID,
                proposalID: proposal.id,
                resolvedProjectID: resolvedProjectID,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            await load()
            return createdProject
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func approveAgentProposal(_ proposal: LocalAgentCreationProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            _ = try await builderService.approveProposal(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                proposal: proposal
            )
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectAgentProposal(_ proposal: LocalAgentCreationProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectAgentProposal(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectTeamProposal(_ proposal: LocalAgentTeamCreationProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: conversationID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func approveMembershipProposal(_ proposal: LocalAgentMembershipProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.approveMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: conversationID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectMembershipProposal(_ proposal: LocalAgentMembershipProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: conversationID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func resolveStore() async throws -> SQLiteAgentGroupChatStore {
        if let openedStore { return openedStore }
        let store = try await service.store()
        openedStore = store
        return store
    }

    private func loadAttachmentData(
        messages: [ProjectAgentMessage],
        excluding loadedAttachmentIDs: Set<String> = [],
        store: SQLiteAgentGroupChatStore
    ) async throws -> [String: Data] {
        var result: [String: Data] = [:]
        for message in messages {
            for attachment in message.attachmentItems
            where attachment.kind == .image && !loadedAttachmentIDs.contains(attachment.id) {
                guard let payload = try await store.messageAttachment(
                    ownerUserID: ownerUserID,
                    roomID: conversationID,
                    messageID: message.id,
                    attachmentID: attachment.id
                ) else { continue }
                result[attachment.id] = try Data(
                    contentsOf: payload.localFileURL,
                    options: [.mappedIfSafe]
                )
            }
        }
        return result
    }

    private func mergeMessages(
        _ current: [ProjectAgentMessage],
        with incoming: [ProjectAgentMessage]
    ) -> [ProjectAgentMessage] {
        var byID = Dictionary(uniqueKeysWithValues: current.map { ($0.id, $0) })
        for message in incoming {
            byID[message.id] = message
        }
        return byID.values.sorted {
            ($0.createdAtUnixMs, $0.id) < ($1.createdAtUnixMs, $1.id)
        }
    }

    private func startScheduler() {
        guard schedulerTask == nil else { return }
        isRunningAgents = true
        schedulerTask = Task { [weak self] in
            guard let self else { return }
            var schedulerMessage: String?
            do {
                let results = try await scheduler.drainAccount(ownerUserID: ownerUserID)
                if let failure = results.last(where: { $0.outcome == .failed }) {
                    schedulerMessage = failure.detail ?? "Agent 运行失败。"
                } else if let suspended = results.last(where: { $0.outcome == .suspended }) {
                    schedulerMessage = suspended.detail ?? "Agent 已暂停。"
                }
            } catch {
                schedulerMessage = error.localizedDescription
            }
            await load()
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            if let schedulerMessage { errorMessage = schedulerMessage }
            isRunningAgents = false
            schedulerTask = nil
        }
    }

    func dismissError() {
        errorMessage = nil
    }
}

struct AgentDirectChatView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: AgentDirectChatViewModel

    init(
        ownerUserID: String,
        conversationID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler,
        builderService: LocalAgentBuilderService,
        projectsService: NativeLocalProjectsService
    ) {
        _viewModel = StateObject(wrappedValue: AgentDirectChatViewModel(
            ownerUserID: ownerUserID,
            conversationID: conversationID,
            service: service,
            scheduler: scheduler,
            builderService: builderService,
            projectsService: projectsService
        ))
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if !viewModel.hasCompletedInitialLoad {
                ProgressView("正在读取聊天记录…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                transcript
            }
            if let errorMessage = viewModel.errorMessage {
                Divider()
                errorBanner(errorMessage)
            }
            if viewModel.isHumanDirect {
                Divider()
                composer
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task { await viewModel.activate() }
    }

    private func errorBanner(_ message: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
            Text(message)
                .font(.callout)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button {
                viewModel.dismissError()
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .help("关闭错误提示")
            .accessibilityLabel("关闭错误提示")
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 10)
        .background(Color.red.opacity(0.08))
    }

    private var header: some View {
        HStack(spacing: 12) {
            Image(systemName: viewModel.isHumanDirect
                  ? "bubble.left.and.bubble.right.fill" : "person.2.wave.2.fill")
                .font(.title2)
                .foregroundStyle(Color.accentColor)
            VStack(alignment: .leading, spacing: 2) {
                Text(viewModel.title).font(.headline)
                Text(viewModel.isHumanDirect ? "私聊" : "Agent 之间的私聊")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if viewModel.isRunningAgents {
                ProgressView().controlSize(.small)
            }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
    }

    private var transcript: some View {
        AgentChatTimelineView(
            items: viewModel.timelineItems,
            isInitialContentReady: viewModel.isInitialTimelineReady,
            hasOlderItems: viewModel.hasOlderMessages,
            isLoadingOlderItems: viewModel.isLoadingOlderMessages,
            scrollToLatestRequest: viewModel.scrollToLatestRequest,
            loadOlderItems: {
                await viewModel.loadOlderMessages().map { "message:\($0)" }
            },
            rowContent: { item in
                Group {
                    switch item {
                    case let .message(message):
                        messageRow(message)
                    case let .agentProposal(proposal):
                        agentProposalCard(proposal)
                    case let .teamProposal(proposal):
                        proposalCard(proposal)
                    case let .membershipProposal(proposal):
                        membershipProposalCard(proposal)
                    }
                }
            },
            emptyContent: {
                ContentUnavailableView {
                    Label("开始对话", systemImage: "bubble.left")
                }
                .padding(.top, 80)
            }
        )
    }

    private func messageRow(_ message: ProjectAgentMessage) -> some View {
        let isHuman = message.senderKind == .human
        return HStack(alignment: .top, spacing: 12) {
            if isHuman { Spacer(minLength: 80) }
            VStack(alignment: isHuman ? .trailing : .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Text(viewModel.displayName(for: message))
                        .font(.caption.weight(.medium))
                    Text(formattedMessageTime(message.createdAtUnixMs))
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                .foregroundStyle(.secondary)
                if !message.content.isEmpty {
                    MarkdownDocumentView(markdown: message.content, widthBehavior: .fitContent)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 12)
                        .background(messageBubbleBackground(isHuman: isHuman))
                        .overlay {
                            RoundedRectangle(cornerRadius: 13)
                                .stroke(
                                    isHuman
                                        ? Color.accentColor.opacity(0.22)
                                        : Color.primary.opacity(0.07),
                                    lineWidth: 1
                                )
                        }
                }
                if !message.attachmentItems.isEmpty {
                    AgentMessageAttachmentChips(
                        ownerUserID: message.ownerUserID,
                        roomID: message.roomID,
                        messageID: message.id,
                        creatorName: viewModel.displayName(for: message),
                        attachments: message.attachmentItems,
                        dataByID: viewModel.attachmentDataByID,
                        service: model.agentGroupChatService
                    )
                }
            }
            .frame(maxWidth: 900, alignment: isHuman ? .trailing : .leading)
            if !isHuman { Spacer(minLength: 80) }
        }
        .frame(maxWidth: .infinity)
    }

    private func messageBubbleBackground(isHuman: Bool) -> AnyShapeStyle {
        isHuman
            ? AnyShapeStyle(Color.accentColor.opacity(0.11))
            : AnyShapeStyle(Color(nsColor: .controlBackgroundColor))
    }

    private func formattedMessageTime(_ unixMs: Int64) -> String {
        Date(timeIntervalSince1970: TimeInterval(unixMs) / 1_000)
            .formatted(date: .omitted, time: .shortened)
    }

    private func proposalCard(_ proposal: LocalAgentTeamCreationProposal) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("创建团队", systemImage: "person.3.sequence.fill")
                .font(.headline)
            Text(proposal.draft.teamName)
            if let key = proposal.draft.newProjectTypeKey,
               let type = projectType(key) {
                Text("项目类型：\(type.label)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if !proposal.draft.teamGoal.isEmpty {
                Text(proposal.draft.teamGoal).font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Button("拒绝", role: .destructive) {
                    Task { await viewModel.rejectTeamProposal(proposal) }
                }
                Button(proposal.draft.newProjectName == nil ? "确认创建团队" : "确认创建项目和团队") {
                    Task {
                        if let project = await viewModel.approveTeamProposal(proposal) {
                            model.registerCreatedProject(project)
                        }
                    }
                }
                .buttonStyle(.borderedProminent)
            }
            .disabled(viewModel.proposalActionIDs.contains(proposal.id))
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.accentColor.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
    }

    private func membershipProposalCard(
        _ proposal: LocalAgentMembershipProposal
    ) -> some View {
        let agentName = viewModel.profilesByID[proposal.draft.targetAgentID]?.draft.name
            ?? "Agent"
        let teamName = viewModel.teamsByID[proposal.draft.targetTeamRoomID]?.draft.name
            ?? "项目团队"
        return VStack(alignment: .leading, spacing: 8) {
            Label("邀请现有 Agent", systemImage: "person.crop.circle.badge.plus")
                .font(.headline)
            Text("\(agentName) → \(teamName)")
            Text("职责：\(proposal.draft.role)")
                .font(.caption)
                .foregroundStyle(.secondary)
            if !proposal.draft.responsibility.isEmpty {
                Text(proposal.draft.responsibility)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            HStack {
                Button("拒绝", role: .destructive) {
                    Task { await viewModel.rejectMembershipProposal(proposal) }
                }
                Button("确认加入团队") {
                    Task { await viewModel.approveMembershipProposal(proposal) }
                }
                .buttonStyle(.borderedProminent)
            }
            .disabled(viewModel.proposalActionIDs.contains(proposal.id))
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.accentColor.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
    }

    private func agentProposalCard(_ proposal: LocalAgentCreationProposal) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("创建 Agent", systemImage: "person.badge.plus")
                .font(.headline)
            Text("\(proposal.draft.name) · \(proposal.draft.role)")
            Text("职业：\(profession(proposal.draft.professionKey)?.label ?? proposal.draft.professionKey)")
                .font(.caption)
                .foregroundStyle(.secondary)
            Text("思考等级：\(proposal.draft.thinkingLevel ?? "跟随模型默认")")
                .font(.caption)
                .foregroundStyle(.secondary)
            if !proposal.draft.responsibility.isEmpty {
                Text(proposal.draft.responsibility).font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Button("拒绝", role: .destructive) {
                    Task { await viewModel.rejectAgentProposal(proposal) }
                }
                Button("确认创建") {
                    Task { await viewModel.approveAgentProposal(proposal) }
                }
                .buttonStyle(.borderedProminent)
            }
            .disabled(viewModel.proposalActionIDs.contains(proposal.id))
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.accentColor.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
    }

    private func profession(_ key: String) -> LocalAgentProfessionDefinition? {
        guard let owner = model.localProjectOwnerUserID else { return nil }
        return model.agentSkillLibrary.profession(ownerUserID: owner, key: key)
    }

    private func projectType(_ key: String) -> LocalProjectTypeDefinition? {
        guard let owner = model.localProjectOwnerUserID else { return nil }
        return model.agentSkillLibrary.projectType(ownerUserID: owner, key: key)
    }

    private var composer: some View {
        AgentChatComposerView(
            text: $viewModel.draftMessage,
            attachments: $viewModel.attachments,
            attachmentError: $viewModel.attachmentError,
            isSending: viewModel.isSending,
            placeholder: "输入消息，或粘贴图片、文档和长文本…",
            mentionCandidates: [],
            onMentionSelected: { _ in },
            onSend: { Task { await viewModel.sendMessage() } },
            leadingControl: { EmptyView() }
        )
        .padding(14)
    }
}
