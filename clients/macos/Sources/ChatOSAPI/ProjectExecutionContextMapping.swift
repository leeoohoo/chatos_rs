import ChatOSCore
import Foundation

extension SessionMessageDTO {
    var messageTaskLookup: MessageTaskLookup? {
        let taskRunner = metadata.object(at: "task_runner_async")
        let sourceUserMessageID = taskRunner?.string(at: "source_user_message_id")
        let usableSourceID = sourceUserMessageID?.hasPrefix("temp_") == true
            ? nil
            : sourceUserMessageID
        let sourceTurnID = metadata["conversation_turn_id"]?.stringValue
            ?? taskRunner?.string(at: "source_turn_id")
        guard usableSourceID != nil || sourceTurnID != nil else { return nil }
        return MessageTaskLookup(
            sessionID: conversationID,
            turnID: sourceTurnID,
            sourceUserMessageID: usableSourceID
        )
    }

    var projectExecutionContext: ProjectExecutionContext? {
        let execution = metadata.object(at: "project_requirement_execution")
        let taskRunner = metadata.object(at: "task_runner_async")
        let mode = taskRunner?.string(at: "mode")
        let executionKind = taskRunner?.string(at: "execution_kind")
        let isProjectExecution = execution != nil
            || mode?.lowercased() == "project_requirement_execution"
            || executionKind?.lowercased() == "project_requirement_execution"
        guard isProjectExecution else { return nil }

        return ProjectExecutionContext(
            projectID: execution?.string(at: "project_id") ?? taskRunner?.string(at: "project_id"),
            requirementID: execution?.string(at: "requirement_id")
                ?? taskRunner?.string(at: "requirement_id"),
            executionGroupID: execution?.string(at: "execution_group_id")
                ?? taskRunner?.string(at: "execution_group_id"),
            replacedExecutionGroupID: execution?.string(at: "replaced_execution_group_id")
                ?? taskRunner?.string(at: "replaced_execution_group_id"),
            contactID: execution?.string(at: "contact_id") ?? taskRunner?.string(at: "contact_id"),
            mode: mode,
            executionKind: executionKind,
            confirmationStatus: taskRunner?.string(at: "confirmation_status"),
            overallStatus: taskRunner?.string(at: "overall_status")
                ?? taskRunner?.string(at: "status")
        )
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
