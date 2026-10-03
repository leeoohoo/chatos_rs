import ChatOSCore
import Foundation

public actor NativeLocalAgentAskUserPromptService: AskUserPromptServicing {
    private let runtimeClient: NativeLocalAgentRuntimeClient
    private let taskClient: NativeLocalAgentTaskClient
    private let conversationClient: NativeLocalAgentConversationClient
    private var ownerUserID: String?

    public init(host: any LocalAgentHostClientServicing) {
        runtimeClient = NativeLocalAgentRuntimeClient(host: host)
        taskClient = NativeLocalAgentTaskClient(host: host)
        conversationClient = NativeLocalAgentConversationClient(host: host)
    }

    public func configure(ownerUserID: String) {
        self.ownerUserID = ownerUserID
    }

    public func reset() {
        ownerUserID = nil
    }

    public func fetchPrompts(sessionID: String, limit: Int = 100) async throws -> [AskUserPrompt] {
        let ownerUserID = try configuredOwner()
        let normalizedLimit = max(1, min(limit, 500))
        let page = try await runtimeClient.listRuns(
            ownerUserID: ownerUserID,
            scope: "active",
            status: "waiting_user",
            limit: UInt32(min(normalizedLimit, 100))
        )
        var prompts: [AskUserPrompt] = []
        for run in page.runs where run.status == "waiting_user" {
            guard Self.context(for: run)?.conversationID == sessionID else { continue }
            if let prompt = try await prompt(for: run, ownerUserID: ownerUserID) {
                prompts.append(prompt)
                if prompts.count == normalizedLimit { break }
            }
        }
        return prompts
    }

    public func submit(
        promptID: String,
        sessionID: String,
        submission: AskUserSubmission
    ) async throws -> AskUserPrompt {
        let resolved = try await resolve(promptID: promptID, sessionID: sessionID)
        try Self.rejectSecrets(in: submission, prompt: resolved.prompt)
        let input = Self.submissionInput(submission, promptID: promptID)
        let message = Self.submissionMessage(submission)
        if resolved.run.profileKey == "main_chat" {
            let conversation = try await conversationClient.get(
                ownerUserID: resolved.run.ownerUserID,
                conversationID: resolved.context.conversationID
            )
            _ = try await conversationClient.resumeTurn(.init(
                ownerUserID: resolved.run.ownerUserID,
                conversationID: resolved.context.conversationID,
                expectedConversationVersion: conversation.conversation.version,
                turnID: resolved.context.turnID,
                expectedRunVersion: resolved.run.version,
                messageID: "local-ask-response-\(UUID().uuidString.lowercased())",
                message: message,
                messageMetadata: input
            ), expectedRunStatus: "waiting_user", reason: "ask_user_submitted")
        } else {
            _ = try await runtimeClient.resumeWaitingRun(
                ownerUserID: resolved.run.ownerUserID,
                runID: resolved.run.runID,
                expectedVersion: resolved.run.version,
                input: input,
                reason: "ask_user_submitted"
            )
        }
        return resolved.prompt.updating(status: .ok)
    }

    public func cancel(promptID: String, sessionID: String) async throws -> AskUserPrompt {
        let resolved = try await resolve(promptID: promptID, sessionID: sessionID)
        if resolved.run.profileKey == "main_chat" {
            let conversation = try await conversationClient.get(
                ownerUserID: resolved.run.ownerUserID,
                conversationID: resolved.context.conversationID
            )
            _ = try await conversationClient.cancelTurn(
                ownerUserID: resolved.run.ownerUserID,
                conversationID: resolved.context.conversationID,
                expectedConversationVersion: conversation.conversation.version,
                turnID: resolved.context.turnID,
                expectedRunVersion: resolved.run.version,
                reason: "user_cancelled"
            )
        } else {
            guard resolved.run.ownerEntityType == "task" else {
                throw NativeLocalAgentAskUserPromptError.unsupportedRun
            }
            _ = try await taskClient.cancel(
                ownerUserID: resolved.run.ownerUserID,
                taskID: resolved.run.ownerEntityID,
                expectedVersion: nil,
                reason: "user_cancelled"
            )
        }
        return resolved.prompt.updating(status: .canceled)
    }

    private func resolve(promptID: String, sessionID: String) async throws -> ResolvedPrompt {
        let ownerUserID = try configuredOwner()
        guard let runID = Self.runID(promptID: promptID) else {
            throw NativeLocalAgentAskUserPromptError.promptNotFound
        }
        let run = try await taskClient.run(ownerUserID: ownerUserID, runID: runID)
        guard run.status == "waiting_user",
              let context = Self.context(for: run),
              context.conversationID == sessionID,
              let prompt = try await prompt(for: run, ownerUserID: ownerUserID) else {
            throw NativeLocalAgentAskUserPromptError.promptNotFound
        }
        return ResolvedPrompt(run: run, context: context, prompt: prompt)
    }

    private func prompt(
        for run: LocalAgentRunRecord,
        ownerUserID: String
    ) async throws -> AskUserPrompt? {
        let page = try await taskClient.events(
            ownerUserID: ownerUserID,
            runID: run.runID,
            limit: 1,
            eventType: "user_input_requested",
            newestFirst: true
        )
        guard let event = page.events.first,
              let context = Self.context(for: run),
              let promptValue = event.payload?.object?["prompt"] else { return nil }
        return LocalAgentAskUserPromptMapper.map(
            promptValue,
            run: run,
            context: context,
            event: event
        )
    }

    private func configuredOwner() throws -> String {
        guard let ownerUserID else { throw NativeLocalAgentAskUserPromptError.notConfigured }
        return ownerUserID
    }

    private static func runID(promptID: String) -> String? {
        let prefix = "local-ask:"
        guard promptID.hasPrefix(prefix) else { return nil }
        let value = String(promptID.dropFirst(prefix.count))
        return value.isEmpty ? nil : value
    }

    private static func context(for run: LocalAgentRunRecord) -> LocalAskUserRunContext? {
        let conversationKey = run.profileKey == "main_chat"
            ? "conversation_id" : "source_conversation_id"
        let turnKey = run.profileKey == "main_chat" ? "turn_id" : "source_turn_id"
        guard let conversationID = run.input.string(conversationKey),
              let turnID = run.input.string(turnKey) else { return nil }
        return .init(conversationID: conversationID, turnID: turnID)
    }

    private static func rejectSecrets(
        in submission: AskUserSubmission,
        prompt: AskUserPrompt
    ) throws {
        let secretKeys = Set(prompt.fields.lazy.filter {
            $0.isSecret || isSensitiveKey($0.key)
        }.map(\.key))
        guard !submission.values.contains(where: {
            (secretKeys.contains($0.key) || isSensitiveKey($0.key)) && !$0.value.isEmpty
        }) else {
            throw NativeLocalAgentAskUserPromptError.secretSubmissionUnsupported
        }
    }

    private static func isSensitiveKey(_ key: String) -> Bool {
        let normalized = key.lowercased().filter(\.isLetter)
        return ["apikey", "accesstoken", "password", "passwd", "secret", "credential"]
            .contains(where: normalized.contains)
    }

    private static func submissionInput(
        _ submission: AskUserSubmission,
        promptID: String
    ) -> LocalAgentJSONValue {
        let values = submission.values.mapValues(LocalAgentJSONValue.string)
        let selection: LocalAgentJSONValue = switch submission.selection {
        case let .single(value): .string(value)
        case let .multiple(values): .array(values.map(LocalAgentJSONValue.string))
        case nil: .null
        }
        return .object([
            "source": .string("ask_user"),
            "prompt_id": .string(promptID),
            "values": .object(values),
            "selection": selection,
        ])
    }

    private static func submissionMessage(_ submission: AskUserSubmission) -> String {
        var lines = submission.values.sorted(by: { $0.key < $1.key }).map { "\($0.key): \($0.value)" }
        switch submission.selection {
        case let .single(value): lines.append(value)
        case let .multiple(values): lines.append(values.joined(separator: ", "))
        case nil: break
        }
        return lines.isEmpty ? "已提交回复" : lines.joined(separator: "\n")
    }
}

private struct ResolvedPrompt: Sendable {
    let run: LocalAgentRunRecord
    let context: LocalAskUserRunContext
    let prompt: AskUserPrompt
}

struct LocalAskUserRunContext: Sendable {
    let conversationID: String
    let turnID: String
}

public enum NativeLocalAgentAskUserPromptError: LocalizedError {
    case notConfigured
    case promptNotFound
    case unsupportedRun
    case secretSubmissionUnsupported

    public var errorDescription: String? {
        switch self {
        case .notConfigured: "Local Agent Ask User service is not configured."
        case .promptNotFound: "The local Ask User prompt is no longer pending."
        case .unsupportedRun: "The local Run cannot be cancelled through Ask User."
        case .secretSubmissionUnsupported:
            "Secret answers cannot be sent through Local Agent IPC or stored in its database."
        }
    }
}

private extension LocalAgentJSONValue {
    var object: [String: LocalAgentJSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }

    func string(_ key: String) -> String? {
        guard case let .string(value)? = object?[key], !value.isEmpty else { return nil }
        return value
    }
}
