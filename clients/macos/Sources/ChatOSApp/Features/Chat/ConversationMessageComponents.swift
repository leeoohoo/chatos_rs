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

struct TaskAgentReplyView: View {
    @EnvironmentObject private var model: AppModel
    let reply: ConversationAssistantReply
    let expandedSection: TaskReplyInspectorSection?
    var projectRootPath: String? = nil
    let onToggleInspector: (TaskReplyInspectorSection) -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "sparkles")
                .foregroundStyle(AppPalette.ai)
                .appFont(.title3)
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text("叽咕狸").appFont(.caption.weight(.semibold))
                    Text(reply.message.createdAt, style: .time)
                        .appFont(.caption2)
                        .foregroundStyle(.tertiary)
                    if let callback = reply.taskCallback {
                        StatusCapsule(title: statusTitle(callback.status), color: statusColor(callback.status))
                    }
                    Spacer()
                }
                MarkdownDocumentView(
                    markdown: reply.message.text,
                    allowsTextSelection: false
                )
                .environment(\.openURL, petFileOpenAction)
                HStack(spacing: 14) {
                    inspectorButton(
                        title: "执行过程",
                        icon: "waveform.path.ecg",
                        section: .process
                    )
                    inspectorButton(
                        title: "任务详情",
                        icon: "doc.text.magnifyingglass",
                        section: .detail
                    )
                }
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

    private func inspectorButton(
        title: String,
        icon: String,
        section: TaskReplyInspectorSection
    ) -> some View {
        let isSelected = expandedSection == section
        return Button {
            onToggleInspector(section)
        } label: {
            Label(LocalizedStringKey(title), systemImage: icon)
                .appFont(.caption.weight(.medium))
                .padding(.horizontal, 9)
                .padding(.vertical, 5)
                .foregroundStyle(isSelected ? Color.white : AppPalette.ai)
                .background(
                    isSelected ? AppPalette.ai : AppPalette.ai.opacity(0.07),
                    in: Capsule()
                )
        }
        .buttonStyle(.plain)
        .help(isSelected
              ? model.localized("再次点击收起", english: "Click again to collapse")
              : model.localized("在当前消息下方展开", english: "Expand below the current message"))
    }

    private func statusTitle(_ status: String?) -> String {
        switch status?.lowercased() {
        case "succeeded", "completed", "success": "已完成"
        case "failed": "失败"
        case "blocked": "阻塞"
        case "running", "processing", "in_progress": "执行中"
        default: status ?? "任务"
        }
    }

    private func statusColor(_ status: String?) -> Color {
        switch status?.lowercased() {
        case "succeeded", "completed", "success": .green
        case "failed": .red
        case "blocked": .orange
        case "running", "processing", "in_progress": AppPalette.ai
        default: .secondary
        }
    }
}
