// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation

public enum LocalAgentUIPresentation {
    public static func date(_ value: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: value) { return date }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: value)
    }

    public static func askUserPrompt(
        _ event: LocalAgentUserInteractionEvent,
        sessionID: String,
        turnID: String,
        emittedAt: String
    ) -> AskUserPrompt {
        let details = event.details?.objectValue
        let title = details?["title"]?.plainString?.nonEmpty ?? "需要你的确认"
        let kind = details?["kind"]?.plainString?.nonEmpty ?? "local_agent"
        let allowsCancel = details?["allows_cancel"]?.boolValue ?? true
        let allowsMultiple = details?["allows_multiple"]?.boolValue ?? false
        let options = event.options.map {
            AskUserChoiceOption(
                value: $0.optionID,
                label: $0.label,
                description: $0.description
            )
        }
        let choice = options.isEmpty ? nil : AskUserChoice(
            allowsMultiple: allowsMultiple,
            options: options,
            minimumSelectionCount: 1,
            maximumSelectionCount: allowsMultiple ? options.count : 1
        )
        let fields = options.isEmpty
            ? [AskUserField(
                key: "answer",
                label: "回复",
                placeholder: "告诉 AI 你的决定或补充信息",
                isRequired: true,
                isMultiline: true
            )]
            : []
        let timestamp = date(emittedAt)
        return AskUserPrompt(
            id: event.interactionID,
            sessionID: sessionID,
            turnID: turnID,
            kind: kind,
            status: .pending,
            title: title,
            message: event.prompt,
            allowsCancel: allowsCancel,
            fields: fields,
            choice: choice,
            createdAt: timestamp,
            updatedAt: timestamp
        )
    }

    /// Reconstructs the full Ask User event from the authoritative Run
    /// snapshot after an app restart. The Rust runtime persists the validated
    /// question inside `pending_interaction`; the UI event stream is only an
    /// incremental delivery mechanism and is not required for restoration.
    public static func pendingUserInteraction(
        _ run: LocalAgentRunSnapshot
    ) -> LocalAgentUserInteractionEvent? {
        guard run.status == .paused,
              let pending = run.pendingInteraction?.objectValue,
              pending["type"]?.plainString == "ask_user",
              let interactionID = pending["interaction_id"]?.plainString?.nonEmpty,
              let question = pending["question"]?.objectValue,
              let prompt = question["prompt"]?.plainString?.nonEmpty,
              let optionValues = question["options"]?.arrayValue,
              let imageValues = question["image_references"]?.arrayValue
        else { return nil }

        var options: [LocalAgentUserInteractionOption] = []
        options.reserveCapacity(optionValues.count)
        for value in optionValues {
            guard let option = value.objectValue,
                  let optionID = option["option_id"]?.plainString?.nonEmpty,
                  let label = option["label"]?.plainString?.nonEmpty
            else { return nil }
            options.append(LocalAgentUserInteractionOption(
                optionID: optionID,
                label: label,
                description: option["description"]?.plainString
            ))
        }

        var imageReferences: [String] = []
        imageReferences.reserveCapacity(imageValues.count)
        for value in imageValues {
            guard let reference = value.plainString?.nonEmpty else { return nil }
            imageReferences.append(reference)
        }

        let details: LocalAgentJSONValue?
        if case .null? = question["details"] {
            details = nil
        } else {
            details = question["details"]
        }
        return LocalAgentUserInteractionEvent(
            interactionID: interactionID,
            runID: run.runID,
            prompt: prompt,
            options: options,
            imageReferences: imageReferences,
            details: details
        )
    }

    public static func runControl(
        _ run: LocalAgentRunSnapshot,
        sessionID: String,
        turnID: String
    ) -> LocalAgentRunControlState {
        LocalAgentRunControlState(
            runID: run.runID,
            sessionID: sessionID,
            turnID: turnID,
            status: run.status,
            iteration: run.iteration,
            retryCount: run.retryCount,
            interactionKind: interactionKind(run.pendingInteraction),
            reviewReason: reviewReason(run.pendingInteraction),
            updatedAt: date(run.updatedAt)
        )
    }

    public static func toolApproval(
        _ tool: LocalAgentToolSnapshot,
        sessionID: String,
        turnID: String
    ) -> LocalAgentToolApprovalRequest {
        LocalAgentToolApprovalRequest(
            invocationID: tool.invocationID,
            runID: tool.runID,
            sessionID: sessionID,
            turnID: turnID,
            toolName: tool.toolName,
            effect: tool.effect,
            argumentsDigest: tool.argumentsDigest
        )
    }

    private static func reviewReason(_ interaction: LocalAgentJSONValue?) -> String? {
        guard case let .object(value) = interaction,
              let type = value["type"]?.plainString
        else { return nil }
        switch type {
        case "review_unknown_tool_outcome":
            let batchID = value["batch_id"]?.plainString
            return batchID.map {
                "工具批次 \($0) 的执行结果无法确认。继续前请核对外部结果，避免重复执行。"
            } ?? "工具执行结果无法确认。继续前请核对外部结果，避免重复执行。"
        case "runtime_blocked":
            return "本地 Agent 已阻塞，需要检查执行过程后再继续。"
        default:
            return nil
        }
    }

    private static func interactionKind(_ interaction: LocalAgentJSONValue?) -> String? {
        guard case let .object(value) = interaction else { return nil }
        return value["type"]?.plainString
    }
}

public extension LocalAgentJSONValue {
    func stringValue(forKey key: String) -> String? {
        guard case let .object(object) = self,
              case let .string(value)? = object[key]
        else { return nil }
        return value
    }

    var boolValue: Bool? {
        guard case let .bool(value) = self else { return nil }
        return value
    }

    var objectValue: [String: LocalAgentJSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }

    var arrayValue: [LocalAgentJSONValue]? {
        guard case let .array(value) = self else { return nil }
        return value
    }

    var plainString: String? {
        guard case let .string(value) = self else { return nil }
        return value
    }

    var integerValue: UInt64? {
        switch self {
        case let .unsigned(value): value
        case let .signed(value) where value >= 0: UInt64(value)
        default: nil
        }
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
