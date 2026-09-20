import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import SwiftUI

extension ProjectAgentGroupChatView {
    var transcript: some View {
        AgentChatTimelineView(
            items: viewModel.messages,
            isInitialContentReady: !viewModel.isLoading,
            hasOlderItems: viewModel.hasOlderMessages,
            isLoadingOlderItems: viewModel.isLoadingOlderMessages,
            scrollToLatestRequest: viewModel.scrollToLatestRequest,
            loadOlderItems: { await viewModel.loadOlderMessages() },
            rowContent: { message in messageRow(message) },
            emptyContent: {
                ContentUnavailableView(
                    "还没有消息",
                    systemImage: "bubble.left.and.bubble.right",
                    description: Text("创建 Agent 后，通过 @ 提及开始协作。")
                )
                .padding(.top, 70)
            }
        )
    }

    private func messageRow(_ message: ProjectAgentMessage) -> some View {
        let isHuman = message.senderKind == .human
        return HStack {
            if isHuman { Spacer(minLength: 80) }
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Text(viewModel.displayName(senderID: message.senderID, kind: message.senderKind))
                        .appFont(.caption).fontWeight(.semibold)
                    if message.hopCount > 0 {
                        Text("第 \(message.hopCount) 跳")
                            .appFont(.caption2).foregroundStyle(.secondary)
                    }
                }
                if !message.content.isEmpty {
                    MarkdownDocumentView(markdown: message.content)
                }
                if !message.attachmentItems.isEmpty {
                    AgentMessageAttachmentChips(
                        ownerUserID: message.ownerUserID,
                        roomID: message.roomID,
                        messageID: message.id,
                        creatorName: viewModel.displayName(
                            senderID: message.senderID,
                            kind: message.senderKind
                        ),
                        attachments: message.attachmentItems,
                        dataByID: viewModel.attachmentDataByID,
                        service: model.agentGroupChatService
                    )
                }
                if !message.mentionedAgentIDs.isEmpty {
                    Text(message.mentionedAgentIDs.compactMap { id in
                        guard let name = viewModel.profilesByID[id]?.draft.name else { return nil }
                        return "@\(name)"
                    }.joined(separator: "  "))
                    .appFont(.caption)
                    .foregroundStyle(.tint)
                }
            }
            .padding(12)
            .background(
                isHuman ? Color.accentColor.opacity(0.12) : Color.secondary.opacity(0.10),
                in: RoundedRectangle(cornerRadius: 12)
            )
            if !isHuman { Spacer(minLength: 80) }
        }
    }

    var composer: some View {
        VStack(alignment: .leading, spacing: 8) {
            if viewModel.isRunningAgents {
                HStack(spacing: 6) {
                    ProgressView().controlSize(.small)
                    Text("本地 Agent 正在处理群聊…")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            if !viewModel.selectedMentionAgentIDs.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(viewModel.selectedMentionAgentIDs.sorted(), id: \.self) { id in
                            Button {
                                viewModel.toggleMention(agentID: id)
                            } label: {
                                Text("@\(viewModel.profilesByID[id]?.draft.name ?? id)  ×")
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                        }
                    }
                }
            }
            AgentChatComposerView(
                text: $viewModel.draftMessage,
                attachments: $viewModel.attachments,
                attachmentError: $viewModel.attachmentError,
                isSending: viewModel.isSending,
                placeholder: "输入消息；不选择 @ 时交给默认 Agent，也可粘贴图片、文档和长文本…",
                mentionCandidates: mentionCandidates,
                onMentionSelected: { viewModel.selectMention(agentID: $0) },
                onSend: { Task { await viewModel.sendMessage() } }
            ) {
                Menu {
                    if viewModel.activeMembers.isEmpty {
                        Text("先创建 Agent")
                    }
                    ForEach(viewModel.activeMembers) { item in
                        Button {
                            viewModel.toggleMention(agentID: item.member.agentID)
                        } label: {
                            Label(
                                item.profile?.draft.name ?? item.member.agentID,
                                systemImage: viewModel.selectedMentionAgentIDs.contains(item.member.agentID)
                                    ? "checkmark.circle.fill" : "circle"
                            )
                        }
                    }
                } label: {
                    Image(systemName: "at")
                }
                .menuStyle(.borderlessButton)
                .disabled(viewModel.activeMembers.isEmpty)
                .help("选择要 @ 的 Agent；不选择时交给默认 Agent")
            }
        }
        .padding(12)
        .background(.bar)
    }

    private var mentionCandidates: [AgentChatMentionCandidate] {
        viewModel.activeMembers.compactMap { item in
            guard let profile = item.profile,
                  !viewModel.selectedMentionAgentIDs.contains(item.member.agentID) else {
                return nil
            }
            return AgentChatMentionCandidate(
                id: item.member.agentID,
                name: profile.draft.name,
                subtitle: profession(profile.draft.professionKey)?.label
            )
        }
    }
}
