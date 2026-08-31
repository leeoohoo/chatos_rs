import ChatOSCore
import Foundation

struct MessageTaskGraphResponseDTO: Decodable, Sendable {
    var rootTaskIDs: [String]
    var nodes: [MessageTaskGraphNodeDTO]
    var edges: [MessageTaskGraphEdgeDTO]
    var sourceSessionID: String?
    var sourceTurnID: String?
    var sourceUserMessageID: String?

    enum CodingKeys: String, CodingKey {
        case nodes, edges
        case rootTaskIDs = "root_task_ids"
        case sourceSessionID = "source_session_id"
        case sourceTurnID = "source_turn_id"
        case sourceUserMessageID = "source_user_message_id"
    }
}

struct MessageTaskGraphNodeDTO: Decodable, Sendable {
    var task: MessageTaskDTO
    var depth: Int
    var isRoot: Bool
    var isCurrentMessage: Bool

    enum CodingKeys: String, CodingKey {
        case task, depth
        case isRoot = "is_root"
        case isCurrentMessage = "is_current_message"
    }
}

struct MessageTaskGraphEdgeDTO: Decodable, Sendable {
    var id: String
    var source: String
    var target: String
    var kind: String?
}

struct MessageTaskReferenceDTO: Decodable, Sendable {
    var id: String
    var title: String?
    var status: String?

    var model: MessageTaskReference {
        MessageTaskReference(
            id: id,
            title: title?.trimmedNonEmptyValue,
            status: status?.trimmedNonEmptyValue
        )
    }
}

struct MessageTaskModelConfigSummaryDTO: Decodable, Sendable {
    var id: String
    var name: String?
    var provider: String?
    var model: String?

    var domainModel: MessageTaskModelConfigSummary {
        MessageTaskModelConfigSummary(
            id: id,
            name: name?.trimmedNonEmptyValue,
            provider: provider?.trimmedNonEmptyValue,
            model: model?.trimmedNonEmptyValue
        )
    }
}

struct MessageTaskDTO: Decodable, Sendable {
    var id: String
    var title: String
    var description: String?
    var objective: String?
    var status: String?
    var priority: Int?
    var tags: [String]
    var defaultModelConfigID: String?
    var defaultModelConfig: MessageTaskModelConfigSummaryDTO?
    var creatorUserID: String?
    var creatorUsername: String?
    var creatorDisplayName: String?
    var resultSummary: String?
    var processLog: String?
    var lastRunID: String?
    var lastRun: MessageTaskRunSummaryDTO?
    var schedule: JSONValue?
    var parentTaskID: String?
    var parentTask: MessageTaskReferenceDTO?
    var sourceRunID: String?
    var sourceRun: MessageTaskRunSummaryDTO?
    var sourceSessionID: String?
    var sourceTurnID: String?
    var sourceUserMessageID: String?
    var prerequisiteTaskIDs: [String]
    var prerequisiteTasks: [MessageTaskReferenceDTO]
    var projectTaskID: String?
    var executionClientRef: String?
    var dependencyContextRefs: [String]
    var inputPayload: JSONValue?
    var taskToolState: JSONValue?
    var mcpConfig: JSONValue?
    var createdAt: String?
    var updatedAt: String?

    enum CodingKeys: String, CodingKey {
        case id, title, description, objective, status, priority, tags
        case defaultModelConfigID = "default_model_config_id"
        case defaultModelConfig = "default_model_config"
        case creatorUserID = "creator_user_id"
        case creatorUsername = "creator_username"
        case creatorDisplayName = "creator_display_name"
        case resultSummary = "result_summary"
        case processLog = "process_log"
        case lastRunID = "last_run_id"
        case lastRun = "last_run"
        case schedule
        case parentTaskID = "parent_task_id"
        case parentTask = "parent_task"
        case sourceRunID = "source_run_id"
        case sourceRun = "source_run"
        case sourceSessionID = "source_session_id"
        case sourceTurnID = "source_turn_id"
        case sourceUserMessageID = "source_user_message_id"
        case prerequisiteTaskIDs = "prerequisite_task_ids"
        case prerequisiteTasks = "prerequisite_tasks"
        case projectTaskID = "project_task_id"
        case executionClientRef = "execution_client_ref"
        case dependencyContextRefs = "dependency_context_refs"
        case inputPayload = "input_payload"
        case taskToolState = "task_tool_state"
        case mcpConfig = "mcp_config"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        title = try container.decodeIfPresent(String.self, forKey: .title) ?? id
        description = try container.decodeIfPresent(String.self, forKey: .description)
        objective = try container.decodeIfPresent(String.self, forKey: .objective)
        status = try container.decodeIfPresent(String.self, forKey: .status)
        priority = try container.decodeIfPresent(Int.self, forKey: .priority)
        tags = try container.decodeIfPresent([String].self, forKey: .tags) ?? []
        defaultModelConfigID = try container.decodeIfPresent(String.self, forKey: .defaultModelConfigID)
        defaultModelConfig = try container.decodeIfPresent(MessageTaskModelConfigSummaryDTO.self, forKey: .defaultModelConfig)
        creatorUserID = try container.decodeIfPresent(String.self, forKey: .creatorUserID)
        creatorUsername = try container.decodeIfPresent(String.self, forKey: .creatorUsername)
        creatorDisplayName = try container.decodeIfPresent(String.self, forKey: .creatorDisplayName)
        resultSummary = try container.decodeIfPresent(String.self, forKey: .resultSummary)
        processLog = try container.decodeIfPresent(String.self, forKey: .processLog)
        lastRunID = try container.decodeIfPresent(String.self, forKey: .lastRunID)
        lastRun = try container.decodeIfPresent(MessageTaskRunSummaryDTO.self, forKey: .lastRun)
        schedule = try container.decodeIfPresent(JSONValue.self, forKey: .schedule)
        parentTaskID = try container.decodeIfPresent(String.self, forKey: .parentTaskID)
        parentTask = try container.decodeIfPresent(MessageTaskReferenceDTO.self, forKey: .parentTask)
        sourceRunID = try container.decodeIfPresent(String.self, forKey: .sourceRunID)
        sourceRun = try container.decodeIfPresent(MessageTaskRunSummaryDTO.self, forKey: .sourceRun)
        sourceSessionID = try container.decodeIfPresent(String.self, forKey: .sourceSessionID)
        sourceTurnID = try container.decodeIfPresent(String.self, forKey: .sourceTurnID)
        sourceUserMessageID = try container.decodeIfPresent(String.self, forKey: .sourceUserMessageID)
        prerequisiteTaskIDs = try container.decodeIfPresent([String].self, forKey: .prerequisiteTaskIDs) ?? []
        prerequisiteTasks = try container.decodeIfPresent([MessageTaskReferenceDTO].self, forKey: .prerequisiteTasks) ?? []
        projectTaskID = try container.decodeIfPresent(String.self, forKey: .projectTaskID)
        executionClientRef = try container.decodeIfPresent(String.self, forKey: .executionClientRef)
        dependencyContextRefs = try container.decodeIfPresent([String].self, forKey: .dependencyContextRefs) ?? []
        inputPayload = try container.decodeIfPresent(JSONValue.self, forKey: .inputPayload)
        taskToolState = try container.decodeIfPresent(JSONValue.self, forKey: .taskToolState)
        mcpConfig = try container.decodeIfPresent(JSONValue.self, forKey: .mcpConfig)
        createdAt = try container.decodeIfPresent(String.self, forKey: .createdAt)
        updatedAt = try container.decodeIfPresent(String.self, forKey: .updatedAt)
    }
}

extension MessageTaskDTO {
    var model: MessageTask {
        let input = inputPayload?.objectValue
        return MessageTask(
            id: id,
            title: title,
            description: description?.trimmedNonEmptyValue,
            objective: objective?.trimmedNonEmptyValue,
            status: status?.trimmedNonEmptyValue,
            priority: priority,
            tags: tags,
            defaultModelConfigID: defaultModelConfigID?.trimmedNonEmptyValue,
            defaultModelConfig: defaultModelConfig?.domainModel,
            creatorUserID: creatorUserID?.trimmedNonEmptyValue,
            creatorUsername: creatorUsername?.trimmedNonEmptyValue,
            creatorDisplayName: creatorDisplayName?.trimmedNonEmptyValue,
            resultSummary: resultSummary?.trimmedNonEmptyValue,
            processLog: processLog?.trimmedNonEmptyValue,
            lastRunID: lastRunID?.trimmedNonEmptyValue ?? lastRun?.id,
            lastRunStatus: lastRun?.status?.trimmedNonEmptyValue,
            lastRun: lastRun?.modelSummary,
            parentTaskID: parentTaskID?.trimmedNonEmptyValue,
            parentTask: parentTask?.model,
            sourceRunID: sourceRunID?.trimmedNonEmptyValue,
            sourceRun: sourceRun?.modelSummary,
            sourceSessionID: sourceSessionID?.trimmedNonEmptyValue,
            sourceTurnID: sourceTurnID?.trimmedNonEmptyValue,
            sourceUserMessageID: sourceUserMessageID?.trimmedNonEmptyValue,
            prerequisiteTaskIDs: prerequisiteTaskIDs,
            prerequisiteTasks: prerequisiteTasks.map(\.model),
            projectTaskID: input?["project_task_id"]?.stringValue
                ?? projectTaskID?.trimmedNonEmptyValue,
            executionClientRef: input?["execution_client_ref"]?.stringValue
                ?? executionClientRef?.trimmedNonEmptyValue,
            dependencyContextRefs: input?["dependency_context_refs"]?.stringArrayValue
                ?? dependencyContextRefs,
            scheduleJSON: schedule?.prettyPrintedString,
            taskToolStateJSON: taskToolState?.prettyPrintedString,
            mcpConfigJSON: mcpConfig?.prettyPrintedString,
            inputPayloadJSON: inputPayload?.prettyPrintedString,
            createdAt: DateParser.parse(createdAt),
            updatedAt: DateParser.parse(updatedAt)
        )
    }
}

extension MessageTaskGraphResponseDTO {
    var model: MessageTaskGraphSnapshot {
        MessageTaskGraphSnapshot(
            rootTaskIDs: rootTaskIDs,
            nodes: nodes.map {
                MessageTaskGraphNode(
                    task: $0.task.model,
                    depth: $0.depth,
                    isRoot: $0.isRoot,
                    isCurrentMessage: $0.isCurrentMessage
                )
            },
            edges: edges.map {
                MessageTaskGraphEdge(
                    id: $0.id,
                    sourceID: $0.source,
                    targetID: $0.target,
                    kind: $0.kind ?? "prerequisite"
                )
            },
            sourceSessionID: sourceSessionID,
            sourceTurnID: sourceTurnID,
            sourceUserMessageID: sourceUserMessageID
        )
    }
}

private extension JSONValue {
    var objectValue: [String: JSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }

    var stringArrayValue: [String]? {
        guard case let .array(values) = self else { return nil }
        return values.compactMap(\.stringValue)
    }
}
