import ChatOSConnector
import ChatOSCore
import SwiftUI

@MainActor
private final class AgentDirectChatViewModel: ObservableObject {
    @Published private(set) var conversation: ProjectAgentRoom?
    @Published private(set) var agents: [LocalAgentProfile] = []
    @Published private(set) var members: [ProjectAgentRoomMember] = []
    @Published private(set) var messages: [ProjectAgentMessage] = []
    @Published private(set) var pendingAgentProposals: [LocalAgentCreationProposal] = []
    @Published private(set) var pendingTeamProposals: [LocalAgentTeamCreationProposal] = []
    @Published var draftMessage = ""
    @Published private(set) var isLoading = false
    @Published private(set) var isSending = false
    @Published private(set) var isRunningAgents = false
    @Published private(set) var proposalActionIDs: Set<String> = []
    @Published var errorMessage: String?

    private let ownerUserID: String
    private let conversationID: String
    private let service: NativeAgentGroupChatService
    private let scheduler: LocalAgentGroupChatScheduler
    private let builderService: LocalAgentBuilderService
    private let projectsService: NativeLocalProjectsService
    private var openedStore: SQLiteAgentGroupChatStore?
    private var schedulerTask: Task<Void, Never>?

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

    var profilesByID: [String: LocalAgentProfile] {
        Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0) })
    }

    var title: String { conversation?.draft.name ?? "私聊" }

    var isHumanDirect: Bool { conversation?.conversationKind == .humanAgentDirect }

    func displayName(for message: ProjectAgentMessage) -> String {
        switch message.senderKind {
        case .human: "你"
        case .system: "系统"
        case .agent: profilesByID[message.senderID]?.draft.name ?? "Agent"
        }
    }

    func activate() async {
        await load()
        startScheduler()
    }

    func load() async {
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let store = try await resolveStore()
            guard let conversation = try await store.room(
                ownerUserID: ownerUserID,
                roomID: conversationID
            ), conversation.conversationKind.isDirect else {
                throw AgentGroupChatError.notFound
            }
            async let loadedAgents = store.listAgents(
                ownerUserID: ownerUserID,
                includeArchived: false
            )
            async let loadedMembers = store.listMembers(
                ownerUserID: ownerUserID,
                roomID: conversationID
            )
            async let loadedMessages = store.listMessages(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                afterUnixMs: nil,
                limit: 500
            )
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
            self.conversation = conversation
            agents = try await loadedAgents
            members = try await loadedMembers
            messages = try await loadedMessages
            pendingTeamProposals = try await loadedProposals
            pendingAgentProposals = try await loadedAgentProposals
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func sendMessage() async {
        guard isHumanDirect, !isSending else { return }
        let content = draftMessage.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !content.isEmpty else { return }
        isSending = true
        defer { isSending = false }
        do {
            let store = try await resolveStore()
            let post = try await store.postMessage(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                draft: .init(senderKind: .human, senderID: ownerUserID, content: content),
                limits: .init()
            )
            draftMessage = ""
            await load()
            if !post.deliveries.isEmpty { startScheduler() }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func approveTeamProposal(_ proposal: LocalAgentTeamCreationProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let resolvedProjectID: String
            if let existingProjectID = proposal.draft.existingProjectID {
                let registry = try await projectsService.registry()
                guard let project = try await registry.get(
                    ownerUserID: ownerUserID,
                    id: existingProjectID
                ), project.status == .active else {
                    throw ProjectRegistryError.notFound
                }
                resolvedProjectID = project.id
            } else if let newProjectName = proposal.draft.newProjectName {
                let project = try await projectsService.createInDefaultWorkspace(
                    ownerUserID: ownerUserID,
                    name: newProjectName,
                    description: proposal.draft.newProjectDescription
                )
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
        } catch {
            errorMessage = error.localizedDescription
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

    private func resolveStore() async throws -> SQLiteAgentGroupChatStore {
        if let openedStore { return openedStore }
        let store = try await service.store()
        openedStore = store
        return store
    }

    private func startScheduler() {
        guard schedulerTask == nil else { return }
        isRunningAgents = true
        schedulerTask = Task { [weak self] in
            guard let self else { return }
            do {
                let results = try await scheduler.drainAccount(ownerUserID: ownerUserID)
                if let failure = results.last(where: { $0.outcome == .failed }) {
                    errorMessage = failure.detail ?? "Agent 运行失败。"
                } else if let suspended = results.last(where: { $0.outcome == .suspended }) {
                    errorMessage = suspended.detail ?? "Agent 已暂停。"
                }
            } catch {
                errorMessage = error.localizedDescription
            }
            await load()
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            isRunningAgents = false
            schedulerTask = nil
        }
    }
}

struct AgentDirectChatView: View {
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
            if viewModel.isLoading, viewModel.messages.isEmpty {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                transcript
            }
            if viewModel.isHumanDirect {
                Divider()
                composer
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task { await viewModel.activate() }
        .alert("私聊错误", isPresented: Binding(
            get: { viewModel.errorMessage != nil },
            set: { if !$0 { viewModel.errorMessage = nil } }
        )) {
            Button("好", role: .cancel) { viewModel.errorMessage = nil }
        } message: {
            Text(viewModel.errorMessage ?? "")
        }
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
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 14) {
                    ForEach(viewModel.pendingAgentProposals) { proposal in
                        agentProposalCard(proposal)
                    }
                    ForEach(viewModel.pendingTeamProposals) { proposal in
                        proposalCard(proposal)
                    }
                    if viewModel.messages.isEmpty {
                        ContentUnavailableView {
                            Label("开始对话", systemImage: "bubble.left")
                        }
                        .padding(.top, 80)
                    } else {
                        ForEach(viewModel.messages) { message in
                            messageRow(message).id(message.id)
                        }
                    }
                }
                .padding(18)
            }
            .onChange(of: viewModel.messages.count) {
                if let id = viewModel.messages.last?.id {
                    withAnimation { proxy.scrollTo(id, anchor: .bottom) }
                }
            }
        }
    }

    private func messageRow(_ message: ProjectAgentMessage) -> some View {
        let isHuman = message.senderKind == .human
        return HStack {
            if isHuman { Spacer(minLength: 80) }
            VStack(alignment: isHuman ? .trailing : .leading, spacing: 4) {
                Text(viewModel.displayName(for: message))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(message.content)
                    .textSelection(.enabled)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 9)
                    .background(
                        isHuman ? Color.accentColor : Color(nsColor: .controlBackgroundColor),
                        in: RoundedRectangle(cornerRadius: 12)
                    )
                    .foregroundStyle(isHuman ? Color.white : Color.primary)
            }
            if !isHuman { Spacer(minLength: 80) }
        }
        .frame(maxWidth: .infinity)
    }

    private func proposalCard(_ proposal: LocalAgentTeamCreationProposal) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("创建团队", systemImage: "person.3.sequence.fill")
                .font(.headline)
            Text(proposal.draft.teamName)
            if !proposal.draft.teamGoal.isEmpty {
                Text(proposal.draft.teamGoal).font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Button("拒绝", role: .destructive) {
                    Task { await viewModel.rejectTeamProposal(proposal) }
                }
                Button(proposal.draft.newProjectName == nil ? "确认创建团队" : "确认创建项目和团队") {
                    Task { await viewModel.approveTeamProposal(proposal) }
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

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField("发消息", text: $viewModel.draftMessage, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...6)
                .onSubmit { Task { await viewModel.sendMessage() } }
            Button {
                Task { await viewModel.sendMessage() }
            } label: {
                Image(systemName: "arrow.up.circle.fill").font(.title2)
            }
            .buttonStyle(.plain)
            .foregroundStyle(Color.accentColor)
            .disabled(viewModel.draftMessage.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                      || viewModel.isSending)
        }
        .padding(14)
    }
}
