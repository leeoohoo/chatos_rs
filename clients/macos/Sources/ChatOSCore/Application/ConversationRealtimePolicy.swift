import Foundation

public enum ConversationRealtimeAction: Sendable, Equatable {
    case refreshAskUserPrompts
    case refreshHistory
    case failure(message: String)
    case ignore
}

/// Maps transport signals to Conversation application actions.
public enum ConversationRealtimePolicy {
    public static let defaultFailureMessage = "AI 处理失败，请检查模型配置后重试。"

    public static func action(
        for signal: ConversationRealtimeSignal
    ) -> ConversationRealtimeAction {
        if signal.askUserPromptUpdate != nil {
            return .refreshAskUserPrompts
        }
        switch signal.kind {
        case .failed:
            let detail = signal.processUpdate?.detail?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return .failure(
                message: detail.flatMap { $0.isEmpty ? nil : $0 }
                    ?? defaultFailureMessage
            )
        case .reconcile, .persisted, .completed, .cancelled:
            return .refreshHistory
        case .started, .updated, .unknown:
            return .ignore
        }
    }
}
