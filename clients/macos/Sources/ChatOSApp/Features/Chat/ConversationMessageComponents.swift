import ChatOSCore
import SwiftUI

struct UserTurnMessageView: View {
    let turn: ConversationTurn
    let showsTaskGraph: Bool
    let onOpenProcess: () -> Void
    let onOpenTaskGraph: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "person.crop.circle.fill")
                .foregroundStyle(.secondary)
                .appFont(.title3)
            VStack(alignment: .leading, spacing: 9) {
                HStack {
                    Text("你").appFont(.caption.weight(.semibold))
                    Text(turn.userMessage.createdAt, style: .time)
                        .appFont(.caption2)
                        .foregroundStyle(.tertiary)
                }
                if !turn.userMessage.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    Text(turn.userMessage.text)
                }
                if !turn.userMessage.attachments.isEmpty {
                    MessageAttachmentChips(attachments: turn.userMessage.attachments)
                }
                if showsProcess || showsTaskGraph {
                    HStack(spacing: 8) {
                        if showsProcess {
                            Button("查看过程", systemImage: "waveform.path.ecg", action: onOpenProcess)
                                .help("查看这一轮对话的推理、工具调用和中间结果")
                        }
                        if showsTaskGraph {
                            Button("任务图", systemImage: "point.3.connected.trianglepath.dotted", action: onOpenTaskGraph)
                                .help("查看这条用户消息创建的 Task Runner 任务图")
                        }
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                }
            }
            .padding(13)
            .background(AppPalette.ai.opacity(0.07), in: RoundedRectangle(cornerRadius: 12))
            .overlay {
                RoundedRectangle(cornerRadius: 12)
                    .stroke(AppPalette.ai.opacity(0.16), lineWidth: 1)
            }
        }
    }

    private var showsProcess: Bool {
        !turn.processEvents.isEmpty
    }

}

struct AssistantReplyView: View {
    @EnvironmentObject private var model: AppModel
    let reply: ConversationAssistantReply
    var projectRootPath: String? = nil

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            agentIcon
            VStack(alignment: .leading, spacing: 5) {
                replyHeader
                MarkdownDocumentView(
                    markdown: reply.message.text,
                    allowsTextSelection: false
                )
                .environment(\.openURL, petFileOpenAction)
            }
        }
    }

    private var petFileOpenAction: OpenURLAction {
        OpenURLAction { url in
            model.openPetFileLink(url, projectRootPath: projectRootPath)
                ? .handled
                : .systemAction
        }
    }

    var agentIcon: some View {
        Image(systemName: "sparkles")
            .foregroundStyle(AppPalette.ai)
            .appFont(.title3)
    }

    var replyHeader: some View {
        HStack {
            Text("叽咕狸").appFont(.caption.weight(.semibold))
            Text(reply.message.createdAt, style: .time)
                .appFont(.caption2)
                .foregroundStyle(.tertiary)
        }
    }
}
