import ChatOSCore
import Foundation

public struct ChatOSTurnProcessService: TurnProcessServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetchProcessNodes(
        sessionID: String,
        turnID: String
    ) async throws -> [TurnProcessNode] {
        let messages: [SessionMessageDTO] = try await client.request(
            "/conversations/\(sessionID.urlPathEncoded)/turns/by-turn/\(turnID.urlPathEncoded)/messages"
        )
        return messages
            .filter { $0.isTurnProcessMessage && !$0.isTaskRunnerCallback }
            .map(\.processNode)
            .sorted { left, right in
                (left.timestamp ?? .distantPast) < (right.timestamp ?? .distantPast)
            }
    }
}

private extension SessionMessageDTO {
    var isTurnProcessMessage: Bool {
        if role == "user" { return false }
        if role == "tool" || !toolCalls.isEmpty { return true }
        if metadata["historyProcessLoaded"]?.boolValue == true { return true }
        if metadata["historyProcessUserMessageId"]?.stringValue != nil { return true }
        if metadata["task_runner_async"] != nil || metadata["task_runner_callback"] != nil {
            return true
        }
        let mode = (messageMode ?? messageSource ?? "").lowercased()
        return mode.contains("tool") || mode.contains("task") || mode.contains("reason")
    }

    var processNode: TurnProcessNode {
        let toolNames = toolCalls.compactMap(\.toolName)
        let mappedContent = TurnProcessContentMapper.map(self)
        let kind: TurnProcessNode.Kind
        let title: String

        if role == "tool", let mappedContent, mappedContent.kind == .task {
            kind = mappedContent.kind
            title = mappedContent.title
        } else if role == "tool" {
            kind = .tool
            title = metadata.value(at: "tool_name")?.stringValue ?? "工具返回"
        } else if !toolNames.isEmpty {
            kind = .tool
            title = "调用工具 · \(toolNames.joined(separator: "、"))"
        } else if let mappedContent {
            kind = mappedContent.kind
            title = mappedContent.title
        } else if metadata["task_runner_async"] != nil || metadata["task_runner_callback"] != nil {
            kind = .task
            title = firstContentLine ?? "任务状态更新"
        } else if (messageMode ?? messageSource ?? "").lowercased().contains("reason") {
            kind = .reasoning
            title = "推理"
        } else {
            kind = .update
            title = firstContentLine ?? "过程更新"
        }

        return TurnProcessNode(
            id: id,
            title: title,
            detail: mappedContent?.detail
                ?? (content.trimmedNonEmptyValue == title ? nil : content.trimmedNonEmptyValue),
            status: mappedContent?.status ?? processStatus,
            kind: kind,
            timestamp: DateParser.parse(createdAt)
        )
    }

    var processStatus: TurnStatus {
        switch status?.lowercased() {
        case "failed", "error": .failed
        case "cancelled", "canceled": .cancelled
        case "completed", "complete", "success", "succeeded": .completed
        case "queued", "pending": .queued
        default: .streaming
        }
    }

}

private extension JSONValue {
    var toolName: String? {
        guard case let .object(call) = self else { return nil }
        if let direct = call["name"]?.stringValue { return direct }
        guard case let .object(function) = call["function"] else { return nil }
        return function["name"]?.stringValue
    }
}
