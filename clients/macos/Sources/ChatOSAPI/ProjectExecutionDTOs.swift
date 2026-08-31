import ChatOSCore
import Foundation

struct ProjectExecutionActionResponseDTO: Decodable, Sendable {
    var success: Bool?
    var status: String?
    var executionGroupID: String?
    var taskIDs: [String]?
    var rootTaskIDs: [String]?
    var discardedTasks: Bool?

    enum CodingKeys: String, CodingKey {
        case success
        case status
        case executionGroupID = "execution_group_id"
        case taskIDs = "task_ids"
        case rootTaskIDs = "root_task_ids"
        case discardedTasks = "discarded_tasks"
    }

    var model: ProjectExecutionActionResult {
        ProjectExecutionActionResult(
            success: success ?? true,
            status: status,
            executionGroupID: executionGroupID,
            taskIDs: taskIDs ?? [],
            rootTaskIDs: rootTaskIDs ?? [],
            discardedTasks: discardedTasks
        )
    }
}

struct ProjectExecutionActionRequestDTO: Encodable {
    var executionGroupID: String
    var conversationID: String
    var contactID: String?
    var discardTasks: Bool?

    enum CodingKeys: String, CodingKey {
        case executionGroupID = "execution_group_id"
        case conversationID = "conversation_id"
        case contactID = "contact_id"
        case discardTasks = "discard_tasks"
    }
}
