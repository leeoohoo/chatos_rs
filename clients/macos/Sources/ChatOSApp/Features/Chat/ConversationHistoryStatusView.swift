import SwiftUI

struct ConversationHistoryStatusView: View {
    @ObservedObject var conversation: ConversationSessionViewModel
    @EnvironmentObject private var model: AppModel

    var body: some View {
        if conversation.isRefreshing {
            Label(
                model.localized("正在同步最新消息…", english: "Syncing latest messages…"),
                systemImage: "arrow.triangle.2.circlepath"
            )
                .appFont(.caption)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 20)
                .padding(.vertical, 7)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color.accentColor.opacity(0.05))
        } else if let error = conversation.sendError {
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
        } else if let error = conversation.historyError {
            HStack(spacing: 8) {
                Label(
                    model.localized("历史同步失败", english: "History sync failed"),
                    systemImage: "exclamationmark.triangle"
                )
                    .appFont(.caption.weight(.medium))
                Text(error)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer()
                Button(model.localized("重试", english: "Retry"), action: conversation.refreshLatest)
                    .controlSize(.small)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 7)
            .background(Color.orange.opacity(0.08))
        }
    }
}
