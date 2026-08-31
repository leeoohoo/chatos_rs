import ChatOSCore
import Foundation

public struct ChatOSMessageTaskGraphService: MessageTaskGraphServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetchGraph(
        messageID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTaskGraphSnapshot {
        let response: MessageTaskGraphResponseDTO = try await client.request(
            endpoint(messageID: messageID, suffix: "graph", lookup: lookup)
        )
        return response.model
    }

    public func fetchTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTask {
        let response: MessageTaskDTO = try await client.request(
            endpoint(
                messageID: messageID,
                suffix: "tasks/\(taskID.urlPathEncoded)",
                lookup: lookup
            )
        )
        return response.model
    }

    public func fetchRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail {
        let safeLimit = min(max(eventLimit, 1), 100)
        let safeOffset = max(eventOffset, 0)
        let response: MessageTaskRunDetailDTO = try await client.request(
            endpoint(
                messageID: messageID,
                suffix: "runs/\(runID.urlPathEncoded)",
                lookup: lookup,
                extraQuery: [
                    URLQueryItem(name: "include_events", value: includeEvents ? "true" : "false"),
                    URLQueryItem(name: "event_limit", value: String(safeLimit)),
                    URLQueryItem(name: "event_offset", value: String(safeOffset)),
                ]
            )
        )
        return response.model
    }

    public func retryRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        instruction: String?
    ) async throws -> MessageTaskRun {
        let response: MessageTaskRetryResponseDTO = try await client.request(
            endpoint(
                messageID: messageID,
                suffix: "runs/\(runID)/retry",
                lookup: lookup
            ),
            method: "POST",
            body: try JSONEncoder().encode(
                RetryRunRequestDTO(retryInstruction: instruction?.trimmedNonEmptyValue)
            )
        )
        guard response.success else {
            throw ChatOSAPIError.server(statusCode: 409, message: "任务重试未被接受")
        }
        return response.run.model
    }

    public func cancelTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?,
        reason: String?
    ) async throws {
        let response: MessageTaskCancelResponseDTO = try await client.request(
            endpoint(
                messageID: messageID,
                suffix: "tasks/\(taskID.urlPathEncoded)/cancel",
                lookup: lookup
            ),
            method: "POST",
            body: try JSONEncoder().encode(
                CancelMessageTaskRequestDTO(reason: reason?.trimmedNonEmptyValue)
            )
        )
        guard response.success else {
            throw ChatOSAPIError.server(statusCode: 409, message: "任务取消未被接受")
        }
    }

    private func endpoint(
        messageID: String,
        suffix: String,
        lookup: MessageTaskLookup?,
        extraQuery: [URLQueryItem] = []
    ) -> String {
        var components = URLComponents()
        components.path = "/messages/\(messageID.urlPathEncoded)/task-runner/\(suffix)"
        var items = extraQuery
        if let sessionID = lookup?.sessionID?.trimmedNonEmptyValue {
            items.append(URLQueryItem(name: "session_id", value: sessionID))
        }
        if let turnID = lookup?.turnID?.trimmedNonEmptyValue {
            items.append(URLQueryItem(name: "turn_id", value: turnID))
        }
        if let sourceID = lookup?.sourceUserMessageID?.trimmedNonEmptyValue {
            items.append(URLQueryItem(name: "source_user_message_id", value: sourceID))
        }
        components.queryItems = items.isEmpty ? nil : items
        return components.string ?? components.path
    }
}

private struct RetryRunRequestDTO: Encodable {
    var retryInstruction: String?

    enum CodingKeys: String, CodingKey {
        case retryInstruction = "retry_instruction"
    }
}

private struct CancelMessageTaskRequestDTO: Encodable {
    var reason: String?
}

private struct MessageTaskCancelResponseDTO: Decodable {
    var success: Bool
}
