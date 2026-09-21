import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
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
        .background(AppPalette.canvas)
    }

    private func messageRow(_ message: ProjectAgentMessage) -> some View {
        let isHuman = message.senderKind == .human
        let displayName = viewModel.displayName(senderID: message.senderID, kind: message.senderKind)
        return HStack(alignment: .top, spacing: 10) {
            if isHuman { Spacer(minLength: 100) }
            if !isHuman {
                messageAvatar(
                    name: displayName,
                    avatarData: viewModel.profilesByID[message.senderID]?.draft.avatarData,
                    isHuman: false
                )
            }

            VStack(alignment: isHuman ? .trailing : .leading, spacing: 7) {
                HStack(spacing: 7) {
                    Text(displayName)
                        .appFont(.caption.weight(.semibold))
                        .foregroundStyle(.primary)
                    Text(formattedMessageTime(message.createdAtUnixMs))
                        .appFont(.caption2)
                        .foregroundStyle(.tertiary)
                }

                VStack(alignment: .leading, spacing: 9) {
                    if !message.content.isEmpty {
                        MarkdownDocumentView(markdown: message.content)
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
                    if !message.mentionedAgentIDs.isEmpty {
                        Text(message.mentionedAgentIDs.compactMap { id in
                            guard let name = viewModel.profilesByID[id]?.draft.name else { return nil }
                            return "@\(name)"
                        }.joined(separator: "  "))
                        .appFont(.caption.weight(.medium))
                        .foregroundStyle(AppPalette.ai)
                    }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
                .background(
                    isHuman ? AppPalette.aiSoft : AppPalette.surface,
                    in: RoundedRectangle(cornerRadius: 14)
                )
                .overlay {
                    RoundedRectangle(cornerRadius: 14)
                        .stroke(
                            isHuman ? AppPalette.ai.opacity(0.20) : AppPalette.border.opacity(0.70),
                            lineWidth: 1
                        )
                }
                .shadow(color: .black.opacity(isHuman ? 0 : 0.025), radius: 5, y: 2)
            }
            .frame(maxWidth: isHuman ? 720 : 820, alignment: isHuman ? .trailing : .leading)

            if isHuman { messageAvatar(name: displayName, avatarData: nil, isHuman: true) }
            if !isHuman { Spacer(minLength: 64) }
        }
        .frame(maxWidth: 980)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func messageAvatar(name: String, avatarData: Data?, isHuman: Bool) -> some View {
        if isHuman {
            Text("你")
                .appFont(.caption.weight(.semibold))
                .foregroundStyle(AppPalette.ai)
                .frame(width: AgentAvatarMetrics.message, height: AgentAvatarMetrics.message)
                .background(AppPalette.aiSoft, in: RoundedRectangle(cornerRadius: 20))
                .overlay {
                    RoundedRectangle(cornerRadius: 20)
                        .stroke(AppPalette.ai.opacity(0.20), lineWidth: 1)
                }
        } else {
            AgentAvatarView(
                name: name,
                data: avatarData,
                size: AgentAvatarMetrics.message,
                cornerRadius: 20
            )
        }
    }

    private func formattedMessageTime(_ unixMs: Int64) -> String {
        Date(timeIntervalSince1970: TimeInterval(unixMs) / 1_000)
            .formatted(date: .omitted, time: .shortened)
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
        .padding(.horizontal, 18)
        .padding(.top, 8)
        .padding(.bottom, 14)
        .frame(maxWidth: .infinity, alignment: .leading)
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
