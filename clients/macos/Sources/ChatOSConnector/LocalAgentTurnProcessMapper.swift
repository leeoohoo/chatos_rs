import ChatOSCore
import Foundation

enum LocalAgentTurnProcessMapper {
    static func map(_ events: [LocalAgentEventRecord]) -> [TurnProcessNode] {
        var nodes: [TurnProcessNode] = []
        var toolNodeByCallID: [String: Int] = [:]
        for event in events.sorted(by: { $0.cursor < $1.cursor }) {
            if event.eventType == "tool_batch_requested" {
                appendToolCalls(event, nodes: &nodes, indexes: &toolNodeByCallID)
                continue
            }
            if event.eventType == "tool_invocation_completed" {
                completeToolCall(event, nodes: &nodes, indexes: toolNodeByCallID)
                continue
            }
            if let node = node(event) {
                nodes.append(node)
            }
        }
        return nodes
    }

    private static func appendToolCalls(
        _ event: LocalAgentEventRecord,
        nodes: inout [TurnProcessNode],
        indexes: inout [String: Int]
    ) {
        let calls = event.payload?.object?["tool_calls"]?.array ?? []
        for (index, value) in calls.enumerated() {
            guard let call = value.object else { continue }
            let callID = call["call_id"]?.string ?? "\(event.eventID)-\(index)"
            let toolName = call["tool_name"]?.string ?? "local_tool"
            indexes[callID] = nodes.count
            nodes.append(.init(
                id: "local-tool:\(event.runID):\(callID)",
                title: "调用工具 · \(displayName(toolName))",
                detail: nil,
                status: .streaming,
                kind: .tool,
                timestamp: date(event.createdAtUnixMs)
            ))
        }
    }

    private static func completeToolCall(
        _ event: LocalAgentEventRecord,
        nodes: inout [TurnProcessNode],
        indexes: [String: Int]
    ) {
        guard let payload = event.payload?.object,
              let callID = payload["call_id"]?.string,
              let index = indexes[callID], nodes.indices.contains(index) else { return }
        let rawStatus = payload["status"]?.string ?? "failed"
        nodes[index].status = status(rawStatus)
        nodes[index].detail = rawStatus == "succeeded" ? "本地工具执行完成" : "本地工具执行未完成"
    }

    private static func node(_ event: LocalAgentEventRecord) -> TurnProcessNode? {
        let payload = event.payload?.object ?? [:]
        let mapped: (String, String?, TurnStatus, TurnProcessNode.Kind)? = switch event.eventType {
        case "conversation_turn_started": ("开始处理", nil, .queued, .update)
        case "run_claimed": ("本地模型开始推理", nil, .streaming, .reasoning)
        case "continuation_requested": ("继续推理", nil, .streaming, .reasoning)
        case "retry_scheduled": (
            "等待重试",
            safeDetail(payload["reason"]?.string),
            .queued,
            .update
        )
        case "user_input_requested": ("等待你的回复", nil, .streaming, .update)
        case "conversation_turn_resumed", "run_resumed": (
            "已收到回复，继续执行",
            nil,
            .streaming,
            .update
        )
        case "conversation_guidance_queued": ("已追加指导", nil, .streaming, .update)
        case "tool_invocation_approved": ("工具调用已批准", nil, .streaming, .tool)
        case "tool_invocation_rejected": ("工具调用被拒绝", nil, .failed, .tool)
        case "tool_claim_expired_requeued": ("工具执行已重新排队", nil, .queued, .tool)
        case "tool_claim_expired_needs_review": (
            "工具执行状态需要检查",
            nil,
            .failed,
            .tool
        )
        case "run_paused": ("执行已暂停", safeDetail(payload["reason"]?.string), .streaming, .update)
        case "run_needs_review", "claim_expired_needs_review": (
            "执行状态需要检查",
            safeDetail(payload["reason"]?.string),
            .failed,
            .update
        )
        case "task_graph_written_back": (
            "本地任务已汇总",
            nil,
            status(payload["status"]?.string ?? "succeeded"),
            .task
        )
        case "run_succeeded": ("本地执行完成", nil, .completed, .update)
        case "run_failed": (
            "本地执行失败",
            safeDetail(payload["error"]?.string),
            .failed,
            .update
        )
        case "run_cancelled": (
            "本地执行已取消",
            safeDetail(payload["reason"]?.string),
            .cancelled,
            .update
        )
        default: nil
        }
        guard let mapped else { return nil }
        return .init(
            id: event.eventID,
            title: mapped.0,
            detail: mapped.1,
            status: mapped.2,
            kind: mapped.3,
            timestamp: date(event.createdAtUnixMs)
        )
    }

    private static func status(_ value: String) -> TurnStatus {
        switch value.lowercased() {
        case "succeeded", "completed", "ok": .completed
        case "failed", "error", "rejected": .failed
        case "cancelled", "canceled": .cancelled
        case "queued", "pending": .queued
        default: .streaming
        }
    }

    private static func displayName(_ value: String) -> String {
        value.replacingOccurrences(of: "__", with: " · ")
            .replacingOccurrences(of: "_", with: " ")
    }

    private static func safeDetail(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else { return nil }
        let lowered = value.lowercased()
        let sensitive = ["authorization", "api_key", "apikey", "access_token", "password"]
        guard !sensitive.contains(where: lowered.contains) else { return "详细信息已隐藏" }
        return String(value.prefix(1_000))
    }

    private static func date(_ unixMilliseconds: Int64) -> Date {
        Date(timeIntervalSince1970: Double(unixMilliseconds) / 1_000)
    }
}

private extension LocalAgentJSONValue {
    var object: [String: LocalAgentJSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }

    var array: [LocalAgentJSONValue]? {
        guard case let .array(value) = self else { return nil }
        return value
    }

    var string: String? {
        guard case let .string(value) = self else { return nil }
        return value
    }
}
