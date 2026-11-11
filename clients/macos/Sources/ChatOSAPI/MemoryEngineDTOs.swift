import Foundation

// Match the public Memory Engine data API, not the system-key /sdk/ DTOs.
struct MemoryThreadDTO: Decodable, Sendable {
    let id: String
    let tenant_id: String
    let source_id: String
    let subject_id: String
}
struct MemoryRecordInput: Encodable, Sendable {
    let id: String
    let role: String
    let record_type: String
    let content: String
    let structured_payload: JSONValue?
    let metadata: JSONValue?
    let created_at: String
}
struct MemoryRecordDTO: Decodable, Sendable {
    let id: String
    let thread_id: String
    let tenant_id: String
    let source_id: String
    let role: String
    let record_type: String
    let content: String
    let structured_payload: JSONValue?
    let metadata: JSONValue?
    let created_at: String
}
struct MemoryRecordEnvelope: Decodable, Sendable { let item: MemoryRecordDTO? }
struct MemorySyncRequest: Encodable, Sendable {
    let tenant_id: String
    let source_id: String
    let project_partition_key: String
    let runtime_context_assertion: String
    let records: [MemoryRecordInput]
}
struct MemoryRuntimeAssertionRequest: Encodable, Sendable {
    let runtime_kind: String
    let project_id: String?
    let runtime_id: String
    let operation: String
    let source_id: String
    let canonical_agent_subject_id: String
    let thread_id: String
}
struct MemoryRuntimeAssertionResponse: Decodable, Sendable {
    let runtime_context_assertion: String
    let source_id: String
    let canonical_agent_subject_id: String
    let thread_id: String
    let project_partition_key: String
    let runtime_identity_revision: String
}
struct MemorySyncResponse: Decodable, Sendable {
    let thread_id: String
    let received_count: Int
    let upserted_count: Int
}
struct MemoryComposeDTO: Decodable, Sendable {
    struct Block: Decodable, Sendable { let block_type: String; let text: String }
    struct Meta: Decodable, Sendable { let summary_count: Int; let recent_record_count: Int }
    let thread_id: String
    let blocks: [Block]
    let recent_records: [MemoryRecordDTO]
    let meta: Meta
}
struct MemorySummaryDTO: Decodable, Sendable {
    let thread_id: String
    let job_run_id: String?
    let accepted: Bool
    let running: Bool
    let completed: Bool
    let failed: Bool
    let generated: Bool
    let compacted: Bool
    let error_message: String?
}
