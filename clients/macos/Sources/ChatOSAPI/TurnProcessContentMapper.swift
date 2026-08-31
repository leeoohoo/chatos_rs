import ChatOSCore
import Foundation

struct MappedProcessContent {
    var title: String
    var detail: String?
    var kind: TurnProcessNode.Kind
    var status: TurnStatus?
}

enum TurnProcessContentMapper {
    static func map(_ message: SessionMessageDTO) -> MappedProcessContent? {
        if let structured = structuredResult(from: message.content) {
            return mapStructuredResult(structured)
        }

        guard let title = message.firstContentLine else { return nil }
        return MappedProcessContent(
            title: title,
            detail: detailAfterFirstLine(message.content),
            kind: message.role == "tool" ? .tool : .update,
            status: nil
        )
    }

    private static func mapStructuredResult(
        _ object: [String: JSONValue]
    ) -> MappedProcessContent {
        let statusText = object["status"]?.stringValue
        let taskTitle = object["title"]?.stringValue
        let localizedMessage = object["message_zh"]?.stringValue
        let accepted = object["accepted"]?.boolValue == true

        let title: String
        if let taskTitle {
            title = "任务 · \(taskTitle)"
        } else if accepted {
            title = "任务已提交"
        } else {
            title = "任务状态更新"
        }

        let detail = object["result_summary"]?.stringValue
            ?? localizedMessage
            ?? object["description"]?.stringValue
            ?? object["objective"]?.stringValue

        return MappedProcessContent(
            title: title,
            detail: detail,
            kind: .task,
            status: statusText.map(mapStatus)
        )
    }

    private static func structuredResult(
        from content: String
    ) -> [String: JSONValue]? {
        guard let data = content.data(using: .utf8),
              let root = try? JSONDecoder().decode(JSONValue.self, from: data),
              case let .object(object) = root else { return nil }
        if case let .object(structured) = object["_structured_result"] {
            return structured
        }
        return object
    }

    private static func detailAfterFirstLine(_ content: String) -> String? {
        let lines = content.split(omittingEmptySubsequences: false, whereSeparator: \.isNewline)
        guard lines.count > 1 else { return nil }
        return lines.dropFirst()
            .map(String.init)
            .joined(separator: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmedNonEmptyValue
    }

    private static func mapStatus(_ status: String) -> TurnStatus {
        switch status.lowercased() {
        case "failed", "error": .failed
        case "cancelled", "canceled": .cancelled
        case "completed", "complete", "success", "succeeded": .completed
        case "queued", "pending": .queued
        default: .streaming
        }
    }
}

extension SessionMessageDTO {
    var firstContentLine: String? {
        content
            .split(whereSeparator: \.isNewline)
            .first
            .map(String.init)?
            .trimmedNonEmptyValue
    }
}
