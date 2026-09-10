import ChatOSCore
import CryptoKit
import Foundation

public struct PluginHostIdentity: Codable, Sendable, Equatable {
    public let pluginID: String
    public let componentKey: String
    public let releaseID: String
    public let version: String
    public let artifactSHA256: String

    public init(pluginID: String, componentKey: String, releaseID: String, version: String, artifactSHA256: String) {
        self.pluginID = pluginID
        self.componentKey = componentKey
        self.releaseID = releaseID
        self.version = version
        self.artifactSHA256 = artifactSHA256
    }
}

public struct PluginHostTaskDraft: Codable, Sendable, Equatable, Identifiable {
    public let clientRef: String
    public let title: String
    public let objective: String
    public let detail: String?
    public let acceptanceCriteria: String?
    public let prerequisiteRefs: [String]

    public var id: String { clientRef }

    public init(
        clientRef: String,
        title: String,
        objective: String,
        detail: String? = nil,
        acceptanceCriteria: String? = nil,
        prerequisiteRefs: [String] = []
    ) {
        self.clientRef = clientRef
        self.title = title
        self.objective = objective
        self.detail = detail
        self.acceptanceCriteria = acceptanceCriteria
        self.prerequisiteRefs = prerequisiteRefs
    }
}

public struct PluginHostTaskBatchRequest: Codable, Sendable, Equatable {
    public let idempotencyKey: String
    public let tasks: [PluginHostTaskDraft]

    public init(idempotencyKey: String, tasks: [PluginHostTaskDraft]) {
        self.idempotencyKey = idempotencyKey
        self.tasks = tasks
    }
}

public struct PluginHostTaskReference: Codable, Sendable, Equatable, Identifiable {
    public let clientRef: String
    public let taskID: String
    public let title: String
    public let status: String
    public let lastRunID: String?
    public let updatedAt: String

    public var id: String { taskID }
}

public struct PluginHostTaskBatch: Codable, Sendable, Equatable, Identifiable {
    public let batchID: String
    public let reused: Bool
    public let tasks: [PluginHostTaskReference]

    public var id: String { batchID }
}

public struct PluginHostTaskRunResult: Codable, Sendable, Equatable {
    public let taskID: String
    public let ok: Bool
    public let message: String?
    public let runID: String?
}

public actor ChatOSTaskRunnerHostService {
    private let client: ChatOSAPIClient
    private let encoder: JSONEncoder

    public init(client: ChatOSAPIClient) {
        self.client = client
        encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
    }

    public func prepareBatch(
        _ request: PluginHostTaskBatchRequest,
        project: ProjectContextSnapshot,
        host: PluginHostIdentity,
        defaultModelConfigID: String
    ) async throws -> PluginHostTaskBatch {
        try validate(
            request: request,
            project: project,
            host: host,
            defaultModelConfigID: defaultModelConfigID
        )
        let ordered = try topologicalOrder(request.tasks)
        let identityDigest = try digest(BatchIdentity(
            schemaVersion: 1,
            plugin: host,
            projectID: project.projectId,
            projectRevision: project.projectRevision,
            idempotencyKey: request.idempotencyKey
        ))
        let contentDigest = try digest(BatchContent(
            identityDigest: identityDigest,
            defaultModelConfigID: defaultModelConfigID,
            tasks: request.tasks
        ))
        let batchID = "host-batch-\(identityDigest.prefix(32))"
        let batchTag = "chatos-host-batch:\(identityDigest.prefix(32))"
        let existing = try await listTasks(projectID: project.projectId, tag: batchTag)
        var byRef: [String: TaskRecordDTO] = [:]
        for task in existing {
            guard let bridge = task.inputPayload?.objectValue?["hostBridge"]?.objectValue,
                  bridge["batchId"]?.stringValue == batchID,
                  bridge["batchDigest"]?.stringValue == contentDigest,
                  let clientRef = bridge["clientRef"]?.stringValue else {
                throw ChatOSAPIError.invalidRequest("Task Runner 中存在冲突的宿主批次记录")
            }
            guard byRef.updateValue(task, forKey: clientRef) == nil else {
                throw ChatOSAPIError.invalidRequest("Task Runner 中存在重复的宿主任务引用")
            }
        }

        var reused = !existing.isEmpty
        for draft in ordered {
            let prerequisiteIDs = try draft.prerequisiteRefs.map { reference in
                guard let id = byRef[reference]?.id else {
                    throw ChatOSAPIError.invalidRequest("任务依赖尚未创建：\(reference)")
                }
                return id
            }
            if let current = byRef[draft.clientRef] {
                guard current.projectID == project.projectId,
                      current.defaultModelConfigID == defaultModelConfigID,
                      Set(current.prerequisiteTaskIDs) == Set(prerequisiteIDs) else {
                    throw ChatOSAPIError.invalidRequest("幂等任务与当前项目、模型或依赖图不一致")
                }
                continue
            }
            reused = false
            let payload = CreateTaskDTO(
                title: draft.title,
                description: draft.detail,
                objective: draft.objective,
                inputPayload: .object([
                    "hostBridge": .object([
                        "schemaVersion": .number(1),
                        "batchId": .string(batchID),
                        "batchDigest": .string(contentDigest),
                        "clientRef": .string(draft.clientRef),
                        "pluginId": .string(host.pluginID),
                        "componentKey": .string(host.componentKey),
                        "releaseId": .string(host.releaseID),
                    ]),
                    "pluginPayload": .object([
                        "detail": draft.detail.map(JSONValue.string) ?? .null,
                        "acceptanceCriteria": draft.acceptanceCriteria.map(JSONValue.string) ?? .null,
                    ]),
                ]),
                status: "ready",
                tags: [batchTag, "chatos-plugin:\(safeTag(host.pluginID))"],
                defaultModelConfigID: defaultModelConfigID,
                projectID: project.projectId,
                projectContext: project,
                prerequisiteTaskIDs: prerequisiteIDs
            )
            let body = try encoder.encode(payload)
            let created: TaskRecordDTO = try await client.request(
                "tasks",
                method: "POST",
                body: body,
                timeoutInterval: 60,
                service: .taskRunner
            )
            guard created.projectID == project.projectId else {
                throw ChatOSAPIError.invalidRequest("Task Runner 返回了错误的项目任务")
            }
            byRef[draft.clientRef] = created
        }
        let references = try request.tasks.map { draft -> PluginHostTaskReference in
            guard let task = byRef[draft.clientRef] else {
                throw ChatOSAPIError.invalidRequest("Task Runner 未返回完整任务批次")
            }
            return task.reference(clientRef: draft.clientRef)
        }
        return .init(batchID: batchID, reused: reused, tasks: references)
    }

    public func taskStatuses(taskIDs: [String], projectID: String) async throws -> [PluginHostTaskReference] {
        let ids = try validatedTaskIDs(taskIDs)
        guard !projectID.isEmpty else { throw ChatOSAPIError.invalidRequest("缺少项目上下文") }
        guard !ids.isEmpty else { return [] }
        let query = "tasks/summaries?ids=\(query(ids.joined(separator: ",")))&project_id=\(query(projectID))"
        let tasks: [TaskSummaryDTO] = try await client.request(query, service: .taskRunner)
        let returned = Set(tasks.map(\.id))
        guard returned == Set(ids), tasks.allSatisfy({ $0.projectID == projectID }) else {
            throw ChatOSAPIError.invalidRequest("部分任务不存在、不属于当前项目或不可访问")
        }
        return tasks.map { $0.reference(clientRef: "") }
    }

    public func startBatch(taskIDs: [String], projectID: String) async throws -> [PluginHostTaskRunResult] {
        let ids = try validatedTaskIDs(taskIDs)
        _ = try await taskStatuses(taskIDs: ids, projectID: projectID)
        let body = try encoder.encode(BatchRunDTO(taskIDs: ids))
        let result: BatchRunResponseDTO = try await client.request(
            "tasks/batch/runs", method: "POST", body: body, timeoutInterval: 60, service: .taskRunner
        )
        return result.results.map { .init(taskID: $0.taskID, ok: $0.ok, message: $0.message, runID: $0.runID) }
    }

    private func listTasks(projectID: String, tag: String) async throws -> [TaskRecordDTO] {
        let endpoint = "tasks?project_scope=project&project_id=\(query(projectID))&tag=\(query(tag))&limit=100"
        return try await client.request(endpoint, service: .taskRunner)
    }

    private func validate(
        request: PluginHostTaskBatchRequest,
        project: ProjectContextSnapshot,
        host: PluginHostIdentity,
        defaultModelConfigID: String
    ) throws {
        guard request.tasks.count >= 1, request.tasks.count <= 50,
              validText(request.idempotencyKey, max: 256),
              validText(defaultModelConfigID, max: 256),
              validText(project.projectId, max: 256), project.schemaVersion == 1,
              [host.pluginID, host.componentKey, host.releaseID, host.version, host.artifactSHA256]
                .allSatisfy({ validText($0, max: 512) }) else {
            throw ChatOSAPIError.invalidRequest("任务批次参数无效")
        }
        var refs = Set<String>()
        for task in request.tasks {
            guard validRef(task.clientRef), refs.insert(task.clientRef).inserted,
                  validText(task.title, max: 240), validText(task.objective, max: 200_000),
                  task.prerequisiteRefs.count <= 50,
                  task.prerequisiteRefs.allSatisfy(validRef) else {
                throw ChatOSAPIError.invalidRequest("任务草稿或依赖引用无效")
            }
        }
        guard request.tasks.flatMap(\.prerequisiteRefs).allSatisfy(refs.contains) else {
            throw ChatOSAPIError.invalidRequest("任务依赖引用不在当前批次")
        }
    }

    private func topologicalOrder(_ tasks: [PluginHostTaskDraft]) throws -> [PluginHostTaskDraft] {
        let byRef = Dictionary(uniqueKeysWithValues: tasks.map { ($0.clientRef, $0) })
        var visiting = Set<String>(), visited = Set<String>(), result: [PluginHostTaskDraft] = []
        func visit(_ ref: String) throws {
            if visited.contains(ref) { return }
            guard visiting.insert(ref).inserted else { throw ChatOSAPIError.invalidRequest("任务依赖图存在循环") }
            guard let task = byRef[ref] else { throw ChatOSAPIError.invalidRequest("任务依赖引用不存在") }
            for dependency in task.prerequisiteRefs { try visit(dependency) }
            visiting.remove(ref); visited.insert(ref); result.append(task)
        }
        for task in tasks { try visit(task.clientRef) }
        return result
    }

    private func validatedTaskIDs(_ values: [String]) throws -> [String] {
        let ids = values.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
        guard ids.count <= 200, Set(ids).count == ids.count, ids.allSatisfy(validRef) else {
            throw ChatOSAPIError.invalidRequest("Task Runner 引用无效")
        }
        return ids
    }

    private func digest<T: Encodable>(_ value: T) throws -> String {
        SHA256.hash(data: try encoder.encode(value)).map { String(format: "%02x", $0) }.joined()
    }

    private func safeTag(_ value: String) -> String {
        SHA256.hash(data: Data(value.utf8)).prefix(16).map { String(format: "%02x", $0) }.joined()
    }

    private func query(_ value: String) -> String {
        value.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed.subtracting(CharacterSet(charactersIn: "&=+?#"))) ?? ""
    }

    private func validText(_ value: String, max: Int) -> Bool {
        !value.isEmpty && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && value.utf8.count <= max && value.unicodeScalars.allSatisfy { !CharacterSet.controlCharacters.contains($0) }
    }

    private func validRef(_ value: String) -> Bool {
        validText(value, max: 256) && !value.contains("/") && !value.contains("\\") && value != "." && value != ".."
    }
}

private struct BatchIdentity: Codable { let schemaVersion: Int; let plugin: PluginHostIdentity; let projectID: String; let projectRevision: Int64; let idempotencyKey: String }
private struct BatchContent: Codable {
    let identityDigest: String
    let defaultModelConfigID: String
    let tasks: [PluginHostTaskDraft]
}

private struct CreateTaskDTO: Encodable {
    let title: String
    let description: String?
    let objective: String
    let inputPayload: JSONValue
    let status: String
    let tags: [String]
    let defaultModelConfigID: String
    let projectID: String
    let projectContext: ProjectContextSnapshot
    let prerequisiteTaskIDs: [String]

    enum CodingKeys: String, CodingKey {
        case title, description, objective, status, tags
        case defaultModelConfigID = "default_model_config_id"
        case inputPayload = "input_payload"
        case projectID = "project_id"
        case projectContext = "project_context"
        case prerequisiteTaskIDs = "prerequisite_task_ids"
    }
}

private struct TaskRecordDTO: Decodable {
    let id: String; let title: String; let status: String; let projectID: String?
    let inputPayload: JSONValue?; let prerequisiteTaskIDs: [String]; let defaultModelConfigID: String?
    let lastRunID: String?; let updatedAt: String
    enum CodingKeys: String, CodingKey {
        case id, title, status
        case projectID = "project_id"; case inputPayload = "input_payload"
        case prerequisiteTaskIDs = "prerequisite_task_ids"
        case defaultModelConfigID = "default_model_config_id"
        case lastRunID = "last_run_id"; case updatedAt = "updated_at"
    }
    func reference(clientRef: String) -> PluginHostTaskReference {
        .init(clientRef: clientRef, taskID: id, title: title, status: status, lastRunID: lastRunID, updatedAt: updatedAt)
    }
}

private struct TaskSummaryDTO: Decodable {
    let id: String; let title: String; let status: String; let projectID: String?; let lastRunID: String?; let updatedAt: String
    enum CodingKeys: String, CodingKey {
        case id, title, status; case projectID = "project_id"; case lastRunID = "last_run_id"; case updatedAt = "updated_at"
    }
    func reference(clientRef: String) -> PluginHostTaskReference {
        .init(clientRef: clientRef, taskID: id, title: title, status: status, lastRunID: lastRunID, updatedAt: updatedAt)
    }
}

private struct BatchRunDTO: Encodable { let taskIDs: [String]; enum CodingKeys: String, CodingKey { case taskIDs = "task_ids" } }
private struct BatchRunResponseDTO: Decodable { let results: [BatchRunItemDTO] }
private struct BatchRunItemDTO: Decodable {
    let taskID: String; let ok: Bool; let message: String?; let runID: String?
    enum CodingKeys: String, CodingKey { case taskID = "task_id"; case ok, message; case runID = "run_id" }
}

private extension JSONValue {
    var objectValue: [String: JSONValue]? { if case let .object(value) = self { value } else { nil } }
}
