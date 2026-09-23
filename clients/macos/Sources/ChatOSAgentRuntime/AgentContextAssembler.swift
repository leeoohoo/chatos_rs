import Foundation

enum AgentContextAssembler {
    static func assemble(checkpoint: AgentRunCheckpoint, memory: AgentMemoryCheckpoint,
                         context: AgentMemoryContext) throws -> [AgentMessage] {
        let all = checkpoint.messages
        let pins = memory.pinnedMessageCount
        guard pins > 0, pins <= all.count,
              all.prefix(pins).allSatisfy({ [.system, .user].contains($0.role) && $0.toolCalls.isEmpty && $0.toolCallID == nil }) else {
            throw AgentContextError.invalidHistory
        }
        let indices = Dictionary(uniqueKeysWithValues: all.indices.map { (memory.scope.recordID(at: $0), $0) })
        var selected = Set<Int>()
        var selectedRecordIDs = Set<String>()
        var retained: [AgentMemoryContextRecord] = []
        for record in context.recentRecords {
            guard selectedRecordIDs.insert(record.id).inserted else {
                throw AgentContextError.invalidHistory
            }
            if let index = indices[record.id] {
                guard messagesAreEquivalent(all[index], record.message),
                      selected.insert(index).inserted else {
                    throw AgentContextError.invalidHistory
                }
                // Current-run pinned instructions and trigger are appended from the authoritative
                // checkpoint below, so do not inject the composed copies a second time.
                if index >= pins { retained.append(record) }
                continue
            }
            guard memory.scope.allowsCrossRunHistory,
                  let location = memory.scope.recordLocation(for: record.id),
                  location.runID != memory.scope.runID else {
                throw AgentContextError.invalidHistory
            }
            retained.append(record)
        }
        if context.blocks.allSatisfy({ $0.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }) {
            guard Set(pins..<all.count).isSubset(of: selected) else { throw AgentContextError.invalidHistory }
        }

        // Match ContextualTurnRunner: fixed instructions first, Memory Engine's composed blocks
        // and recent records next, and the current task contract as sticky user input last.
        var messages = all.prefix(pins).filter { $0.role == .system }
        let blockText = context.blocks.map { "[\($0.blockType)]\n\($0.text)" }
            .joined(separator: "\n\n===\n\n")
        if !blockText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            messages.append(.init(role: .system, content: blockText))
        }
        messages.append(contentsOf: composeRecentRecords(retained))
        messages.append(contentsOf: all.prefix(pins).filter { $0.role == .user })
        return messages
    }

    /// Memory Engine transports may decode and encode the Responses output again. JSON object
    /// key ordering is not semantic, so comparing the raw `Data` would reject a valid checkpoint.
    /// Every other message field stays under strict equality, and malformed JSON remains invalid.
    private static func messagesAreEquivalent(_ lhs: AgentMessage, _ rhs: AgentMessage) -> Bool {
        guard lhs.role == rhs.role,
              lhs.content == rhs.content,
              lhs.toolCalls == rhs.toolCalls,
              lhs.toolCallID == rhs.toolCallID,
              lhs.usage == rhs.usage,
              lhs.attachments == rhs.attachments else {
            return false
        }
        switch (lhs.responseOutputJSON, rhs.responseOutputJSON) {
        case (nil, nil):
            return true
        case let (.some(left), .some(right)):
            guard let canonicalLeft = canonicalJSON(left),
                  let canonicalRight = canonicalJSON(right) else {
                return false
            }
            return canonicalLeft == canonicalRight
        case (.some, nil), (nil, .some):
            return false
        }
    }

    private static func canonicalJSON(_ data: Data) -> Data? {
        guard let object = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) else {
            return nil
        }
        return try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys, .fragmentsAllowed])
    }

    /// Chat-completions equivalent of `compose_response_to_input_items_with_budget`.
    /// Orphan tool outputs and calls without a retained output are omitted exactly as in the
    /// shared Rust runtime; complete call/result pairs keep their original IDs.
    private static func composeRecentRecords(_ records: [AgentMemoryContextRecord]) -> [AgentMessage] {
        var remainingOutputs: [String: Int] = [:]
        for record in records where record.message.role == .tool {
            if let id = record.message.toolCallID { remainingOutputs[id, default: 0] += 1 }
        }
        var seenCalls = Set<String>()
        var messages: [AgentMessage] = []
        for record in records {
            let message = record.message
            switch message.role {
            case .tool:
                guard let id = message.toolCallID else { continue }
                if seenCalls.contains(id) { messages.append(message) }
                if let count = remainingOutputs[id] {
                    if count <= 1 { remainingOutputs.removeValue(forKey: id) }
                    else { remainingOutputs[id] = count - 1 }
                }
            case .assistant:
                let calls = message.toolCalls.filter { (remainingOutputs[$0.id] ?? 0) > 0 }
                seenCalls.formUnion(calls.map(\.id))
                if !message.content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !calls.isEmpty {
                    messages.append(.init(role: .assistant, content: message.content, toolCalls: calls))
                }
            case .system, .user:
                if !message.content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    messages.append(message)
                }
            }
        }
        return messages
    }
}
