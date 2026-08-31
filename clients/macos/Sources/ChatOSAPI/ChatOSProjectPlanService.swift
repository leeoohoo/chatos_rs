import ChatOSCore
import Foundation

public struct ChatOSProjectPlanService: ProjectPlanServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetchPlan(projectID: String) async throws -> ProjectPlanSnapshot {
        let response: PlanDTO = try await client.request(
            "/projects/\(projectID.pathEncoded)/plan?include_archived=false&include_work_items=false"
        )
        return response.domainModel(fallbackProjectID: projectID)
    }

    public func fetchWorkItems(projectID: String, requirementID: String) async throws -> ProjectPlanSnapshot {
        let response: RequirementWorkItemsDTO = try await client.request(
            "/projects/\(projectID.pathEncoded)/requirements/\(requirementID.pathEncoded)/work-items?include_archived=false&include_dependency_graph=true"
        )
        let workItems = (response.workItems ?? []).map(\.domainModel)
        return ProjectPlanSnapshot(
            projectID: projectID,
            requirements: [],
            workItems: workItems,
            edges: response.dependencyGraph?.edges?.map(\.domainModel) ?? [],
            counts: ProjectPlanCounts.make(from: workItems)
        )
    }

    public func fetchDocuments(projectID: String, requirementID: String) async throws -> [ProjectRequirementDocument] {
        let response: [DocumentDTO] = try await client.request(
            "/projects/\(projectID.pathEncoded)/requirements/\(requirementID.pathEncoded)/documents"
        )
        return response.map(\.domainModel)
    }

    public func fetchExecution(projectID: String, requirementID: String) async throws -> ProjectRequirementExecutionLaunch? {
        let response: ProjectRequirementExecutionDTO = try await client.request(
            "/projects/\(projectID.pathEncoded)/requirements/\(requirementID.pathEncoded)/execution-plan"
        )
        guard response.found != false,
              let conversationID = response.conversationID?.nonEmpty,
              let executionGroupID = response.executionGroupID?.nonEmpty else { return nil }
        return response.domainModel(
            fallbackProjectID: projectID,
            fallbackRequirementID: requirementID,
            conversationID: conversationID,
            executionGroupID: executionGroupID
        )
    }

    public func createExecution(
        projectID: String,
        requirementID: String,
        includePrerequisiteDependents: Bool,
        planningFeedback: String?
    ) async throws -> ProjectRequirementExecutionLaunch {
        let body = try JSONEncoder().encode(
            ExecuteRequest(
                includePrerequisiteDependents: includePrerequisiteDependents,
                planningFeedback: planningFeedback?.nonEmpty
            )
        )
        let response: ProjectRequirementExecutionDTO = try await client.request(
            "/projects/\(projectID.pathEncoded)/requirements/\(requirementID.pathEncoded)/execute",
            method: "POST",
            body: body
        )
        guard let conversationID = response.conversationID?.nonEmpty,
              let executionGroupID = response.executionGroupID?.nonEmpty else {
            throw ChatOSAPIError.decoding("执行计划响应缺少 conversation_id 或 execution_group_id")
        }
        return response.domainModel(
            fallbackProjectID: projectID,
            fallbackRequirementID: requirementID,
            conversationID: conversationID,
            executionGroupID: executionGroupID
        )
    }
}

private struct PlanDTO: Decodable, Sendable {
    var projectID: String?
    var requirements: [RequirementDTO]?
    var workItems: [WorkItemDTO]?
    var workItemCounts: CountsDTO?
    var dependencyGraph: DependencyGraphDTO?

    enum CodingKeys: String, CodingKey {
        case requirements
        case projectID = "project_id"
        case workItems = "work_items"
        case workItemCounts = "work_item_counts"
        case dependencyGraph = "dependency_graph"
    }

    func domainModel(fallbackProjectID: String) -> ProjectPlanSnapshot {
        let items = (workItems ?? []).map(\.domainModel)
        return ProjectPlanSnapshot(
            projectID: projectID ?? fallbackProjectID,
            requirements: (requirements ?? []).map(\.domainModel),
            workItems: items,
            edges: dependencyGraph?.edges?.map(\.domainModel) ?? [],
            counts: workItemCounts?.domainModel ?? .make(from: items)
        )
    }
}

private struct RequirementWorkItemsDTO: Decodable, Sendable {
    var workItems: [WorkItemDTO]?
    var dependencyGraph: DependencyGraphDTO?
    enum CodingKeys: String, CodingKey {
        case workItems = "work_items"
        case dependencyGraph = "dependency_graph"
    }
}

private struct RequirementDTO: Decodable, Sendable {
    var id: String
    var projectID: String?
    var parentRequirementID: String?
    var requirementType: String?
    var title: String
    var summary: String?
    var detail: String?
    var businessValue: String?
    var acceptanceCriteria: String?
    var priority: Int?
    var status: String?
    var updatedAt: String?

    enum CodingKeys: String, CodingKey {
        case id, title, summary, detail, priority, status
        case projectID = "project_id"
        case parentRequirementID = "parent_requirement_id"
        case requirementType = "requirement_type"
        case businessValue = "business_value"
        case acceptanceCriteria = "acceptance_criteria"
        case updatedAt = "updated_at"
    }

    var domainModel: ProjectRequirement {
        ProjectRequirement(
            id: id,
            projectID: projectID,
            parentRequirementID: parentRequirementID,
            type: requirementType ?? "requirement",
            title: title,
            summary: summary,
            detail: detail,
            businessValue: businessValue,
            acceptanceCriteria: acceptanceCriteria,
            priority: priority ?? 0,
            status: status ?? "draft",
            updatedAt: APIDateParser.parse(updatedAt)
        )
    }
}

private struct WorkItemDTO: Decodable, Sendable {
    var id: String
    var requirementID: String?
    var title: String
    var description: String?
    var status: String?
    var priority: Int?
    var tags: [String]?
    var isPlanningTask: Bool?
    var dueAt: String?

    enum CodingKeys: String, CodingKey {
        case id, title, description, status, priority, tags
        case requirementID = "requirement_id"
        case isPlanningTask = "is_planning_task"
        case dueAt = "due_at"
    }

    var domainModel: ProjectWorkItem {
        ProjectWorkItem(
            id: id,
            requirementID: requirementID,
            title: title,
            detail: description,
            status: status ?? "todo",
            priority: priority ?? 0,
            tags: tags ?? [],
            isPlanningTask: isPlanningTask ?? false,
            dueAt: APIDateParser.parse(dueAt)
        )
    }
}

private struct CountsDTO: Decodable, Sendable {
    var total: Int?
    var open: Int?
    var done: Int?
    var blocked: Int?
    var domainModel: ProjectPlanCounts {
        ProjectPlanCounts(total: total ?? 0, open: open ?? 0, done: done ?? 0, blocked: blocked ?? 0)
    }
}

private struct DependencyGraphDTO: Decodable, Sendable { var edges: [EdgeDTO]? }
private struct EdgeDTO: Decodable, Sendable {
    var from: String
    var to: String
    var edgeType: String?
    enum CodingKeys: String, CodingKey { case from, to; case edgeType = "edge_type" }
    var domainModel: ProjectPlanEdge {
        ProjectPlanEdge(
            sourceID: from.removingGraphPrefix,
            targetID: to.removingGraphPrefix,
            kind: edgeType ?? "depends_on"
        )
    }
}

private struct DocumentDTO: Decodable, Sendable {
    var id: String
    var docType: String?
    var title: String?
    var format: String?
    var content: String?
    var version: Int?
    var updatedAt: String?
    enum CodingKeys: String, CodingKey {
        case id, title, format, content, version
        case docType = "doc_type"
        case updatedAt = "updated_at"
    }
    var domainModel: ProjectRequirementDocument {
        ProjectRequirementDocument(
            id: id,
            title: title ?? "未命名文档",
            type: docType ?? "document",
            format: format ?? "markdown",
            content: content ?? "",
            version: version ?? 1,
            updatedAt: APIDateParser.parse(updatedAt)
        )
    }
}

struct ProjectRequirementExecutionDTO: Decodable, Sendable {
    var found: Bool?
    var projectID: String?
    var requirementID: String?
    var conversationID: String?
    var executionGroupID: String?
    var messageID: String?
    var contactID: String?
    var status: String?
    var confirmationStatus: String?
    var hasStartedRuns: Bool?
    var taskCount: Int?
    var includePrerequisiteDependents: Bool?
    var failureKind: String?
    var failureReason: String?
    var createdAt: String?
    enum CodingKeys: String, CodingKey {
        case found
        case projectID = "project_id"
        case requirementID = "requirement_id"
        case conversationID = "conversation_id"
        case executionGroupID = "execution_group_id"
        case messageID = "message_id"
        case contactID = "contact_id"
        case status
        case confirmationStatus = "confirmation_status"
        case hasStartedRuns = "has_started_runs"
        case taskCount = "task_count"
        case includePrerequisiteDependents = "include_prerequisite_dependents"
        case failureKind = "failure_kind"
        case failureReason = "failure_reason"
        case createdAt = "created_at"
    }

    func domainModel(
        fallbackProjectID: String,
        fallbackRequirementID: String,
        conversationID: String,
        executionGroupID: String
    ) -> ProjectRequirementExecutionLaunch {
        ProjectRequirementExecutionLaunch(
            projectID: projectID ?? fallbackProjectID,
            requirementID: requirementID ?? fallbackRequirementID,
            conversationID: conversationID,
            executionGroupID: executionGroupID,
            messageID: messageID,
            confirmationStatus: confirmationStatus ?? "pending",
            hasStartedRuns: hasStartedRuns ?? false,
            overallStatus: status,
            contactID: contactID,
            taskCount: taskCount ?? 0,
            includePrerequisiteDependents: includePrerequisiteDependents ?? false,
            failureKind: failureKind,
            failureReason: failureReason,
            createdAt: APIDateParser.parse(createdAt)
        )
    }
}

private struct ExecuteRequest: Encodable {
    var includePrerequisiteDependents: Bool
    var planningFeedback: String?
    enum CodingKeys: String, CodingKey {
        case includePrerequisiteDependents = "include_prerequisite_dependents"
        case planningFeedback = "planning_feedback"
    }
}

private extension ProjectPlanCounts {
    static func make(from items: [ProjectWorkItem]) -> Self {
        let doneStatuses = Set(["done", "completed", "succeeded", "success"])
        let blockedStatuses = Set(["blocked", "failed"])
        let done = items.filter { doneStatuses.contains($0.status.lowercased()) }.count
        let blocked = items.filter { blockedStatuses.contains($0.status.lowercased()) }.count
        return .init(total: items.count, open: max(0, items.count - done), done: done, blocked: blocked)
    }
}

private extension String {
    var pathEncoded: String { addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? self }
    var removingGraphPrefix: String {
        guard let separator = firstIndex(of: ":") else { return self }
        return String(self[index(after: separator)...])
    }
    var nonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
