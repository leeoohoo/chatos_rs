import ChatOSCore
import Foundation

struct RealtimeEventEnvelopeDTO: Decodable, Sendable {
    var type: String
    var event: String
    var eventID: String
    var eventSequence: Int64
    var conversationID: String?
    var payload: RealtimePayloadDTO?
    var timestamp: String

    enum CodingKeys: String, CodingKey {
        case type
        case event
        case eventID = "event_id"
        case eventSequence = "event_sequence"
        case conversationID = "conversation_id"
        case payload
        case timestamp = "ts"
    }
}

struct RealtimePayloadDTO: Decodable, Sendable {
    var kind: String
    var conversationID: String?
    var turnID: String?
    var streamType: String?
    var raw: [String: JSONValue]?
    var promptID: String?
    var action: String?
    var status: String?

    enum CodingKeys: String, CodingKey {
        case kind
        case conversationID = "conversation_id"
        case turnID = "conversation_turn_id"
        case streamType = "stream_type"
        case raw
        case promptID = "prompt_id"
        case action, status
    }
}

extension RealtimeEventEnvelopeDTO {
    func signal(expectedSessionID: String) -> ConversationRealtimeSignal? {
        guard type == "event", let payload else { return nil }
        let sessionID = conversationID ?? payload.conversationID ?? ""
        guard sessionID == expectedSessionID else { return nil }
        if payload.kind == "ask_user_prompt", let promptID = payload.promptID {
            return ConversationRealtimeSignal(
                eventID: eventID,
                eventSequence: eventSequence,
                sessionID: sessionID,
                turnID: payload.turnID,
                kind: .unknown,
                eventName: event,
                timestamp: timestamp,
                askUserPromptUpdate: AskUserPromptRealtimeUpdate(
                    promptID: promptID,
                    sessionID: sessionID,
                    turnID: payload.turnID,
                    action: payload.action ?? "",
                    status: payload.status.flatMap { AskUserPromptStatus(rawValue: $0.lowercased()) }
                )
            )
        }
        guard payload.kind == "chat_stream" else { return nil }
        let normalizedType = (
            payload.raw?["type"]?.stringValue
                ?? payload.streamType
                ?? event
        ).lowercased()

        return ConversationRealtimeSignal(
            eventID: eventID,
            eventSequence: eventSequence,
            sessionID: sessionID,
            turnID: payload.turnID,
            kind: normalizedType.realtimeKind,
            eventName: event,
            timestamp: timestamp,
            processUpdate: processUpdate(
                normalizedType: normalizedType,
                raw: payload.raw ?? [:]
            )
        )
    }

    private func processUpdate(
        normalizedType: String,
        raw: [String: JSONValue]
    ) -> ConversationRealtimeProcessUpdate? {
        let eventType = normalizedType.replacingOccurrences(of: "-", with: "_")
        let title: String
        let detail: String?
        let status: String

        if eventType == "start" || event.contains("turn.started") {
            title = "AI 已开始生成执行计划"
            detail = "正在读取需求、技术文档和项目任务"
            status = "running"
        } else if eventType.contains("thinking") {
            title = "正在分析需求与任务依赖"
            detail = nil
            status = "running"
        } else if eventType.contains("turn_phase") || eventType == "phase" {
            title = "AI 进入新的处理阶段"
            detail = firstString(
                in: raw,
                paths: [["data", "phase"], ["data", "status"], ["data", "name"]]
            )
            status = "running"
        } else if eventType.contains("tools_start") || event.contains("tool.started") {
            let names = toolNames(in: raw)
            title = names.isEmpty ? "正在调用规划工具" : "正在调用工具：\(names.joined(separator: "、"))"
            detail = "正在读取上下文或创建任务节点"
            status = "running"
        } else if eventType.contains("tools_end") || eventType.contains("tool_completed")
                    || event.contains("tool.completed") {
            title = "工具调用已完成"
            detail = nil
            status = "completed"
        } else if eventType.contains("complete") || eventType.contains("finish") {
            title = "AI 执行计划已生成"
            detail = "正在同步任务流程图和确认状态"
            status = "completed"
        } else if eventType.contains("fail") || eventType.contains("error") {
            title = "AI 生成执行计划失败"
            detail = firstString(
                in: raw,
                paths: [["error"], ["message"], ["data", "error"], ["data", "message"]]
            )
            status = "failed"
        } else if eventType.contains("cancel") {
            title = "AI 生成过程已取消"
            detail = nil
            status = "cancelled"
        } else {
            return nil
        }

        return ConversationRealtimeProcessUpdate(
            id: eventID,
            title: title,
            detail: detail,
            status: status,
            timestamp: timestamp
        )
    }

    private func firstString(
        in raw: [String: JSONValue],
        paths: [[String]]
    ) -> String? {
        for path in paths {
            var value: JSONValue = .object(raw)
            for component in path {
                guard case let .object(object) = value,
                      let next = object[component] else {
                    value = .null
                    break
                }
                value = next
            }
            if let string = value.stringValue { return string }
        }
        return nil
    }

    private func toolNames(in raw: [String: JSONValue]) -> [String] {
        guard let calls = raw.value(at: "data", "tool_calls") else { return [] }
        let values: [JSONValue]
        switch calls {
        case let .array(items): values = items
        default: values = [calls]
        }
        var seen = Set<String>()
        return values.compactMap { value in
            guard case let .object(object) = value else { return nil }
            let name = object["name"]?.stringValue
                ?? object.value(at: "function", "name")?.stringValue
                ?? object["tool_name"]?.stringValue
            guard let name, seen.insert(name).inserted else { return nil }
            return name
        }
    }
}

private extension String {
    var realtimeKind: ConversationRealtimeKind {
        if contains("cancel") { return .cancelled }
        if contains("fail") || contains("error") { return .failed }
        if contains("complete") || contains("finish") || contains("final") { return .completed }
        if contains("persist") || contains("callback") { return .persisted }
        if contains("start") { return .started }
        if contains("delta") || contains("stream") || contains("update") { return .updated }
        return .unknown
    }
}
