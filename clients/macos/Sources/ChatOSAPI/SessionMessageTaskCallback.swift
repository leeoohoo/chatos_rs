import ChatOSCore
import Foundation

extension SessionMessageDTO {
    var isTaskRunnerCallback: Bool {
        let mode = (messageMode ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        let taskRunner = metadata.object(at: "task_runner_async")
        let messageKind = taskRunner?.string(at: "message_kind")?.lowercased() ?? ""
        return mode == "task_runner_callback"
            || messageKind == "task_terminal_update"
            || messageKind == "task_lifecycle_update"
    }

    var isCancelledTaskCallback: Bool {
        let taskRunner = metadata.object(at: "task_runner_async")
        let event = taskRunner?.string(at: "event")?.lowercased()
        let status = taskRunner?.string(at: "status")?.lowercased()
        return isTaskRunnerCallback
            && (event == "task.cancelled"
                || event == "task.canceled"
                || status == "cancelled"
                || status == "canceled")
    }

    var taskRunnerCallbackReference: TaskRunnerCallbackReference? {
        guard isTaskRunnerCallback,
              let taskRunner = metadata.object(at: "task_runner_async"),
              let taskID = taskRunner.string(at: "task_id")?.trimmedNonEmpty else {
            return nil
        }
        let event = taskRunner.string(at: "event")?.trimmedNonEmpty
        let status = taskRunner.string(at: "status")?.trimmedNonEmpty
        return TaskRunnerCallbackReference(
            taskID: taskID,
            runID: taskRunner.string(at: "run_id")?.trimmedNonEmpty,
            event: event,
            status: resolvedCallbackStatus(event: event, status: status),
            sourceSessionID: taskRunner.string(at: "source_session_id")?.trimmedNonEmpty,
            sourceTurnID: taskRunner.string(at: "source_turn_id")?.trimmedNonEmpty,
            sourceUserMessageID: taskRunner.string(at: "source_user_message_id")?.trimmedNonEmpty
        )
    }

    private func resolvedCallbackStatus(event: String?, status: String?) -> String? {
        let normalizedStatus = status?.lowercased()
        switch normalizedStatus {
        case "completed", "succeeded", "success", "done": return "completed"
        case "failed", "error": return "failed"
        case "blocked": return "blocked"
        case "cancelled", "canceled", "stopped": return "cancelled"
        default: break
        }

        switch event?.lowercased() {
        case "task.completed": return "completed"
        case "task.failed": return "failed"
        case "task.blocked": return "blocked"
        case "task.cancelled", "task.canceled": return "cancelled"
        case "task.run.started", "task.started": return normalizedStatus ?? "running"
        default: return normalizedStatus
        }
    }
}

private extension Dictionary where Key == String, Value == JSONValue {
    func object(at key: String) -> [String: JSONValue]? {
        guard case let .object(value) = self[key] else { return nil }
        return value
    }

    func string(at key: String) -> String? {
        self[key]?.stringValue
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
