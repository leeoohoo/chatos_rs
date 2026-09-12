import SwiftUI

struct ConversationHistoryStatusView: View {
    @ObservedObject var conversation: ConversationSessionViewModel
    @EnvironmentObject private var model: AppModel

    var body: some View {
        if let error = conversation.sendError {
            HStack(spacing: 8) {
                Label(
                    model.localized("消息发送失败", english: "Message failed to send"),
                    systemImage: "exclamationmark.triangle"
                )
                    .appFont(.caption.weight(.medium))
                Text(error)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer()
                Button(model.localized("重试", english: "Retry"), action: conversation.sendDraft)
                    .controlSize(.small)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 7)
            .background(Color.orange.opacity(0.08))
        } else if let error = conversation.runtimeSettingsError {
            HStack(spacing: 8) {
                Label(
                    model.localized("会话设置更新失败", english: "Conversation settings update failed"),
                    systemImage: "exclamationmark.triangle"
                )
                    .appFont(.caption.weight(.medium))
                Text(error)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer()
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 7)
            .background(Color.orange.opacity(0.08))
        } else if let error = conversation.askUserStateError {
            HStack(spacing: 8) {
                Label(
                    model.localized("本地交互状态读取失败", english: "Local interaction state failed"),
                    systemImage: "exclamationmark.triangle"
                )
                    .appFont(.caption.weight(.medium))
                Text(error)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer()
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 7)
            .background(Color.orange.opacity(0.08))
        }
    }
}
