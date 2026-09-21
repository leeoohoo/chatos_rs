import ChatOSConnector
import ChatOSCore
import SwiftUI
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
            if let errorMessage = viewModel.presentedErrorMessage {
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
            if viewModel.isHumanDirect,
               let agentID = viewModel.conversation?.defaultAgentID,
               let agent = viewModel.profilesByID[agentID] {
                AgentAvatarView(
                    name: agent.draft.name,
                    data: agent.draft.avatarData,
                    size: AgentAvatarMetrics.header,
                    cornerRadius: 24
                )
            } else {
                Image(systemName: "person.2.wave.2.fill")
                    .font(.title2)
                    .foregroundStyle(Color.accentColor)
            }
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
        let displayName = viewModel.displayName(for: message)
        return HStack(alignment: .top, spacing: 12) {
            if isHuman { Spacer(minLength: 80) }
            if !isHuman {
                AgentAvatarView(
                    name: displayName,
                    data: viewModel.profilesByID[message.senderID]?.draft.avatarData,
                    size: AgentAvatarMetrics.message,
                    cornerRadius: 20
                )
            }
            VStack(alignment: isHuman ? .trailing : .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Text(displayName)
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
                        creatorName: displayName,
                        attachments: message.attachmentItems,
                        dataByID: viewModel.attachmentDataByID,
                        service: model.agentGroupChatService
                    )
                }
            }
            .frame(maxWidth: 900, alignment: isHuman ? .trailing : .leading)
            if isHuman {
                AgentAvatarView(
                    name: "你",
                    data: nil,
                    size: AgentAvatarMetrics.message,
                    cornerRadius: 20
                )
            }
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
            Label(
                proposal.draft.importedProjectDraft == nil ? "创建团队" : "导入目录并创建团队",
                systemImage: "person.3.sequence.fill"
            )
                .font(.headline)
            Text(proposal.draft.teamName)
            if let importedDraft = proposal.draft.importedProjectDraft,
               let absolutePath = proposal.draft.importedProjectAbsolutePath {
                Text("项目：\(importedDraft.name)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("目录：\(absolutePath)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                Text("只注册这个现有目录，不会创建、移动、复制或链接目录。")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
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
                Button(
                    proposal.draft.existingProjectID == nil
                        ? "确认创建项目和团队"
                        : "确认创建团队"
                ) {
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
